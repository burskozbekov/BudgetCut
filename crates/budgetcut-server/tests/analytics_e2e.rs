//! MMB-parity analytics endpoints end-to-end (§ feature-parity pass): amort/
//! pattern series, incentive estimation, budget comparison, reusable setup
//! libraries, and the accounting CSV — through the real HTTP router, with the
//! same `budgetcut-core` math the client would use. RBAC is enforced.

use axum::body::Body;
use axum::http::{Request, StatusCode};
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

async fn call_text(app: &axum::Router, uri: &str, token: &str) -> (StatusCode, String) {
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

async fn register(app: &axum::Router, email: &str) -> String {
    let (_, v) = call(
        app,
        "POST",
        "/auth/register",
        None,
        Some(json!({"email": email, "password": "hunter2"})),
    )
    .await;
    v["token"].as_str().unwrap().to_string()
}

async fn create_seeded(app: &axum::Router, token: &str, name: &str) -> String {
    let (_, v) = call(
        app,
        "POST",
        "/budgets",
        Some(token),
        Some(json!({"name": name, "seed_template": true})),
    )
    .await;
    v["id"].as_str().unwrap().to_string()
}

/// Add a director line (1 × 660000, +17% stopaj gross-up) so net > 0.
async fn add_director(app: &axum::Router, token: &str, bid: &str) {
    let (_, tree) = call(
        app,
        "GET",
        &format!("/budgets/{bid}/tree"),
        Some(token),
        None,
    )
    .await;
    let mut acc_id = None;
    for c in tree["categories"].as_array().unwrap() {
        for a in c["accounts"].as_array().unwrap() {
            if a["number"] == "1301" {
                acc_id = Some(a["id"].as_str().unwrap().to_string());
            }
        }
    }
    let acc = acc_id.expect("account 1301 in template");
    let (_, line) = call(
        app,
        "POST",
        &format!("/budgets/{bid}/lines"),
        Some(token),
        Some(json!({"account": acc})),
    )
    .await;
    let did = line["id"].as_str().unwrap().to_string();
    call(
        app,
        "POST",
        &format!("/budgets/{bid}/details/{did}/field"),
        Some(token),
        Some(json!({"field":"rate","value":"660000"})),
    )
    .await;
    call(
        app,
        "POST",
        &format!("/budgets/{bid}/details/{did}/fringes"),
        Some(token),
        Some(json!({"fringes":[{"code":"TR_STOPAJ","rate":"0.17"}]})),
    )
    .await;
}

#[tokio::test]
async fn series_incentive_compare_library_accounting_end_to_end() {
    let app = budgetcut_server::app(budgetcut_server::AppState::new());
    let owner = register(&app, "owner@x.io").await;
    let a = create_seeded(&app, &owner, "İstanbul").await;
    add_director(&app, &owner, &a).await;

    // --- A) Amort & pattern series: pattern episode = net (795180.72) ---
    let (st, s) = call(
        &app,
        "POST",
        &format!("/budgets/{a}/series"),
        Some(&owner),
        Some(json!({"episodes": 8, "amortized": [{"label":"Dekor","total":"1200000","over_episodes":8}]})),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(s["episodes"], 8);
    assert_eq!(s["pattern_episode"], "795180.72");
    assert_eq!(s["pattern_total"], "6361445.76"); // 795180.72 × 8
    assert_eq!(s["amort_total"], "1200000.00");
    assert_eq!(s["series_total"], "7561445.76");

    // --- C) Incentive: qualifying defaults to net; T.C. preset is 30% ---
    let (st, inc) = call(
        &app,
        "GET",
        &format!("/budgets/{a}/incentives"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(inc["qualifying_spend"], "795180.72");
    let first = &inc["lines"][0];
    assert_eq!(first["rate"], "0.3"); // fraction; UI renders as 30%
    assert_eq!(first["estimate"], "238554.22"); // 795180.72 × 0.30, half-up

    // --- B) Comparison: İstanbul vs a cheaper duplicate ---
    let (_, dup) = call(
        &app,
        "POST",
        &format!("/budgets/{a}/duplicate"),
        Some(&owner),
        Some(json!({"name":"Kapadokya"})),
    )
    .await;
    let b = dup["id"].as_str().unwrap().to_string();
    let (st, cmp) = call(
        &app,
        "GET",
        &format!("/compare?a={a}&b={b}"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(cmp["a_name"], "İstanbul");
    assert_eq!(cmp["b_name"], "Kapadokya");
    assert_eq!(cmp["diff"], "0.00"); // identical duplicate

    // --- E) Library: extract from a, apply to a fresh (unseeded) budget ---
    let (st, lib) = call(
        &app,
        "POST",
        "/libraries",
        Some(&owner),
        Some(json!({"budget_id": a, "name":"TR Standart"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let lib_id = lib["id"].as_str().unwrap().to_string();
    let (_, libs) = call(&app, "GET", "/libraries", Some(&owner), None).await;
    assert_eq!(libs.as_array().unwrap().len(), 1);
    assert!(libs[0]["fringes"].as_u64().unwrap() >= 2); // TR_STOPAJ + TR_KOMISYON

    let (_, blank) = call(
        &app,
        "POST",
        "/budgets",
        Some(&owner),
        Some(json!({"name":"Boş","seed_template": false})),
    )
    .await;
    let blank_id = blank["id"].as_str().unwrap().to_string();
    let (st, applied) = call(
        &app,
        "POST",
        &format!("/budgets/{blank_id}/libraries/{lib_id}/apply"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert!(applied["added_fringes"].as_u64().unwrap() >= 2);

    // --- Accounting CSV: real text/csv with the grossed-up director ---
    let (st, csv) = call_text(&app, &format!("/budgets/{a}/accounting.csv"), &owner).await;
    assert_eq!(st, StatusCode::OK);
    assert!(csv.contains("account_number,category_number"));
    assert!(csv.contains("795180.72"), "csv was:\n{csv}");
}

#[tokio::test]
async fn analytics_endpoints_enforce_rbac() {
    let app = budgetcut_server::app(budgetcut_server::AppState::new());
    let owner = register(&app, "owner2@x.io").await;
    let stranger = register(&app, "stranger@x.io").await;
    let a = create_seeded(&app, &owner, "Gizli").await;

    // Non-member cannot run series, incentives, compare, accounting, or save lib.
    let (st, _) = call(
        &app,
        "POST",
        &format!("/budgets/{a}/series"),
        Some(&stranger),
        Some(json!({"episodes": 6, "amortized": []})),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    let (st, _) = call(
        &app,
        "GET",
        &format!("/budgets/{a}/incentives"),
        Some(&stranger),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    let (st, _) = call(
        &app,
        "GET",
        &format!("/compare?a={a}&b={a}"),
        Some(&stranger),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    let (st, _) = call_text(&app, &format!("/budgets/{a}/accounting.csv"), &stranger).await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    let (st, _) = call(
        &app,
        "POST",
        "/libraries",
        Some(&stranger),
        Some(json!({"budget_id": a, "name":"çalıntı"})),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    // Actuals: non-member can neither read nor record.
    let (st, _) = call(
        &app,
        "GET",
        &format!("/budgets/{a}/actuals"),
        Some(&stranger),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, _) = call(
        &app,
        "POST",
        &format!("/budgets/{a}/actuals"),
        Some(&stranger),
        Some(json!({"account": a, "net":"1000"})),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn actuals_record_variance_efc_and_delete() {
    let app = budgetcut_server::app(budgetcut_server::AppState::new());
    let owner = register(&app, "owner4@x.io").await;
    let a = create_seeded(&app, &owner, "Aktüel Dizi").await;
    add_director(&app, &owner, &a).await; // account 1301 estimate = grossed-up 795180.72

    // Find account 1301's id.
    let (_, tree) = call(
        &app,
        "GET",
        &format!("/budgets/{a}/tree"),
        Some(&owner),
        None,
    )
    .await;
    let mut acc = String::new();
    for c in tree["categories"].as_array().unwrap() {
        for ac in c["accounts"].as_array().unwrap() {
            if ac["number"] == "1301" {
                acc = ac["id"].as_str().unwrap().to_string();
            }
        }
    }
    assert!(!acc.is_empty());

    // Record an invoice: net 660000, stopaj 17%, KDV 20% → brüt 795180.72.
    let (st, rec) = call(
        &app,
        "POST",
        &format!("/budgets/{a}/actuals"),
        Some(&owner),
        Some(json!({"account": acc, "vendor":"Yönetmen Ltd", "net":"660000", "stopaj_rate":"0.17", "kdv_rate":"0.20"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let actual_id = rec["id"].as_str().unwrap().to_string();

    let (st, rep) = call(
        &app,
        "GET",
        &format!("/budgets/{a}/actuals"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let row = rep["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["account_number"] == "1301")
        .expect("a variance row for 1301");
    assert_eq!(row["estimate"], "795180.72");
    assert_eq!(row["actual"], "795180.72"); // brüt cost matches the grossed-up estimate
    assert_eq!(row["variance"], "0.00"); // on budget
    assert_eq!(row["efc"], "795180.72");
    assert_eq!(row["over"], false);
    // Invoice breakdown line carries the FATURA tax math.
    let line = &rep["lines"][0];
    assert_eq!(line["net"], "660000.00");
    assert_eq!(line["brut"], "795180.72");
    assert_eq!(line["stopaj"], "135180.72");

    // Delete → the actual is gone; the account still shows its estimate.
    let (st, _) = call(
        &app,
        "POST",
        &format!("/budgets/{a}/actuals/{actual_id}/delete"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let (_, rep2) = call(
        &app,
        "GET",
        &format!("/budgets/{a}/actuals"),
        Some(&owner),
        None,
    )
    .await;
    assert!(rep2["lines"].as_array().unwrap().is_empty());
    let row2 = rep2["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["account_number"] == "1301")
        .expect("estimate row remains after deleting the actual");
    assert_eq!(row2["actual"], "0.00");
}

#[tokio::test]
async fn settlement_records_receipts_reconciles_and_rbac() {
    let app = budgetcut_server::app(budgetcut_server::AppState::new());
    let owner = register(&app, "owner5@x.io").await;
    let stranger = register(&app, "stranger5@x.io").await;
    let a = create_seeded(&app, &owner, "Hesap Kapama").await;

    // Three KDV-inclusive receipts in the uber sheet's tax shapes.
    for (cat, gross, rate) in [
        ("YEMEK%10", "790", "0.10"),
        ("SANAT-DEKOR%20", "5900", "0.20"),
        ("YEMEK%1", "1500", "0.01"),
    ] {
        let (st, _) = call(
            &app,
            "POST",
            &format!("/budgets/{a}/receipts"),
            Some(&owner),
            Some(json!({"category": cat, "gross": gross, "kdv_rate": rate, "vendor":"ÇAĞLAR"})),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
    }

    let (st, rep) = call(
        &app,
        "GET",
        &format!("/budgets/{a}/settlement?advance=10000"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(rep["gross_total"], "8190.00"); // 790 + 5900 + 1500
    assert_eq!(rep["kdv_total"], "1070.00"); // 71.82 + 983.33 + 14.85
    assert_eq!(rep["net_total"], "7120.00");
    assert_eq!(rep["balance"], "1810.00"); // 10000 − 8190 to refund
    assert_eq!(rep["refund"], true);
    assert_eq!(rep["lines"].as_array().unwrap().len(), 3);
    let sanat = rep["categories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["category"] == "SANAT-DEKOR%20")
        .unwrap();
    assert_eq!(sanat["net"], "4916.67"); // 5900 incl. %20

    // RBAC: a non-member can neither read nor record receipts.
    let (st, _) = call(
        &app,
        "GET",
        &format!("/budgets/{a}/settlement"),
        Some(&stranger),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, _) = call(
        &app,
        "POST",
        &format!("/budgets/{a}/receipts"),
        Some(&stranger),
        Some(json!({"category":"X","gross":"100"})),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
}

async fn account_1301(app: &axum::Router, token: &str, bid: &str) -> String {
    let (_, tree) = call(
        app,
        "GET",
        &format!("/budgets/{bid}/tree"),
        Some(token),
        None,
    )
    .await;
    for c in tree["categories"].as_array().unwrap() {
        for ac in c["accounts"].as_array().unwrap() {
            if ac["number"] == "1301" {
                return ac["id"].as_str().unwrap().to_string();
            }
        }
    }
    panic!("account 1301 not found");
}

#[tokio::test]
async fn schedule_day_out_of_days_and_rbac() {
    let app = budgetcut_server::app(budgetcut_server::AppState::new());
    let owner = register(&app, "owner6@x.io").await;
    let stranger = register(&app, "stranger6@x.io").await;
    let a = create_seeded(&app, &owner, "Plan").await;

    for (day, els) in [
        (1, "ANA KARAKTER,YARDIMCI"),
        (2, "YARDIMCI"),
        (3, "ANA KARAKTER"),
    ] {
        let (st, _) = call(
            &app,
            "POST",
            &format!("/budgets/{a}/strips"),
            Some(&owner),
            Some(json!({"day": day, "scene": format!("S{day}"), "eighths": 8,
                "elements": els.split(',').collect::<Vec<_>>()})),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
    }

    let (st, sched) = call(
        &app,
        "GET",
        &format!("/budgets/{a}/schedule"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(sched["total_days"], 3);
    assert_eq!(sched["total_eighths"], 24);
    let lead = sched["dood"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["element"] == "ANA KARAKTER")
        .unwrap();
    assert_eq!(lead["start_day"], 1);
    assert_eq!(lead["finish_day"], 3);
    assert_eq!(lead["work_days"], 2);
    assert_eq!(lead["hold_days"], 1);

    // RBAC
    let (st, _) = call(
        &app,
        "GET",
        &format!("/budgets/{a}/schedule"),
        Some(&stranger),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, _) = call(
        &app,
        "POST",
        &format!("/budgets/{a}/strips"),
        Some(&stranger),
        Some(json!({"day":1,"scene":"X"})),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn purchase_order_approve_convert_and_rbac() {
    let app = budgetcut_server::app(budgetcut_server::AppState::new());
    let owner = register(&app, "owner7@x.io").await;
    let stranger = register(&app, "stranger7@x.io").await;
    let a = create_seeded(&app, &owner, "PO").await;
    add_director(&app, &owner, &a).await;
    let acc = account_1301(&app, &owner, &a).await;

    // Create a Draft PO of 50000 against the director account.
    let (st, rec) = call(
        &app,
        "POST",
        &format!("/budgets/{a}/purchase-orders"),
        Some(&owner),
        Some(json!({"account": acc, "vendor": "Kamera Ltd", "amount": "50000"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let po = rec["id"].as_str().unwrap().to_string();

    let (_, d0) = call(
        &app,
        "GET",
        &format!("/budgets/{a}/purchase-orders"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(d0["draft_total"], "50000.00");
    assert_eq!(d0["committed_total"], "0.00");

    // Approve → committed reflects it.
    call(
        &app,
        "POST",
        &format!("/budgets/{a}/purchase-orders/{po}/approve"),
        Some(&owner),
        None,
    )
    .await;
    let (_, d1) = call(
        &app,
        "GET",
        &format!("/budgets/{a}/purchase-orders"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(d1["approved_total"], "50000.00");
    assert_eq!(d1["committed_total"], "50000.00");

    // Convert → status converted + an actual is created.
    let (st, _) = call(
        &app,
        "POST",
        &format!("/budgets/{a}/purchase-orders/{po}/convert"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let (_, d2) = call(
        &app,
        "GET",
        &format!("/budgets/{a}/purchase-orders"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(d2["orders"][0]["status"], "converted");
    assert_eq!(d2["converted_total"], "50000.00");
    let (_, actuals) = call(
        &app,
        "GET",
        &format!("/budgets/{a}/actuals"),
        Some(&owner),
        None,
    )
    .await;
    let lines = actuals["lines"].as_array().unwrap();
    assert!(
        lines.iter().any(|l| l["net"] == "50000.00"),
        "convert should create an actual: {actuals:?}"
    );
    // Deterministic id: the actual is keyed by the PO id, so a repeat convert
    // re-inserts the SAME actual (LWW) — no double-count across nodes.
    assert!(
        lines.iter().any(|l| l["id"] == po),
        "actual id should equal the PO id"
    );
    let before = lines.len();
    call(
        &app,
        "POST",
        &format!("/budgets/{a}/purchase-orders/{po}/convert"),
        Some(&owner),
        None,
    )
    .await;
    let (_, again) = call(
        &app,
        "GET",
        &format!("/budgets/{a}/actuals"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(
        again["lines"].as_array().unwrap().len(),
        before,
        "re-convert must not duplicate the actual"
    );

    // RBAC: a non-member cannot create a PO.
    let (st, _) = call(
        &app,
        "POST",
        &format!("/budgets/{a}/purchase-orders"),
        Some(&stranger),
        Some(json!({"account": acc, "amount": "10"})),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn dizi_template_reproduces_bos_butce_total() {
    let app = budgetcut_server::app(budgetcut_server::AppState::new());
    let owner = register(&app, "tpl@x.io").await;
    // Create straight from the real "BOŞ BÜTÇE" template.
    let (st, b) = call(
        &app,
        "POST",
        "/budgets",
        Some(&owner),
        Some(json!({"name":"Mayadrom Bölüm 1","template":"dizi"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let bid = b["id"].as_str().unwrap();
    let (st, top) = call(
        &app,
        "GET",
        &format!("/budgets/{bid}/topsheet"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    // DİREKT MALİYET from the source sheet, to the kuruş.
    assert_eq!(top["grand_total"], "32488843.87");
    assert_eq!(top["atl_total"], "10854170.17");
    assert_eq!(top["btl_total"], "21634673.70");
    assert_eq!(top["error_count"], 0);
    assert_eq!(top["categories"].as_array().unwrap().len(), 22);

    // The national-format sheet (Ulusal Dizi Formatı) reproduces the workbook.
    let (st, sheet) = call(
        &app,
        "GET",
        &format!("/budgets/{bid}/national-sheet"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(sheet["grand_total"], "32488843.87");
    assert_eq!(sheet["atl_total"], "10854170.17");
    assert_eq!(sheet["btl_total"], "21634673.70");
    let rows = sheet["rows"].as_array().unwrap();
    assert!(rows.iter().any(|r| r["kind"] == "grand"));
    // 22 per-category "TOPLAM /" rows + 2 section rows.
    assert_eq!(rows.iter().filter(|r| r["kind"] == "subtotal").count(), 22);
    assert_eq!(rows.iter().filter(|r| r["kind"] == "section").count(), 2);
}

#[tokio::test]
async fn netflix_reports_end_to_end() {
    let app = budgetcut_server::app(budgetcut_server::AppState::new());
    let owner = register(&app, "nflx@x.io").await;
    let a = create_seeded(&app, &owner, "Pera").await;
    add_director(&app, &owner, &a).await; // account 1301 → 660000 line + 17% stopaj

    // Locate account 1301's id.
    let (_, tree) = call(
        &app,
        "GET",
        &format!("/budgets/{a}/tree"),
        Some(&owner),
        None,
    )
    .await;
    let mut acc_id = String::new();
    for c in tree["categories"].as_array().unwrap() {
        for ac in c["accounts"].as_array().unwrap() {
            if ac["number"] == "1301" {
                acc_id = ac["id"].as_str().unwrap().to_string();
            }
        }
    }
    assert!(!acc_id.is_empty());

    // A March actual (net 100000 @ 17% stopaj → brut 120481.93) + an Approved PO.
    call(
        &app,
        "POST",
        &format!("/budgets/{a}/actuals"),
        Some(&owner),
        Some(json!({"account": acc_id, "date":"2021-03-10", "net":"100000", "stopaj_rate":"0.17", "kdv_rate":"0.2"})),
    )
    .await;
    let (_, po) = call(
        &app,
        "POST",
        &format!("/budgets/{a}/purchase-orders"),
        Some(&owner),
        Some(json!({"account": acc_id, "date":"2021-03-05", "vendor":"Acme", "amount":"50000"})),
    )
    .await;
    let po_id = po["id"].as_str().unwrap();
    call(
        &app,
        "POST",
        &format!("/budgets/{a}/purchase-orders/{po_id}/approve"),
        Some(&owner),
        None,
    )
    .await;

    // 1) Budget topsheet — ATL section present, ladder foots to grand.
    let (st, bud) = call(
        &app,
        "GET",
        &format!("/budgets/{a}/netflix/budget?episodes=8"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert!(!bud["sections"].as_array().unwrap().is_empty());
    assert_eq!(bud["ab_total"], bud["grand_total"]);

    // 2) Cost report — brut to-date + Approved-only commitment.
    let (st, cost) = call(
        &app,
        "GET",
        &format!("/budgets/{a}/netflix/cost-report?period_start=2021-03-01&period_end=2021-03-31&episodes=8"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let atl = cost["group_rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["group_key"] == "ATL")
        .expect("ATL group");
    assert_eq!(atl["actuals_to_date"], "120481.93"); // 100000 / 0.83
    assert_eq!(atl["actuals_period"], "120481.93"); // the actual is in March
    assert_eq!(atl["commitments"], "50000.00");
    assert_eq!(atl["total_costs"], "170481.93");

    // 3) Cash flow — positive YTD, week columns present.
    let (st, cash) = call(
        &app,
        "GET",
        &format!("/budgets/{a}/netflix/cash-flow?level=detail"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert!(!cash["weeks"].as_array().unwrap().is_empty());
    assert_ne!(cash["ytd_total"], "0.00");

    // 4) Trial balance — bank param + open Approved PO by vendor.
    let (st, tb) = call(
        &app,
        "GET",
        &format!("/budgets/{a}/netflix/trial-balance?bank_balance=100000"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(tb["total"], "150000.00"); // 100000 bank + 50000 open PO
    assert!(tb["rows"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["kind"] == "Deposit" && r["amount"] == "50000.00"));
}
