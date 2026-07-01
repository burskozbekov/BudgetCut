//! Live presence over WebSocket (§20.4/§8): one client's ephemeral presence
//! message is fanned out to other subscribers of the same budget channel.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures_util::{SinkExt, StreamExt};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message;
use tower::ServiceExt;

async fn call(app: &axum::Router, uri: &str, token: Option<&str>, body: Value) -> Value {
    let mut rb = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(t) = token {
        rb = rb.header("authorization", format!("Bearer {t}"));
    }
    let resp = app
        .clone()
        .oneshot(rb.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn presence_is_broadcast_to_other_subscribers() {
    let app = budgetcut_server::app(budgetcut_server::AppState::new());

    // Set up over HTTP (shares the same in-memory state as the served app).
    let auth = call(
        &app,
        "/auth/register",
        None,
        json!({"email":"a@x.io","password":"hunter2"}),
    )
    .await;
    let token = auth["token"].as_str().unwrap().to_string();
    let b = call(
        &app,
        "/budgets",
        Some(&token),
        json!({"name":"B","seed_template":false}),
    )
    .await;
    let bid = b["id"].as_str().unwrap().to_string();

    // Serve the app on an ephemeral port.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let url = format!("ws://{addr}/budgets/{bid}/ws?token={token}");
    let (mut a, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("client A connect");
    let (mut bsock, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("client B connect");

    // Client A announces it is editing a cell.
    a.send(Message::Text(
        json!({"cell":"1301","editing":true}).to_string(),
    ))
    .await
    .unwrap();

    // Client B must receive A's presence within a short window.
    let received = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            if let Some(Ok(Message::Text(t))) = bsock.next().await {
                let v: Value = serde_json::from_str(&t).unwrap();
                if v["type"] == "presence" {
                    return v;
                }
            }
        }
    })
    .await
    .expect("client B should receive presence");

    assert_eq!(received["type"], "presence");
    assert_eq!(received["payload"]["cell"], "1301");
    assert_eq!(received["payload"]["editing"], true);
}
