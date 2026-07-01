//! Sync round-trip (§20.3): a client makes edits, pushes its op outbox to the
//! server, and a *second* client that pulls the snapshot replays the op log and
//! converges to byte-identical state and identical totals — through the real
//! HTTP server, using the same `budgetcut-core` reducer on both ends.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use budgetcut_core::ids::*;
use budgetcut_core::*;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

async fn call(
    app: &axum::Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut rb = Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        rb = rb.header("authorization", format!("Bearer {t}"));
    }
    let req = match body {
        Some(b) => rb
            .header("content-type", "application/json")
            .body(Body::from(b.to_string()))
            .unwrap(),
        None => rb.body(Body::empty()).unwrap(),
    };
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let val = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, val)
}

#[tokio::test]
async fn offline_edits_sync_and_two_clients_converge() {
    let app = budgetcut_server::app(budgetcut_server::AppState::new());

    let (_, ow) = call(
        &app,
        "POST",
        "/auth/register",
        None,
        Some(json!({"email":"o@x.io","password":"hunter2"})),
    )
    .await;
    let token = ow["token"].as_str().unwrap().to_string();

    let (_, b) = call(
        &app,
        "POST",
        "/budgets",
        Some(&token),
        Some(json!({"name":"Sync Dizi","seed_template":false})),
    )
    .await;
    let bid = b["id"].as_str().unwrap().to_string();

    // Pull the initial (empty) base both clients start from.
    let (_, snap0) = call(
        &app,
        "GET",
        &format!("/budgets/{bid}/snapshot"),
        Some(&token),
        None,
    )
    .await;
    let base: Budget = serde_json::from_value(snap0["base"].clone()).unwrap();

    // CLIENT 1 edits offline: builds ops, applies them locally, queues an outbox.
    let author = UserId::new();
    let mut clock = HlcClock::new(author);
    let mut client1 = Document::new(base.clone());

    let cat = Category {
        id: CategoryId::new(),
        number: "1300".into(),
        description: Localized::tr("YÖNETMENLER"),
        position: rust_decimal::Decimal::ONE,
        atl_btl: Some(AtlBtl::Atl),
        applied_fringes: vec![],
    };
    let acc = Account {
        id: AccountId::new(),
        category: cat.id,
        number: "1301".into(),
        description: Localized::tr("YÖNETMEN"),
        position: rust_decimal::Decimal::ONE,
        show_subtotal: true,
        applied_fringes: vec![],
    };
    let det = Detail {
        id: DetailId::new(),
        account: acc.id,
        position: rust_decimal::Decimal::ONE,
        description: "Yönetmen".into(),
        name: None,
        amount: Formula::Const(rust_decimal::Decimal::from(1)),
        multiplier: Formula::Const(rust_decimal::Decimal::ONE),
        rate: Formula::Const(rust_decimal::Decimal::from(660000)),
        unit: UnitId::new(),
        currency: base.base_currency,
        applied_fringes: vec![],
        groups: vec![],
        location: None,
        set: None,
        gl_code: None,
        notes: None,
    };

    let mut outbox = vec![];
    for kind in [
        OpKind::InsertCategory(cat.clone()),
        OpKind::InsertAccount(acc.clone()),
        OpKind::InsertDetail(det.clone()),
    ] {
        let op = Op::new(
            clock.tick(1_700_000_000_000 + outbox.len() as u64),
            author,
            kind,
        );
        client1.apply(&op);
        outbox.push(op);
    }

    // Reconnect: flush the outbox to the server (each op validated + applied authoritatively).
    for op in &outbox {
        let (st, _) = call(
            &app,
            "POST",
            &format!("/budgets/{bid}/ops"),
            Some(&token),
            Some(json!({"op": serde_json::to_value(op).unwrap()})),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
    }

    // CLIENT 2 pulls the snapshot and replays the op log.
    let (_, snap) = call(
        &app,
        "GET",
        &format!("/budgets/{bid}/snapshot"),
        Some(&token),
        None,
    )
    .await;
    let base2: Budget = serde_json::from_value(snap["base"].clone()).unwrap();
    let ops2: Vec<Op> = serde_json::from_value(snap["ops"].clone()).unwrap();
    let mut client2 = Document::new(base2);
    for op in &ops2 {
        client2.apply(op);
    }

    // Convergence: identical materialized state and identical totals.
    assert_eq!(ops2.len(), 3);
    assert_eq!(client1.budget, client2.budget, "clients did not converge");
    assert_eq!(
        evaluate(&client1.budget).net_total,
        evaluate(&client2.budget).net_total
    );
    assert_eq!(
        evaluate(&client2.budget).net_total,
        rust_decimal::Decimal::from(660000)
    );
}
