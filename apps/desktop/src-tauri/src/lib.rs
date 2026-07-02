//! Tauri command layer for the BudgetCut desktop app.
//!
//! Thin glue only: every number comes from `budgetcut-core` via
//! `budgetcut-store::Session`, persisted to a local SQLite file. The frontend
//! invokes these commands; there is no business math here (§4/§11).

use std::str::FromStr;
use std::sync::Mutex;

use budgetcut_core::actuals::{tevkifat_rate, Actual};
use budgetcut_core::ids::*;
use budgetcut_core::po::{POStatus, PurchaseOrder};
use budgetcut_core::scheduling::Strip;
use budgetcut_core::settlement::Receipt;
use budgetcut_core::view::{
    series_summary_for, ActualsReportDto, AmortInput, IncentiveReportDto, PurchaseOrdersDto,
    ScheduleDto, SeriesSummaryDto, SettlementReportDto,
};
use budgetcut_core::*;
use budgetcut_store::dto::{ToolsDto, TopsheetDto, TreeDto};
use budgetcut_store::{Session, Store};
use rust_decimal::Decimal;
use tauri::Manager;

struct AppState(Mutex<Session>);

#[tauri::command]
fn get_topsheet(state: tauri::State<AppState>) -> TopsheetDto {
    state.0.lock().unwrap().topsheet()
}

#[tauri::command]
fn get_tree(state: tauri::State<AppState>) -> TreeDto {
    state.0.lock().unwrap().tree()
}

#[tauri::command]
fn get_tools(state: tauri::State<AppState>) -> ToolsDto {
    state.0.lock().unwrap().tools()
}

/// Set a named global's constant value and recompute (live recalc demo).
#[tauri::command]
fn set_global_by_name(
    state: tauri::State<AppState>,
    name: String,
    value: String,
) -> Result<(), String> {
    let v = Decimal::from_str(&value).map_err(|e| format!("geçersiz sayı: {e}"))?;
    let mut s = state.0.lock().unwrap();
    let gid = s
        .budget()
        .globals
        .values()
        .find(|g| g.name == name)
        .map(|g| g.id)
        .ok_or_else(|| format!("global bulunamadı: {name}"))?;
    s.set_global(gid, Formula::Const(v)).map_err(|e| e.to_string())?;
    Ok(())
}

/// Parse an editable cell value: a leading `=` means a global expression,
/// otherwise a literal number.
fn parse_formula(v: &str) -> Formula {
    let t = v.trim();
    if let Some(expr) = t.strip_prefix('=') {
        Formula::Expr(expr.trim().to_string())
    } else {
        match Decimal::from_str(t) {
            Ok(d) => Formula::Const(d),
            Err(_) => Formula::Const(Decimal::ZERO),
        }
    }
}

/// Edit a detail field from the grid (description / name / amount / rate).
#[tauri::command]
fn set_detail_field(
    state: tauri::State<AppState>,
    detail: String,
    field: String,
    value: String,
) -> Result<(), String> {
    let did = DetailId::from_uuid(uuid_parse(&detail)?);
    let f = match field.as_str() {
        "description" => DetailField::Description(value),
        "name" => DetailField::Name(if value.is_empty() { None } else { Some(value) }),
        "amount" => DetailField::Amount(parse_formula(&value)),
        "multiplier" => DetailField::Multiplier(parse_formula(&value)),
        "rate" => DetailField::Rate(parse_formula(&value)),
        other => return Err(format!("bilinmeyen alan: {other}")),
    };
    state.0.lock().unwrap().set_detail_field(did, f).map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(serde::Deserialize)]
struct FringeArg {
    code: String,
    rate: Option<String>,
}

/// Apply-Tools (offline): replace a line's fringes by code (+ optional rate).
#[tauri::command]
fn set_detail_fringes(
    state: tauri::State<AppState>,
    detail: String,
    fringes: Vec<FringeArg>,
) -> Result<(), String> {
    let did = DetailId::from_uuid(uuid_parse(&detail)?);
    let mut s = state.0.lock().unwrap();
    let applied = {
        let b = s.budget();
        let mut v = Vec::new();
        for fr in &fringes {
            let fid = b
                .fringes
                .values()
                .find(|f| f.code == fr.code)
                .map(|f| f.id)
                .ok_or(format!("fringe yok: {}", fr.code))?;
            v.push(match &fr.rate {
                Some(r) => AppliedFringe::with_rate(fid, Decimal::from_str(r).unwrap_or(Decimal::ZERO)),
                None => AppliedFringe::new(fid),
            });
        }
        v
    };
    s.set_detail_field(did, DetailField::Fringes(applied)).map_err(|e| e.to_string())?;
    Ok(())
}

