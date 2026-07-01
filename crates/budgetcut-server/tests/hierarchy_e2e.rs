//! Org→Project→Budget RBAC cascade (§9/§20.1), refresh-token rotation + reuse
//! detection (§9), and budget Versions via duplicate (§5) — over HTTP.

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
    let st = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let v = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (st, v)
}

async fn reg(app: &axum::Router, email: &str) -> Value {
    let (st, v) = call(
        app,
        "POST",
        "/auth/register",
        None,
        Some(json!({"email": email, "password": "hunter2"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "register: {v}");
    v
}

fn detail(acc: AccountId) -> OpKind {
    OpKind::InsertDetail(Detail {
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
    })
}

fn op(kind: OpKind) -> Value {
    serde_json::to_value(Op::new(Hlc::new(1, 0, UserId::new()), UserId::new(), kind)).unwrap()
}

#[tokio::test]
async fn refresh_token_rotates_and_detects_reuse() {
    let app = budgetcut_server::app(budgetcut_server::AppState::new());
    let v = reg(&app, "r@x.io").await;
    let t0 = v["refresh_token"].as_str().unwrap().to_string();

    // Rotate once: t0 -> t1 (t0 is now spent).
    let (st, v1) = call(
        &app,
        "POST",
        "/auth/refresh",
        None,
        Some(json!({"refresh_token": t0})),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let t1 = v1["refresh_token"].as_str().unwrap().to_string();
    assert!(v1["token"].is_string());

    // Reusing t0 is detected -> 401, and revokes the whole family.
    let (st, _) = call(
        &app,
        "POST",
        "/auth/refresh",
        None,
        Some(json!({"refresh_token": t0})),
    )
    .await;
    assert_eq!(
        st,
        StatusCode::UNAUTHORIZED,
        "reuse of an old refresh token must fail"
    );

    // ...which also revokes the rotated token t1.
    let (st, _) = call(
        &app,
        "POST",
        "/auth/refresh",
        None,
        Some(json!({"refresh_token": t1})),
    )
    .await;
    assert_eq!(st, StatusCode::UNAUTHORIZED, "family revoked after reuse");
}

#[tokio::test]
async fn org_project_budget_rbac_cascades() {
    let app = budgetcut_server::app(budgetcut_server::AppState::new());
    let owner = reg(&app, "owner@x.io").await;
    let owner_t = owner["token"].as_str().unwrap().to_string();
    let editor = reg(&app, "editor@x.io").await;
    let editor_id = editor["user_id"].as_str().unwrap().to_string();
    let editor_t = editor["token"].as_str().unwrap().to_string();
    let stranger = reg(&app, "stranger@x.io").await;
    let stranger_t = stranger["token"].as_str().unwrap().to_string();

    // Org -> Project -> Budget, all created by the owner.
    let (_, o) = call(
        &app,
        "POST",
        "/orgs",
        Some(&owner_t),
        Some(json!({"name":"Prod Co"})),
    )
    .await;
    let oid = o["id"].as_str().unwrap();
    let (_, p) = call(
        &app,
        "POST",
        &format!("/orgs/{oid}/projects"),
        Some(&owner_t),
        Some(json!({"name":"Dizi"})),
    )
    .await;
    let pid = p["id"].as_str().unwrap();

    // Grant the editor an Editor role at the PROJECT level.
    let (st, _) = call(
        &app,
        "POST",
        &format!("/projects/{pid}/members"),
        Some(&owner_t),
        Some(json!({"user_id": editor_id, "role":"editor"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK);

    let (_, b) = call(
        &app,
        "POST",
        &format!("/projects/{pid}/budgets"),
        Some(&owner_t),
        Some(json!({"name":"Bölüm 1","seed_template":false})),
    )
    .await;
    let bid = b["id"].as_str().unwrap().to_string();

    // Build a category+account as owner (org owner cascades to the budget).
    let cat = Category {
        id: CategoryId::new(),
        number: "1300".into(),
        description: Localized::tr(""),
        position: rust_decimal::Decimal::ONE,
        atl_btl: Some(AtlBtl::Atl),
        applied_fringes: vec![],
    };
    let acc = Account {
        id: AccountId::new(),
        category: cat.id,
        number: "1301".into(),
        description: Localized::tr(""),
        position: rust_decimal::Decimal::ONE,
        show_subtotal: true,
        applied_fringes: vec![],
    };
    for k in [
        OpKind::InsertCategory(cat.clone()),
        OpKind::InsertAccount(acc.clone()),
    ] {
        let (st, _) = call(
            &app,
            "POST",
            &format!("/budgets/{bid}/ops"),
            Some(&owner_t),
            Some(json!({"op": op(k)})),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
    }

    // The project Editor can write the budget (role cascaded down the tree).
    let (st, _) = call(
        &app,
        "POST",
        &format!("/budgets/{bid}/ops"),
        Some(&editor_t),
        Some(json!({"op": op(detail(acc.id))})),
    )
    .await;
    assert_eq!(
        st,
        StatusCode::OK,
        "project editor should edit budgets in the project"
    );

    // A user with no org/project/budget membership is rejected.
    let (st, _) = call(
        &app,
        "POST",
        &format!("/budgets/{bid}/ops"),
        Some(&stranger_t),
        Some(json!({"op": op(detail(acc.id))})),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN, "non-member must be rejected");
}

#[tokio::test]
async fn duplicate_creates_an_independent_version() {
    let app = budgetcut_server::app(budgetcut_server::AppState::new());
    let owner = reg(&app, "v@x.io").await;
    let t = owner["token"].as_str().unwrap().to_string();

    let (_, b) = call(
        &app,
        "POST",
        "/budgets",
        Some(&t),
        Some(json!({"name":"v1","seed_template":false})),
    )
    .await;
    let bid = b["id"].as_str().unwrap().to_string();
    let cat = Category {
        id: CategoryId::new(),
        number: "1300".into(),
        description: Localized::tr(""),
        position: rust_decimal::Decimal::ONE,
        atl_btl: None,
        applied_fringes: vec![],
    };
    let acc = Account {
        id: AccountId::new(),
        category: cat.id,
        number: "1301".into(),
        description: Localized::tr(""),
        position: rust_decimal::Decimal::ONE,
        show_subtotal: true,
        applied_fringes: vec![],
    };
    for k in [
        OpKind::InsertCategory(cat.clone()),
        OpKind::InsertAccount(acc.clone()),
        detail(acc.id),
    ] {
        call(
            &app,
            "POST",
            &format!("/budgets/{bid}/ops"),
            Some(&t),
            Some(json!({"op": op(k)})),
        )
        .await;
    }

    // Duplicate -> a new budget whose base is the current materialized state.
    let (st, d) = call(
        &app,
        "POST",
        &format!("/budgets/{bid}/duplicate"),
        Some(&t),
        Some(json!({"name":"v2 (alternatif)"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let new_bid = d["id"].as_str().unwrap().to_string();
    assert_ne!(new_bid, bid);

    let (_, snap) = call(
        &app,
        "GET",
        &format!("/budgets/{new_bid}/snapshot"),
        Some(&t),
        None,
    )
    .await;
    let base: Budget = serde_json::from_value(snap["base"].clone()).unwrap();
    assert_eq!(base.name, "v2 (alternatif)");
    assert_eq!(
        base.details.len(),
        1,
        "the duplicate carries the materialized detail"
    );
    assert_eq!(
        snap["ops"].as_array().unwrap().len(),
        0,
        "a fresh version starts with an empty op log"
    );
}
