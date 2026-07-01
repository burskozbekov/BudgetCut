//! End-to-end RBAC over HTTP (§20.4): a Viewer cannot write (server-rejected,
//! not just UI-hidden), a Department Editor is confined to its categories, and
//! a Department Viewer's snapshot is server-filtered to its scope.

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
    let val: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, val)
}

async fn register(app: &axum::Router, email: &str) -> (String, String) {
    let (st, v) = call(
        app,
        "POST",
        "/auth/register",
        None,
        Some(json!({"email": email, "password": "hunter2"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "register failed: {v}");
    (
        v["token"].as_str().unwrap().to_string(),
        v["user_id"].as_str().unwrap().to_string(),
    )
}

fn category(num: &str) -> Category {
    Category {
        id: CategoryId::new(),
        number: num.into(),
        description: Localized::tr(num),
        position: rust_decimal::Decimal::ONE,
        atl_btl: None,
        applied_fringes: vec![],
    }
}
fn account(cat: CategoryId, num: &str) -> Account {
    Account {
        id: AccountId::new(),
        category: cat,
        number: num.into(),
        description: Localized::tr(num),
        position: rust_decimal::Decimal::ONE,
        show_subtotal: true,
        applied_fringes: vec![],
    }
}

async fn submit(app: &axum::Router, bid: &str, token: &str, kind: OpKind) -> StatusCode {
    call(
        app,
        "POST",
        &format!("/budgets/{bid}/ops"),
        Some(token),
        Some(json!({ "op": op_inner(kind) })),
    )
    .await
    .0
}

// Build an Op value but keep the kind's ids stable by constructing the Op here.
fn op_inner(kind: OpKind) -> Value {
    serde_json::to_value(Op::new(Hlc::new(1, 0, UserId::new()), UserId::new(), kind)).unwrap()
}

#[tokio::test]
async fn viewer_cannot_write_but_editor_can() {
    let app = budgetcut_server::app(budgetcut_server::AppState::new());
    let (owner_t, _) = register(&app, "owner@x.io").await;
    let (_viewer_t, viewer_id) = register(&app, "viewer@x.io").await;

    // Owner creates a budget.
    let (_, v) = call(
        &app,
        "POST",
        "/budgets",
        Some(&owner_t),
        Some(json!({"name":"B","seed_template":false})),
    )
    .await;
    let bid = v["id"].as_str().unwrap().to_string();

    // Owner builds a category + account.
    let cat = category("2200");
    let acc = account(cat.id, "2201");
    assert_eq!(
        submit(&app, &bid, &owner_t, OpKind::InsertCategory(cat.clone())).await,
        StatusCode::OK
    );
    assert_eq!(
        submit(&app, &bid, &owner_t, OpKind::InsertAccount(acc.clone())).await,
        StatusCode::OK
    );

    // Add the second user as a Viewer.
    let viewer_t = {
        let (st, _) = call(
            &app,
            "POST",
            &format!("/budgets/{bid}/members"),
            Some(&owner_t),
            Some(json!({"user_id": viewer_id, "role": "viewer"})),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        register_login(&app, "viewer@x.io").await
    };

    // Viewer tries to write -> server REJECTS (403), not just UI-hidden.
    let st = submit(&app, &bid, &viewer_t, OpKind::RemoveAccount(acc.id)).await;
    assert_eq!(
        st,
        StatusCode::FORBIDDEN,
        "viewer write must be server-rejected"
    );
}

async fn register_login(app: &axum::Router, email: &str) -> String {
    let (st, v) = call(
        app,
        "POST",
        "/auth/login",
        None,
        Some(json!({"email": email, "password":"hunter2"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    v["token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn department_editor_confined_and_viewer_snapshot_filtered() {
    let app = budgetcut_server::app(budgetcut_server::AppState::new());
    let (owner_t, _) = register(&app, "o2@x.io").await;
    let (_dt, dept_id) = register(&app, "art@x.io").await;

    let (_, v) = call(
        &app,
        "POST",
        "/budgets",
        Some(&owner_t),
        Some(json!({"name":"B","seed_template":false})),
    )
    .await;
    let bid = v["id"].as_str().unwrap().to_string();

    // Two departments: ART (2200) and CAMERA (3100), each with an account + detail.
    let art = category("2200");
    let cam = category("3100");
    let art_acc = account(art.id, "2201");
    let cam_acc = account(cam.id, "3101");
    for k in [
        OpKind::InsertCategory(art.clone()),
        OpKind::InsertCategory(cam.clone()),
        OpKind::InsertAccount(art_acc.clone()),
        OpKind::InsertAccount(cam_acc.clone()),
    ] {
        assert_eq!(submit(&app, &bid, &owner_t, k).await, StatusCode::OK);
    }

    // Add the dept user as a Department EDITOR scoped to ART only.
    let (st, _) = call(
        &app,
        "POST",
        &format!("/budgets/{bid}/members"),
        Some(&owner_t),
        Some(
            json!({"user_id": dept_id, "role": "department_editor", "scope": [art.id.to_string()]}),
        ),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let dept_t = register_login(&app, "art@x.io").await;

    // Build the two detail ops with KNOWN ids so we can check the snapshot.
    let art_detail = detail(art_acc.id);
    let cam_detail = detail(cam_acc.id);

    // In-scope write (ART) -> allowed. Submit via raw op value to keep the id.
    let st = call(&app, "POST", &format!("/budgets/{bid}/ops"), Some(&dept_t),
        Some(json!({"op": serde_json::to_value(Op::new(Hlc::new(2,0,UserId::new()), UserId::new(), OpKind::InsertDetail(art_detail.clone()))).unwrap()}))).await.0;
    assert_eq!(st, StatusCode::OK, "dept editor must edit its own category");

    // Out-of-scope write (CAMERA) -> server REJECTS.
    let st = call(&app, "POST", &format!("/budgets/{bid}/ops"), Some(&dept_t),
        Some(json!({"op": serde_json::to_value(Op::new(Hlc::new(3,0,UserId::new()), UserId::new(), OpKind::InsertDetail(cam_detail.clone()))).unwrap()}))).await.0;
    assert_eq!(
        st,
        StatusCode::FORBIDDEN,
        "dept editor must NOT edit other categories"
    );

    // Owner adds a CAMERA detail (so the camera line exists in the log).
    assert_eq!(
        submit(
            &app,
            &bid,
            &owner_t,
            OpKind::InsertDetail(cam_detail.clone())
        )
        .await,
        StatusCode::OK
    );

    // Department user's snapshot must be server-FILTERED: it sees the ART
    // detail but never the CAMERA detail.
    let (_, snap) = call(
        &app,
        "GET",
        &format!("/budgets/{bid}/snapshot"),
        Some(&dept_t),
        None,
    )
    .await;
    let ops = snap["ops"].as_array().unwrap();
    let mentions = |needle: &str| ops.iter().any(|o| o.to_string().contains(needle));
    assert!(
        mentions(&art_detail.id.to_string()),
        "should see own ART detail"
    );
    assert!(
        !mentions(&cam_detail.id.to_string()),
        "must NOT see CAMERA detail (server-filtered)"
    );
}

fn detail(acc: AccountId) -> Detail {
    Detail {
        id: DetailId::new(),
        account: acc,
        position: rust_decimal::Decimal::ONE,
        description: "x".into(),
        name: None,
        amount: Formula::Const(rust_decimal::Decimal::ONE),
        multiplier: Formula::Const(rust_decimal::Decimal::ONE),
        rate: Formula::Const(rust_decimal::Decimal::from(1000)),
        unit: UnitId::new(),
        currency: CurrencyId::new(),
        applied_fringes: vec![],
        groups: vec![],
        location: None,
        set: None,
        gl_code: None,
        notes: None,
    }
}