/// Append a blank line to an account; returns the new detail id.
#[tauri::command]
fn add_line(state: tauri::State<AppState>, account: String) -> Result<String, String> {
    let aid = AccountId::from_uuid(uuid_parse(&account)?);
    let mut s = state.0.lock().unwrap();
    let b = s.budget();
    let unit = b
        .units
        .values()
        .find(|u| u.code == "ADET")
        .or_else(|| b.units.values().next())
        .map(|u| u.id)
        .ok_or("bütçede birim yok")?;
    let cur = b.base_currency;
    let pos = b
        .details_of(aid)
        .last()
        .map(|d| d.position + Decimal::ONE)
        .unwrap_or(Decimal::ONE);
    let d = Detail {
        id: DetailId::new(),
        account: aid,
        position: pos,
        description: "Yeni satır".into(),
        name: None,
        amount: Formula::Const(Decimal::ONE),
        multiplier: Formula::Const(Decimal::ONE),
        rate: Formula::Const(Decimal::ZERO),
        unit,
        currency: cur,
        applied_fringes: vec![],
        groups: vec![],
        location: None,
        set: None,
        gl_code: None,
        notes: None,
    };
    let id = d.id;
    s.insert_detail(d).map_err(|e| e.to_string())?;
    Ok(id.to_string())
}

/// Delete a line.
#[tauri::command]
fn remove_line(state: tauri::State<AppState>, detail: String) -> Result<(), String> {
    let did = DetailId::from_uuid(uuid_parse(&detail)?);
    state.0.lock().unwrap().remove_detail(did).map_err(|e| e.to_string())?;
    Ok(())
}

fn uuid_parse(s: &str) -> Result<uuid::Uuid, String> {
    uuid::Uuid::parse_str(s).map_err(|_| format!("geçersiz id: {s}"))
}

// --- MMB-parity analytics (offline): same core math as the server ---

/// Amort & pattern series: this local budget's net is one pattern episode.
#[tauri::command]
fn series_summary(
    state: tauri::State<AppState>,
    episodes: u32,
    amortized: Vec<AmortInput>,
) -> SeriesSummaryDto {
    let s = state.0.lock().unwrap();
    let r = evaluate(s.budget());
    series_summary_for(&r, episodes, &amortized)
}

/// Incentive estimates (Turkish presets). Qualifying defaults to net total.
#[tauri::command]
fn incentive_report(state: tauri::State<AppState>, qualifying: Option<String>) -> IncentiveReportDto {
    let s = state.0.lock().unwrap();
    let q = match qualifying.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(v) => Decimal::from_str(v).unwrap_or_else(|_| evaluate(s.budget()).net_total),
        None => evaluate(s.budget()).net_total,
    };
    IncentiveReportDto::turkish_for(q)
}

/// Accounting GL export as CSV text (the SmartAccounting hand-off analog).
#[tauri::command]
fn accounting_csv(state: tauri::State<AppState>) -> String {
    let s = state.0.lock().unwrap();
    budgetcut_export::accounting_csv(s.budget())
}

/// Estimate-vs-actual / EFC report + invoice lines (§16 Phase 3).
#[tauri::command]
fn get_actuals(state: tauri::State<AppState>) -> ActualsReportDto {
    state.0.lock().unwrap().actuals()
}

#[derive(serde::Deserialize)]
struct AddActualArg {
    account: String,
    #[serde(default)]
    date: String,
    #[serde(default)]
    vendor: String,
    #[serde(default)]
    description: String,
    net: String,
    #[serde(default)]
    stopaj_rate: String,
    #[serde(default)]
    kdv_rate: String,
    #[serde(default)]
    tevkifat_kind: Option<String>,
}

/// Record an actual against an account (offline).
#[tauri::command]
fn add_actual(state: tauri::State<AppState>, arg: AddActualArg) -> Result<String, String> {
    let account = AccountId::from_uuid(uuid_parse(&arg.account)?);
    let net = Decimal::from_str(arg.net.trim()).map_err(|e| format!("geçersiz net: {e}"))?;
    let rate = |s: &str| Decimal::from_str(s.trim()).unwrap_or(Decimal::ZERO);
    let a = Actual {
        id: uuid::Uuid::now_v7(),
        account,
        date: arg.date,
        vendor: arg.vendor,
        description: arg.description,
        net,
        stopaj_rate: rate(&arg.stopaj_rate),
        kdv_rate: rate(&arg.kdv_rate),
        tevkifat_rate: arg.tevkifat_kind.as_deref().map(tevkifat_rate).unwrap_or(Decimal::ZERO),
    };
    let id = a.id.to_string();
    state.0.lock().unwrap().add_actual(a).map_err(|e| e.to_string())?;
    Ok(id)
}

/// Delete a recorded actual (offline).
#[tauri::command]
fn remove_actual(state: tauri::State<AppState>, actual: String) -> Result<(), String> {
    let aid = uuid_parse(&actual)?;
    state.0.lock().unwrap().remove_actual(aid).map_err(|e| e.to_string())?;
    Ok(())
}

/// Expense settlement ("Hesap Kapama") reconciled against an advance (offline).
#[tauri::command]
fn get_settlement(state: tauri::State<AppState>, advance: Option<String>) -> SettlementReportDto {
    let adv = advance
        .as_deref()
        .map(|s| Decimal::from_str(s.trim()).unwrap_or(Decimal::ZERO))
        .unwrap_or(Decimal::ZERO);
    state.0.lock().unwrap().settlement(adv)
}

#[derive(serde::Deserialize)]
struct AddReceiptArg {
    #[serde(default)]
    date: String,
    #[serde(default)]
    vendor: String,
    #[serde(default)]
    receipt_no: String,
    category: String,
    #[serde(default)]
    description: String,
    gross: String,
    #[serde(default)]
    kdv_rate: String,
}

/// Record a settlement receipt (offline).
#[tauri::command]
fn add_receipt(state: tauri::State<AppState>, arg: AddReceiptArg) -> Result<String, String> {
    let gross = Decimal::from_str(arg.gross.trim()).map_err(|e| format!("geçersiz tutar: {e}"))?;
    let r = Receipt {
        id: uuid::Uuid::now_v7(),
        date: arg.date,
        vendor: arg.vendor,
        receipt_no: arg.receipt_no,
        category: arg.category,
        description: arg.description,
        gross,
        kdv_rate: Decimal::from_str(arg.kdv_rate.trim()).unwrap_or(Decimal::ZERO),
    };
    let id = r.id.to_string();
    state.0.lock().unwrap().add_receipt(r).map_err(|e| e.to_string())?;
    Ok(id)
}

/// Delete a settlement receipt (offline).
#[tauri::command]
fn remove_receipt(state: tauri::State<AppState>, receipt: String) -> Result<(), String> {
    let rid = uuid_parse(&receipt)?;
    state.0.lock().unwrap().remove_receipt(rid).map_err(|e| e.to_string())?;
    Ok(())
}

/// Stripboard + Day-Out-of-Days (offline).
#[tauri::command]
fn get_schedule(state: tauri::State<AppState>) -> ScheduleDto {
    state.0.lock().unwrap().schedule()
}

#[derive(serde::Deserialize)]
struct AddStripArg {
    day: u32,
    scene: String,
    #[serde(default)]
    set: String,
    #[serde(default)]
    eighths: u32,
    #[serde(default)]
    elements: Vec<String>,
}

/// Add a stripboard strip (offline).
#[tauri::command]
fn add_strip(state: tauri::State<AppState>, arg: AddStripArg) -> Result<String, String> {
    let s = Strip {
        id: uuid::Uuid::now_v7(),
        day: arg.day.max(1), // 1-based; reject the phantom day 0
        scene: arg.scene,
        set: arg.set,
        eighths: arg.eighths,
        elements: arg.elements,
    };
    let id = s.id.to_string();
    state.0.lock().unwrap().add_strip(s).map_err(|e| e.to_string())?;
    Ok(id)
}

/// Delete a stripboard strip (offline).
#[tauri::command]
fn remove_strip(state: tauri::State<AppState>, strip: String) -> Result<(), String> {
    let sid = uuid_parse(&strip)?;
    state.0.lock().unwrap().remove_strip(sid).map_err(|e| e.to_string())?;
    Ok(())
}

/// Purchase orders + committed totals (offline).
#[tauri::command]
fn get_purchase_orders(state: tauri::State<AppState>) -> PurchaseOrdersDto {
    state.0.lock().unwrap().purchase_orders()
}

#[derive(serde::Deserialize)]
struct AddPoArg {
    account: String,
    #[serde(default)]
    date: String,
    #[serde(default)]
    vendor: String,
    #[serde(default)]
    description: String,
    amount: String,
}

/// Create a Draft PO (offline).
#[tauri::command]
fn add_po(state: tauri::State<AppState>, arg: AddPoArg) -> Result<String, String> {
    let account = AccountId::from_uuid(uuid_parse(&arg.account)?);
    let amount = Decimal::from_str(arg.amount.trim()).map_err(|e| format!("geçersiz tutar: {e}"))?;
    let po = PurchaseOrder {
        id: uuid::Uuid::now_v7(),
        account,
        date: arg.date,
        vendor: arg.vendor,
        description: arg.description,
        amount,
        status: POStatus::Draft,
    };
    let id = po.id.to_string();
    state.0.lock().unwrap().put_purchase_order(po).map_err(|e| e.to_string())?;
    Ok(id)
}

/// Approve a PO (offline).
#[tauri::command]
fn approve_po(state: tauri::State<AppState>, po: String) -> Result<(), String> {
    let pid = uuid_parse(&po)?;
    let mut s = state.0.lock().unwrap();
    let mut p = s.purchase_order(pid).ok_or("PO yok")?;
    p.status = POStatus::Approved;
    s.put_purchase_order(p).map_err(|e| e.to_string())?;
    Ok(())
}

/// Convert a PO to an actual (offline).
#[tauri::command]
fn convert_po(state: tauri::State<AppState>, po: String) -> Result<(), String> {
    let pid = uuid_parse(&po)?;
    let mut s = state.0.lock().unwrap();
    let mut p = s.purchase_order(pid).ok_or("PO yok")?;
    if p.status == POStatus::Converted {
        return Ok(());
    }
    let actual = Actual {
        // Deterministic id from the PO → concurrent convert re-inserts the same
        // actual (LWW) rather than double-counting it.
        id: p.id,
        account: p.account,
        date: p.date.clone(),
        vendor: p.vendor.clone(),
        description: p.description.clone(),
        net: p.amount,
        stopaj_rate: Decimal::ZERO,
        kdv_rate: Decimal::ZERO,
        tevkifat_rate: Decimal::ZERO,
    };
    s.add_actual(actual).map_err(|e| e.to_string())?;
    p.status = POStatus::Converted;
    s.put_purchase_order(p).map_err(|e| e.to_string())?;
    Ok(())
}

/// Delete a PO (offline).
#[tauri::command]
fn remove_po(state: tauri::State<AppState>, po: String) -> Result<(), String> {
    let pid = uuid_parse(&po)?;
    state.0.lock().unwrap().remove_purchase_order(pid).map_err(|e| e.to_string())?;
    Ok(())
}

/// Replace the local budget with the real "BOŞ BÜTÇE" sample (offline reset).
#[tauri::command]
fn load_sample(state: tauri::State<AppState>) -> Result<(), String> {
    state
        .0
        .lock()
        .unwrap()
        .reseed(templates::dizi_full_template("BOŞ BÜTÇE — Bölüm 1"))
        .map_err(|e| e.to_string())
}

/// Fetch TCMB FX + İstanbul fuel prices (blocking helper for the command).
fn fetch_live_rates_blocking() -> budgetcut_importers::rates::LiveRates {
    use budgetcut_importers::rates::*;
    let get = |url: &str| -> Option<String> {
        ureq::get(url)
            .timeout(std::time::Duration::from_secs(8))
            .set("User-Agent", "BudgetCut/0.1")
            .call()
            .ok()?
            .into_string()
            .ok()
    };
    let mut out = LiveRates::default();
    if let Some(xml) = get("https://www.tcmb.gov.tr/kurlar/today.xml") {
        let (date, usd, eur) = parse_tcmb_xml(&xml);
        out.date = date;
        out.usd = usd;
        out.eur = eur;
    }
    if let Some(json) =
        get("https://api.opet.com.tr/api/fuelprices/prices?ProvinceCode=034&IncludeAllProducts=true")
    {
        let (b, m) = parse_opet_json(&json);
        out.benzin = b;
        out.motorin = m;
    }
    if out.benzin.is_none() {
        if let Some(json) = get("https://hasanadiguzel.com.tr/api/akaryakit/sehir=ISTANBUL") {
            let (b, m) = parse_ha_json(&json);
            out.benzin = b;
            out.motorin = out.motorin.or(m);
        }
    }
    out
}

/// Today's TCMB USD/EUR + İstanbul pump prices for the top-right panel.
#[tauri::command]
async fn live_rates() -> budgetcut_importers::rates::LiveRates {
    tauri::async_runtime::spawn_blocking(fetch_live_rates_blocking)
        .await
        .unwrap_or_default()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir).ok();
            let db = dir.join("budget.db");
            let store = Store::open(&db)?;
            let author = UserId::new();
            let session = if store.has_budget()? {
                Session::open(store, author)?
            } else {
                // First run: seed the real "BOŞ BÜTÇE" dizi budget (≈₺32,5M,
                // 258 lines) so the app opens on a genuine production budget.
                Session::create(
                    store,
                    templates::dizi_full_template("BOŞ BÜTÇE — Bölüm 1"),
                    author,
                )?
            };
            app.manage(AppState(Mutex::new(session)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_topsheet,
            get_tree,
            get_tools,
            set_global_by_name,
            set_detail_field,
            set_detail_fringes,
            add_line,
            remove_line,
            series_summary,
            incentive_report,
            accounting_csv,
            get_actuals,
            add_actual,
            remove_actual,
            get_settlement,
            add_receipt,
            remove_receipt,
            get_schedule,
            add_strip,
            remove_strip,
            get_purchase_orders,
            add_po,
            approve_po,
            convert_po,
            remove_po,
            load_sample,
            live_rates
        ])
        .run(tauri::generate_context!())
        .expect("error while running BudgetCut");
}
