//! BudgetCut sync server (§9/§10). Server-authoritative: every op is validated
//! against the caller's RBAC and applied through the **same** `budgetcut-core`
//! reducer the client uses (§4), then broadcast to permitted subscribers.
//!
//! RBAC is a tree (§9): membership at **Organization → Project → Budget**, most
//! specific wins. Auth issues short-lived access tokens plus rotating refresh
//! tokens with reuse detection. The WebSocket channel carries both authoritative
//! ops (scope-filtered) and ephemeral presence. State is in-memory so the server
//! runs with no external dependencies; Postgres is the production backend.

#![forbid(unsafe_code)]

pub mod auth;
pub mod rbac;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{FromRequestParts, Path, Query, State};
use axum::http::{header::AUTHORIZATION, request::Parts, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use budgetcut_core::actuals::{tevkifat_rate, Actual};
use budgetcut_core::ids::{
    AccountId, BudgetId, CategoryId, DetailId, FringeId, GlobalId, OrgId, ProjectId, UserId,
};
use budgetcut_core::library::SetupLibrary;
use budgetcut_core::po::{POStatus, PurchaseOrder};
use budgetcut_core::scheduling::Strip;
use budgetcut_core::settlement::Receipt;
use budgetcut_core::view::{
    series_summary_for, ActualsReportDto, AmortInput, ComparisonDto, IncentiveReportDto,
    NationalSheetDto, PurchaseOrdersDto, ScheduleDto, SeriesSummaryDto, SettlementReportDto,
    ToolsDto, TopsheetDto, TreeDto,
};
use budgetcut_core::{
    evaluate, AppliedFringe, ApplyResult, Budget, Detail, DetailField, Document, Formula, Fringe,
    Global, HlcClock, Op, OpKind,
};
use futures_util::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::str::FromStr;
use tokio::sync::broadcast;
use uuid::Uuid;

use rbac::{can_read, can_write, Membership, Role};

const DEFAULT_DEV_SECRET: &[u8] = b"budgetcut-dev-secret-change-me";
const ACCESS_TTL_SECS: usize = 60 * 15;
const REFRESH_TTL_SECS: usize = 60 * 60 * 24 * 30;

struct User {
    id: UserId,
    pw_hash: String,
}

struct Org {
    // Stored for the (future) org/project listing endpoints.
    #[allow(dead_code)]
    name: String,
    members: HashMap<UserId, Role>,
}

struct Project {
    org: OrgId,
    #[allow(dead_code)]
    name: String,
    members: HashMap<UserId, Role>,
}

struct Room {
    project: Option<ProjectId>,
    base: Budget,
    doc: Document,
    ops: Vec<Op>,
    members: HashMap<UserId, Membership>,
    tx: broadcast::Sender<ServerMsg>,
}

/// A rotating refresh-token record; `family` ties a rotation chain together so
/// reuse can revoke the whole chain.
struct Refresh {
    user_id: UserId,
    family: String,
    used: bool,
    revoked: bool,
}

/// A reusable setup library (fringes + globals) owned by a user (§ MMB
/// cloud-synched libraries). Keyed by a generated id.
struct StoredLibrary {
    name: String,
    owner: UserId,
    lib: SetupLibrary,
}

struct Inner {
    users: HashMap<String, User>,
    orgs: HashMap<OrgId, Org>,
    projects: HashMap<ProjectId, Project>,
    budgets: HashMap<BudgetId, Room>,
    libraries: HashMap<Uuid, StoredLibrary>,
    refresh: HashMap<String, Refresh>,
    /// Authoritative HLC clock for server-minted ops (the convenience mutation
    /// endpoints). Keeps server ops strictly increasing.
    clock: HlcClock,
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Messages broadcast on a budget's channel: authoritative ops and ephemeral
/// presence.
#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ServerMsg {
    // Boxed: an `Op` is much larger than a presence ping, so boxing keeps the
    // broadcast message small (clippy::large_enum_variant).
    Op { op: Box<Op> },
    Presence { user: String, payload: Value },
}

pub struct AppState {
    jwt_secret: Vec<u8>,
    inner: Mutex<Inner>,
}

impl AppState {
    pub fn new() -> Arc<Self> {
        let secret = std::env::var("BUDGETCUT_JWT_SECRET")
            .map(|s| s.into_bytes())
            .unwrap_or_else(|_| DEFAULT_DEV_SECRET.to_vec());
        Arc::new(Self {
            jwt_secret: secret,
            inner: Mutex::new(Inner {
                users: HashMap::new(),
                orgs: HashMap::new(),
                projects: HashMap::new(),
                budgets: HashMap::new(),
                libraries: HashMap::new(),
                refresh: HashMap::new(),
                clock: HlcClock::new(UserId::new()),
            }),
        })
    }

    /// Lock the shared state, tolerating a poisoned mutex. Handlers must not
    /// panic while holding the lock, but if one ever does, the next caller
    /// recovers the guard instead of cascading the panic across every request
    /// (defense-in-depth against a single bad request bricking the server).
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

pub fn app(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .route("/auth/refresh", post(refresh_token))
        .route("/orgs", post(create_org))
        .route("/orgs/:id/members", post(add_org_member))
        .route("/orgs/:id/projects", post(create_project))
        .route("/projects/:id/members", post(add_project_member))
        .route("/projects/:id/budgets", post(create_project_budget))
        .route("/budgets", post(create_budget).get(list_budgets))
        .route("/budgets/:id/topsheet", get(get_topsheet))
        .route("/budgets/:id/tree", get(get_tree))
        .route("/budgets/:id/national-sheet", get(get_national_sheet))
        .route("/budgets/:id/tools", get(get_tools))
        .route("/budgets/:id/members", post(add_member))
        .route("/budgets/:id/duplicate", post(duplicate_budget))
        .route("/budgets/:id/snapshot", get(snapshot))
        .route("/budgets/:id/ops", post(submit_op))
        .route("/budgets/:id/global", post(set_global))
        .route("/budgets/:id/lines", post(add_line))
        .route("/budgets/:id/details/:detail/field", post(set_detail_field))
        .route(
            "/budgets/:id/details/:detail/fringes",
            post(set_detail_fringes),
        )
        .route("/budgets/:id/details/:detail/delete", post(remove_line))
        // MMB-parity analytics (§ feature-parity pass)
        .route("/budgets/:id/series", post(post_series))
        .route("/budgets/:id/incentives", get(get_incentives))
        .route("/budgets/:id/accounting.csv", get(get_accounting_csv))
        .route("/compare", get(get_compare))
        .route("/libraries", get(list_libraries).post(save_library))
        .route("/budgets/:id/libraries/:lib/apply", post(apply_library))
        .route("/budgets/:id/actuals", get(get_actuals).post(post_actual))
        .route("/budgets/:id/actuals/:actual/delete", post(delete_actual))
        .route("/budgets/:id/settlement", get(get_settlement))
        .route("/budgets/:id/receipts", post(post_receipt))
        .route(
            "/budgets/:id/receipts/:receipt/delete",
            post(delete_receipt),
        )
        .route("/budgets/:id/schedule", get(get_schedule))
        .route("/budgets/:id/strips", post(post_strip))
        .route("/budgets/:id/strips/:strip/delete", post(delete_strip))
        .route(
            "/budgets/:id/purchase-orders",
            get(get_purchase_orders).post(post_po),
        )
        .route("/budgets/:id/purchase-orders/:po/approve", post(approve_po))
        .route("/budgets/:id/purchase-orders/:po/convert", post(convert_po))
        .route("/budgets/:id/purchase-orders/:po/delete", post(delete_po))
        .route("/rates", get(get_live_rates))
        .route("/budgets/:id/ws", get(ws_handler))
        .layer(tower_http::cors::CorsLayer::permissive())
        .with_state(state)
}

/// Fetch TCMB FX + İstanbul fuel prices (blocking; call via spawn_blocking).
/// Public data proxied so the browser client dodges CORS; every field optional.
fn fetch_live_rates() -> budgetcut_importers::rates::LiveRates {
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
    if let Some(json) = get(
        "https://api.opet.com.tr/api/fuelprices/prices?ProvinceCode=034&IncludeAllProducts=true",
    ) {
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
async fn get_live_rates() -> Json<budgetcut_importers::rates::LiveRates> {
    let rates = tokio::task::spawn_blocking(fetch_live_rates)
        .await
        .unwrap_or_default();
    Json(rates)
}

// ---------------------------------------------------------------------------
// Auth extractor + helpers
// ---------------------------------------------------------------------------

pub struct AuthUser(pub UserId);

#[axum::async_trait]
impl FromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = (StatusCode, String);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .ok_or((StatusCode::UNAUTHORIZED, "missing Authorization".into()))?;
        let token = header.strip_prefix("Bearer ").unwrap_or(header);
        let claims = auth::verify_token(&state.jwt_secret, token)
            .map_err(|e| (StatusCode::UNAUTHORIZED, e))?;
        let uid = Uuid::parse_str(&claims.sub)
            .map(UserId::from_uuid)
            .map_err(|_| (StatusCode::UNAUTHORIZED, "bad subject".into()))?;
        Ok(AuthUser(uid))
    }
}

type ApiResult<T> = Result<Json<T>, (StatusCode, String)>;

fn err(code: StatusCode, msg: impl Into<String>) -> (StatusCode, String) {
    (code, msg.into())
}

/// Effective membership on a budget, cascading Budget → Project → Org with most
/// specific winning (§9 RBAC tree).
fn effective_membership(inner: &Inner, uid: UserId, bid: BudgetId) -> Option<Membership> {
    let room = inner.budgets.get(&bid)?;
    if let Some(m) = room.members.get(&uid) {
        return Some(m.clone());
    }
    let pid = room.project?;
    let project = inner.projects.get(&pid)?;
    if let Some(r) = project.members.get(&uid) {
        return Some(Membership::full(*r));
    }
    let org = inner.orgs.get(&project.org)?;
    org.members.get(&uid).map(|r| Membership::full(*r))
}

#[derive(Serialize)]
struct AuthResp {
    token: String,
    refresh_token: String,
    user_id: String,
}

/// Mint an access + refresh token pair (starts a new refresh family).
fn issue_tokens(
    state: &AppState,
    inner: &mut Inner,
    uid: UserId,
) -> Result<AuthResp, (StatusCode, String)> {
    let access = auth::make_token(&state.jwt_secret, &uid.to_string(), ACCESS_TTL_SECS)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let jti = Uuid::now_v7().to_string();
    let family = Uuid::now_v7().to_string();
    let refresh = auth::make_refresh_token(
        &state.jwt_secret,
        &uid.to_string(),
        &jti,
        &family,
        REFRESH_TTL_SECS,
    )
    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    inner.refresh.insert(
        jti,
        Refresh {
            user_id: uid,
            family,
            used: false,
            revoked: false,
        },
    );
    Ok(AuthResp {
        token: access,
        refresh_token: refresh,
        user_id: uid.to_string(),
    })
}

// ---------------------------------------------------------------------------
// Auth handlers
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct Credentials {
    email: String,
    password: String,
}

async fn register(
    State(state): State<Arc<AppState>>,
    Json(body): Json<Credentials>,
) -> ApiResult<AuthResp> {
    if body.password.len() < 6 {
        return Err(err(StatusCode::BAD_REQUEST, "password too short"));
    }
    let hash = auth::hash_password(&body.password)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let mut inner = state.lock();
    if inner.users.contains_key(&body.email) {
        return Err(err(StatusCode::CONFLICT, "email already registered"));
    }
    let id = UserId::new();
    inner
        .users
        .insert(body.email.clone(), User { id, pw_hash: hash });
    let resp = issue_tokens(&state, &mut inner, id)?;
    Ok(Json(resp))
}

async fn login(
    State(state): State<Arc<AppState>>,
    Json(body): Json<Credentials>,
) -> ApiResult<AuthResp> {
    let mut inner = state.lock();
    let user = inner
        .users
        .get(&body.email)
        .ok_or(err(StatusCode::UNAUTHORIZED, "invalid credentials"))?;
    if !auth::verify_password(&body.password, &user.pw_hash) {
        return Err(err(StatusCode::UNAUTHORIZED, "invalid credentials"));
    }
    let id = user.id;
    let resp = issue_tokens(&state, &mut inner, id)?;
    Ok(Json(resp))
}

#[derive(Deserialize)]
struct RefreshReq {
    refresh_token: String,
}

/// Rotate a refresh token. Detects reuse: presenting an already-used token
/// revokes the entire family (§9).
async fn refresh_token(
    State(state): State<Arc<AppState>>,
    Json(body): Json<RefreshReq>,
) -> ApiResult<AuthResp> {
    let claims = auth::verify_refresh_token(&state.jwt_secret, &body.refresh_token)
        .map_err(|e| err(StatusCode::UNAUTHORIZED, e))?;
    let mut inner = state.lock();

    let rec = inner
        .refresh
        .get(&claims.jti)
        .ok_or(err(StatusCode::UNAUTHORIZED, "unknown refresh token"))?;
    if rec.revoked {
        return Err(err(StatusCode::UNAUTHORIZED, "refresh token revoked"));
    }
    if rec.used {
        // Reuse detected — revoke the whole family.
        let family = rec.family.clone();
        for r in inner.refresh.values_mut() {
            if r.family == family {
                r.revoked = true;
            }
        }
        return Err(err(
            StatusCode::UNAUTHORIZED,
            "refresh token reuse detected; family revoked",
        ));
    }
    let uid = rec.user_id;
    let family = rec.family.clone();
    // Mark the presented token used, then rotate within the same family.
    inner.refresh.get_mut(&claims.jti).unwrap().used = true;

    let access = auth::make_token(&state.jwt_secret, &uid.to_string(), ACCESS_TTL_SECS)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let new_jti = Uuid::now_v7().to_string();
    let refresh = auth::make_refresh_token(
        &state.jwt_secret,
        &uid.to_string(),
        &new_jti,
        &family,
        REFRESH_TTL_SECS,
    )
    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    inner.refresh.insert(
        new_jti,
        Refresh {
            user_id: uid,
            family,
            used: false,
            revoked: false,
        },
    );
    Ok(Json(AuthResp {
        token: access,
        refresh_token: refresh,
        user_id: uid.to_string(),
    }))
}

// ---------------------------------------------------------------------------
// Org / Project handlers
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct NameReq {
    name: String,
}

#[derive(Serialize)]
struct IdResp {
    id: String,
}

async fn create_org(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Json(body): Json<NameReq>,
) -> ApiResult<IdResp> {
    let id = OrgId::new();
    let mut members = HashMap::new();
    members.insert(uid, Role::Owner);
    state.lock().orgs.insert(
        id,
        Org {
            name: body.name,
            members,
        },
    );
    Ok(Json(IdResp { id: id.to_string() }))
}

#[derive(Deserialize)]
struct MemberReq {
    user_id: String,
    role: Role,
    #[serde(default)]
    scope: Vec<CategoryId>,
}

fn parse_user(s: &str) -> Result<UserId, (StatusCode, String)> {
    Uuid::parse_str(s)
        .map(UserId::from_uuid)
        .map_err(|_| err(StatusCode::BAD_REQUEST, "bad user_id"))
}

async fn add_org_member(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path(id): Path<String>,
    Json(body): Json<MemberReq>,
) -> ApiResult<Value> {
    let oid = OrgId::from_uuid(
        Uuid::parse_str(&id).map_err(|_| err(StatusCode::BAD_REQUEST, "bad org id"))?,
    );
    let target = parse_user(&body.user_id)?;
    let mut inner = state.lock();
    let org = inner
        .orgs
        .get_mut(&oid)
        .ok_or(err(StatusCode::NOT_FOUND, "no such org"))?;
    match org.members.get(&uid) {
        Some(Role::Owner | Role::Admin) => {}
        _ => {
            return Err(err(
                StatusCode::FORBIDDEN,
                "only org owner/admin can add members",
            ))
        }
    }
    org.members.insert(target, body.role);
    Ok(Json(serde_json::json!({"ok": true})))
}

async fn create_project(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path(id): Path<String>,
    Json(body): Json<NameReq>,
) -> ApiResult<IdResp> {
    let oid = OrgId::from_uuid(
        Uuid::parse_str(&id).map_err(|_| err(StatusCode::BAD_REQUEST, "bad org id"))?,
    );
    let mut inner = state.lock();
    let org = inner
        .orgs
        .get(&oid)
        .ok_or(err(StatusCode::NOT_FOUND, "no such org"))?;
    match org.members.get(&uid) {
        Some(Role::Owner | Role::Admin) => {}
        _ => {
            return Err(err(
                StatusCode::FORBIDDEN,
                "only org owner/admin can create projects",
            ))
        }
    }
    let pid = ProjectId::new();
    inner.projects.insert(
        pid,
        Project {
            org: oid,
            name: body.name,
            members: HashMap::new(),
        },
    );
    Ok(Json(IdResp {
        id: pid.to_string(),
    }))
}

async fn add_project_member(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path(id): Path<String>,
    Json(body): Json<MemberReq>,
) -> ApiResult<Value> {
    let pid = ProjectId::from_uuid(
        Uuid::parse_str(&id).map_err(|_| err(StatusCode::BAD_REQUEST, "bad project id"))?,
    );
    let target = parse_user(&body.user_id)?;
    let mut inner = state.lock();
    let project = inner
        .projects
        .get(&pid)
        .ok_or(err(StatusCode::NOT_FOUND, "no such project"))?;
    let org_id = project.org;
    let allowed = matches!(project.members.get(&uid), Some(Role::Owner | Role::Admin))
        || matches!(
            inner.orgs.get(&org_id).and_then(|o| o.members.get(&uid)),
            Some(Role::Owner | Role::Admin)
        );
    if !allowed {
        return Err(err(
            StatusCode::FORBIDDEN,
            "only project/org owner/admin can add members",
        ));
    }
    inner
        .projects
        .get_mut(&pid)
        .unwrap()
        .members
        .insert(target, body.role);
    Ok(Json(serde_json::json!({"ok": true})))
}

#[derive(Deserialize)]
struct CreateBudgetReq {
    name: String,
    #[serde(default)]
    seed_template: bool,
    /// Which seed template: "dizi" (the real BOŞ BÜTÇE), "netflix" (Netflix
    /// CoA), or empty. Falls back to seed_template=true ⇒ "netflix".
    #[serde(default)]
    template: String,
}

impl CreateBudgetReq {
    fn template_key(&self) -> &str {
        if !self.template.is_empty() {
            &self.template
        } else if self.seed_template {
            "netflix"
        } else {
            ""
        }
    }
}

fn make_room(
    name: &str,
    template: &str,
    project: Option<ProjectId>,
    owner: Option<UserId>,
) -> (BudgetId, Room) {
    let base = match template {
        "dizi" => budgetcut_core::templates::dizi_full_template(name),
        "netflix" => budgetcut_core::templates::turkish_dizi_template(name),
        _ => Budget::new(name, budgetcut_core::templates::try_currency()),
    };
    let id = base.id;
    let (tx, _) = broadcast::channel(1024);
    let mut members = HashMap::new();
    if let Some(o) = owner {
        members.insert(o, Membership::full(Role::Owner));
    }
    (
        id,
        Room {
            project,
            base: base.clone(),
            doc: Document::new(base),
            ops: vec![],
            members,
            tx,
        },
    )
}

async fn create_budget(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Json(body): Json<CreateBudgetReq>,
) -> ApiResult<IdResp> {
    let (id, room) = make_room(&body.name, body.template_key(), None, Some(uid));
    state.lock().budgets.insert(id, room);
    Ok(Json(IdResp { id: id.to_string() }))
}

async fn create_project_budget(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path(id): Path<String>,
    Json(body): Json<CreateBudgetReq>,
) -> ApiResult<IdResp> {
    let pid = ProjectId::from_uuid(
        Uuid::parse_str(&id).map_err(|_| err(StatusCode::BAD_REQUEST, "bad project id"))?,
    );
    let mut inner = state.lock();
    let project = inner
        .projects
        .get(&pid)
        .ok_or(err(StatusCode::NOT_FOUND, "no such project"))?;
    let org_id = project.org;
    let can_create = matches!(
        project.members.get(&uid),
        Some(Role::Owner | Role::Admin | Role::Editor)
    ) || matches!(
        inner.orgs.get(&org_id).and_then(|o| o.members.get(&uid)),
        Some(Role::Owner | Role::Admin | Role::Editor)
    );
    if !can_create {
        return Err(err(
            StatusCode::FORBIDDEN,
            "insufficient role to create budgets here",
        ));
    }
    // Budget inherits membership from the project/org tree (no budget-level owner).
    let (bid, room) = make_room(&body.name, body.template_key(), Some(pid), None);
    inner.budgets.insert(bid, room);
    Ok(Json(IdResp {
        id: bid.to_string(),
    }))
}

async fn add_member(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path(id): Path<String>,
    Json(body): Json<MemberReq>,
) -> ApiResult<Value> {
    let bid = parse_budget_id(&id)?;
    let target = parse_user(&body.user_id)?;
    let mut inner = state.lock();
    // Caller must be an owner/admin on the budget (via the tree).
    match effective_membership(&inner, uid, bid).map(|m| m.role) {
        Some(Role::Owner | Role::Admin) => {}
        _ => {
            return Err(err(
                StatusCode::FORBIDDEN,
                "only owner/admin can add members",
            ))
        }
    }
    let room = inner
        .budgets
        .get_mut(&bid)
        .ok_or(err(StatusCode::NOT_FOUND, "no such budget"))?;
    room.members.insert(
        target,
        Membership {
            role: body.role,
            scope: body.scope,
        },
    );
    Ok(Json(serde_json::json!({"ok": true})))
}

/// Duplicate a budget into a new one (a Version / active alternative, §5).
async fn duplicate_budget(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path(id): Path<String>,
    Json(body): Json<NameReq>,
) -> ApiResult<IdResp> {
    let bid = parse_budget_id(&id)?;
    let mut inner = state.lock();
    if effective_membership(&inner, uid, bid).is_none() {
        return Err(err(StatusCode::FORBIDDEN, "not a member"));
    }
    let src = inner
        .budgets
        .get(&bid)
        .ok_or(err(StatusCode::NOT_FOUND, "no such budget"))?;
    // Snapshot the current materialized state as the new budget's base.
    let mut snap = src.doc.budget.clone();
    snap.id = BudgetId::new();
    snap.name = body.name.clone();
    let new_id = snap.id;
    let (tx, _) = broadcast::channel(1024);
    let mut members = HashMap::new();
    members.insert(uid, Membership::full(Role::Owner));
    inner.budgets.insert(
        new_id,
        Room {
            project: None,
            base: snap.clone(),
            doc: Document::new(snap),
            ops: vec![],
            members,
            tx,
        },
    );
    Ok(Json(IdResp {
        id: new_id.to_string(),
    }))
}

#[derive(Serialize)]
struct BudgetListItem {
    id: String,
    name: String,
    role: Role,
}

/// List the budgets the caller can see (across the org/project/budget tree).
async fn list_budgets(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
) -> Json<Vec<BudgetListItem>> {
    let inner = state.lock();
    let mut out = Vec::new();
    let ids: Vec<BudgetId> = inner.budgets.keys().copied().collect();
    for bid in ids {
        if let Some(m) = effective_membership(&inner, uid, bid) {
            let name = inner.budgets[&bid].doc.budget.name.clone();
            out.push(BudgetListItem {
                id: bid.to_string(),
                name,
                role: m.role,
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Json(out)
}

/// Server-computed views (the browser client has no calc engine).
async fn get_topsheet(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path(id): Path<String>,
) -> ApiResult<TopsheetDto> {
    let bid = parse_budget_id(&id)?;
    let inner = state.lock();
    effective_membership(&inner, uid, bid).ok_or(err(StatusCode::FORBIDDEN, "not a member"))?;
    let room = inner
        .budgets
        .get(&bid)
        .ok_or(err(StatusCode::NOT_FOUND, "no such budget"))?;
    let r = evaluate(&room.doc.budget);
    Ok(Json(TopsheetDto::build(&room.doc.budget, &r)))
}

async fn get_tree(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path(id): Path<String>,
) -> ApiResult<TreeDto> {
    let bid = parse_budget_id(&id)?;
    let inner = state.lock();
    effective_membership(&inner, uid, bid).ok_or(err(StatusCode::FORBIDDEN, "not a member"))?;
    let room = inner
        .budgets
        .get(&bid)
        .ok_or(err(StatusCode::NOT_FOUND, "no such budget"))?;
    let r = evaluate(&room.doc.budget);
    Ok(Json(TreeDto::build(&room.doc.budget, &r)))
}

async fn get_national_sheet(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path(id): Path<String>,
) -> ApiResult<NationalSheetDto> {
    let bid = parse_budget_id(&id)?;
    let inner = state.lock();
    effective_membership(&inner, uid, bid).ok_or(err(StatusCode::FORBIDDEN, "not a member"))?;
    let room = inner
        .budgets
        .get(&bid)
        .ok_or(err(StatusCode::NOT_FOUND, "no such budget"))?;
    let r = evaluate(&room.doc.budget);
    Ok(Json(NationalSheetDto::build(&room.doc.budget, &r)))
}

async fn get_tools(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path(id): Path<String>,
) -> ApiResult<ToolsDto> {
    let bid = parse_budget_id(&id)?;
    let inner = state.lock();
    effective_membership(&inner, uid, bid).ok_or(err(StatusCode::FORBIDDEN, "not a member"))?;
    let room = inner
        .budgets
        .get(&bid)
        .ok_or(err(StatusCode::NOT_FOUND, "no such budget"))?;
    Ok(Json(ToolsDto::build(&room.doc.budget)))
}

#[derive(Serialize)]
struct SnapshotResp {
    base: Budget,
    ops: Vec<Op>,
}

async fn snapshot(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path(id): Path<String>,
) -> ApiResult<SnapshotResp> {
    let bid = parse_budget_id(&id)?;
    let inner = state.lock();
    let m =
        effective_membership(&inner, uid, bid).ok_or(err(StatusCode::FORBIDDEN, "not a member"))?;
    let room = inner
        .budgets
        .get(&bid)
        .ok_or(err(StatusCode::NOT_FOUND, "no such budget"))?;
    let ops = room
        .ops
        .iter()
        .filter(|op| can_read(&m, &op.kind, &room.doc.budget))
        .cloned()
        .collect();
    Ok(Json(SnapshotResp {
        base: room.base.clone(),
        ops,
    }))
}

#[derive(Deserialize)]
struct SubmitOpReq {
    op: Op,
}

#[derive(Serialize)]
struct SubmitResp {
    applied: bool,
    result: String,
}

async fn submit_op(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path(id): Path<String>,
    Json(body): Json<SubmitOpReq>,
) -> ApiResult<SubmitResp> {
    let bid = parse_budget_id(&id)?;
    let mut inner = state.lock();
    let m =
        effective_membership(&inner, uid, bid).ok_or(err(StatusCode::FORBIDDEN, "not a member"))?;
    let room = inner
        .budgets
        .get_mut(&bid)
        .ok_or(err(StatusCode::NOT_FOUND, "no such budget"))?;

    if let Err(d) = can_write(&m, &body.op, &room.doc.budget) {
        return Err(err(StatusCode::FORBIDDEN, d.to_string()));
    }
    let outcome = room.doc.apply(&body.op);
    if matches!(outcome, ApplyResult::Applied | ApplyResult::Buffered) {
        room.ops.push(body.op.clone());
        let _ = room.tx.send(ServerMsg::Op {
            op: Box::new(body.op.clone()),
        });
    }
    Ok(Json(SubmitResp {
        applied: matches!(outcome, ApplyResult::Applied | ApplyResult::Buffered),
        result: format!("{outcome:?}"),
    }))
}

// ---------------------------------------------------------------------------
// WebSocket: ops (scope-filtered) + ephemeral presence
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct WsQuery {
    token: String,
}

async fn ws_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(q): Query<WsQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    let bid = match parse_budget_id(&id) {
        Ok(b) => b,
        Err(e) => return e.into_response(),
    };
    let uid = match auth::verify_token(&state.jwt_secret, &q.token)
        .ok()
        .and_then(|c| Uuid::parse_str(&c.sub).ok())
        .map(UserId::from_uuid)
    {
        Some(u) => u,
        None => return (StatusCode::UNAUTHORIZED, "bad token").into_response(),
    };
    let (rx, membership) = {
        let inner = state.lock();
        let Some(m) = effective_membership(&inner, uid, bid) else {
            return (StatusCode::FORBIDDEN, "not a member").into_response();
        };
        let Some(room) = inner.budgets.get(&bid) else {
            return (StatusCode::NOT_FOUND, "no such budget").into_response();
        };
        (room.tx.subscribe(), m)
    };
    ws.on_upgrade(move |socket| ws_loop(socket, state, bid, uid, membership, rx))
}

async fn ws_loop(
    socket: WebSocket,
    state: Arc<AppState>,
    bid: BudgetId,
    uid: UserId,
    membership: Membership,
    mut rx: broadcast::Receiver<ServerMsg>,
) {
    let (mut sink, mut stream) = socket.split();
    loop {
        tokio::select! {
            incoming = stream.next() => match incoming {
                Some(Ok(Message::Text(t))) => {
                    // Treat any client text as an ephemeral presence payload and
                    // fan it out (not persisted).
                    if let Ok(payload) = serde_json::from_str::<Value>(&t) {
                        let msg = ServerMsg::Presence { user: uid.to_string(), payload };
                        if let Some(room) = state.lock().budgets.get(&bid) {
                            let _ = room.tx.send(msg);
                        }
                    }
                }
                Some(Ok(Message::Close(_))) | None => break,
                _ => {}
            },
            outgoing = rx.recv() => match outgoing {
                Ok(msg) => {
                    let visible = match &msg {
                        ServerMsg::Presence { .. } => true,
                        ServerMsg::Op { op } => {
                            let inner = state.lock();
                            inner.budgets.get(&bid).map(|r| can_read(&membership, &op.kind, &r.doc.budget)).unwrap_or(false)
                        }
                    };
                    if visible {
                        if let Ok(text) = serde_json::to_string(&msg) {
                            if sink.send(Message::Text(text)).await.is_err() {
                                break;
                            }
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => break,
            },
        }
    }
}

fn parse_budget_id(s: &str) -> Result<BudgetId, (StatusCode, String)> {
    Uuid::parse_str(s)
        .map(BudgetId::from_uuid)
        .map_err(|_| err(StatusCode::BAD_REQUEST, "bad budget id"))
}

// ---------------------------------------------------------------------------
// Convenience mutation endpoints: the server mints + RBAC-checks + applies +
// broadcasts the op, so a calc-engine-less browser client can edit via plain
// REST. (The desktop app does the same locally via budgetcut-store.)
// ---------------------------------------------------------------------------

/// Mint a server op for `kind`, enforce the caller's write RBAC, apply it
/// authoritatively, persist + broadcast. The single mutation choke point.
fn apply_member_op(
    inner: &mut Inner,
    uid: UserId,
    bid: BudgetId,
    kind: OpKind,
) -> Result<ApplyResult, (StatusCode, String)> {
    let m =
        effective_membership(inner, uid, bid).ok_or(err(StatusCode::FORBIDDEN, "not a member"))?;
    let hlc = inner.clock.tick(now_ms());
    let op = Op::new(hlc, uid, kind);
    let room = inner
        .budgets
        .get_mut(&bid)
        .ok_or(err(StatusCode::NOT_FOUND, "no such budget"))?;
    if let Err(d) = can_write(&m, &op, &room.doc.budget) {
        return Err(err(StatusCode::FORBIDDEN, d.to_string()));
    }
    let outcome = room.doc.apply(&op);
    if matches!(outcome, ApplyResult::Applied | ApplyResult::Buffered) {
        room.ops.push(op.clone());
        let _ = room.tx.send(ServerMsg::Op { op: Box::new(op) });
    }
    Ok(outcome)
}

fn parse_formula(v: &str) -> Formula {
    let t = v.trim();
    if let Some(expr) = t.strip_prefix('=') {
        Formula::Expr(expr.trim().to_string())
    } else {
        Formula::Const(Decimal::from_str(t).unwrap_or(Decimal::ZERO))
    }
}

#[derive(Deserialize)]
struct SetGlobalReq {
    name: String,
    value: String,
}

async fn set_global(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path(id): Path<String>,
    Json(body): Json<SetGlobalReq>,
) -> ApiResult<Value> {
    let bid = parse_budget_id(&id)?;
    let mut inner = state.lock();
    let gid = inner
        .budgets
        .get(&bid)
        .and_then(|r| {
            r.doc
                .budget
                .globals
                .values()
                .find(|g| g.name == body.name)
                .map(|g| g.id)
        })
        .ok_or(err(StatusCode::NOT_FOUND, "global yok"))?;
    apply_member_op(
        &mut inner,
        uid,
        bid,
        OpKind::SetGlobalValue {
            global: gid,
            value: parse_formula(&body.value),
        },
    )?;
    Ok(Json(serde_json::json!({"ok": true})))
}

#[derive(Deserialize)]
struct SetFieldReq {
    field: String,
    value: String,
}

async fn set_detail_field(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path((id, detail)): Path<(String, String)>,
    Json(body): Json<SetFieldReq>,
) -> ApiResult<Value> {
    let bid = parse_budget_id(&id)?;
    let did = DetailId::from_uuid(
        Uuid::parse_str(&detail).map_err(|_| err(StatusCode::BAD_REQUEST, "bad detail id"))?,
    );
    let field = match body.field.as_str() {
        "description" => DetailField::Description(body.value),
        "amount" => DetailField::Amount(parse_formula(&body.value)),
        "multiplier" => DetailField::Multiplier(parse_formula(&body.value)),
        "rate" => DetailField::Rate(parse_formula(&body.value)),
        other => {
            return Err(err(
                StatusCode::BAD_REQUEST,
                format!("bilinmeyen alan: {other}"),
            ))
        }
    };
    let mut inner = state.lock();
    apply_member_op(
        &mut inner,
        uid,
        bid,
        OpKind::SetDetailField { detail: did, field },
    )?;
    Ok(Json(serde_json::json!({"ok": true})))
}

#[derive(Deserialize)]
struct FringeReq {
    code: String,
    rate: Option<String>,
}

#[derive(Deserialize)]
struct SetFringesReq {
    fringes: Vec<FringeReq>,
}

/// Replace a line's applied fringes (Apply-Tools, §5). Codes resolve to fringe
/// ids against the budget's Library; an optional per-line rate override mirrors
/// the spreadsheet's VERGİ/KOM columns.
async fn set_detail_fringes(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path((id, detail)): Path<(String, String)>,
    Json(body): Json<SetFringesReq>,
) -> ApiResult<Value> {
    let bid = parse_budget_id(&id)?;
    let did = DetailId::from_uuid(
        Uuid::parse_str(&detail).map_err(|_| err(StatusCode::BAD_REQUEST, "bad detail id"))?,
    );
    let mut inner = state.lock();
    let applied: Vec<AppliedFringe> = {
        let b = &inner
            .budgets
            .get(&bid)
            .ok_or(err(StatusCode::NOT_FOUND, "no such budget"))?
            .doc
            .budget;
        let mut v = Vec::new();
        for fr in &body.fringes {
            let fid = b
                .fringes
                .values()
                .find(|f| f.code == fr.code)
                .map(|f| f.id)
                .ok_or(err(
                    StatusCode::NOT_FOUND,
                    format!("fringe yok: {}", fr.code),
                ))?;
            v.push(match &fr.rate {
                Some(r) => {
                    AppliedFringe::with_rate(fid, Decimal::from_str(r).unwrap_or(Decimal::ZERO))
                }
                None => AppliedFringe::new(fid),
            });
        }
        v
    };
    apply_member_op(
        &mut inner,
        uid,
        bid,
        OpKind::SetDetailField {
            detail: did,
            field: DetailField::Fringes(applied),
        },
    )?;
    Ok(Json(serde_json::json!({"ok": true})))
}

#[derive(Deserialize)]
struct AddLineReq {
    account: String,
}

async fn add_line(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path(id): Path<String>,
    Json(body): Json<AddLineReq>,
) -> ApiResult<Value> {
    let bid = parse_budget_id(&id)?;
    let aid = AccountId::from_uuid(
        Uuid::parse_str(&body.account)
            .map_err(|_| err(StatusCode::BAD_REQUEST, "bad account id"))?,
    );
    let mut inner = state.lock();
    let (unit, cur, pos) = {
        let b = &inner
            .budgets
            .get(&bid)
            .ok_or(err(StatusCode::NOT_FOUND, "no such budget"))?
            .doc
            .budget;
        let unit = b
            .units
            .values()
            .find(|u| u.code == "ADET")
            .or_else(|| b.units.values().next())
            .map(|u| u.id)
            .ok_or(err(StatusCode::BAD_REQUEST, "bütçede birim yok"))?;
        let pos = b
            .details_of(aid)
            .last()
            .map(|d| d.position + Decimal::ONE)
            .unwrap_or(Decimal::ONE);
        (unit, b.base_currency, pos)
    };
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
    let new_id = d.id.to_string();
    apply_member_op(&mut inner, uid, bid, OpKind::InsertDetail(d))?;
    Ok(Json(serde_json::json!({ "id": new_id })))
}

async fn remove_line(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path((id, detail)): Path<(String, String)>,
) -> ApiResult<Value> {
    let bid = parse_budget_id(&id)?;
    let did = DetailId::from_uuid(
        Uuid::parse_str(&detail).map_err(|_| err(StatusCode::BAD_REQUEST, "bad detail id"))?,
    );
    let mut inner = state.lock();
    apply_member_op(&mut inner, uid, bid, OpKind::RemoveDetail(did))?;
    Ok(Json(serde_json::json!({"ok": true})))
}

// ---------------------------------------------------------------------------
// MMB-parity analytics endpoints: amort/pattern series, incentive estimation,
// budget comparison, reusable libraries, accounting CSV. Read endpoints require
// membership; library-apply requires write (it emits ops).
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct SeriesReq {
    episodes: u32,
    #[serde(default)]
    amortized: Vec<AmortInput>,
}

/// Amort & pattern series: this budget's net total is one pattern episode.
async fn post_series(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path(id): Path<String>,
    Json(body): Json<SeriesReq>,
) -> ApiResult<SeriesSummaryDto> {
    let bid = parse_budget_id(&id)?;
    let inner = state.lock();
    effective_membership(&inner, uid, bid).ok_or(err(StatusCode::FORBIDDEN, "not a member"))?;
    let room = inner
        .budgets
        .get(&bid)
        .ok_or(err(StatusCode::NOT_FOUND, "no such budget"))?;
    let r = evaluate(&room.doc.budget);
    Ok(Json(series_summary_for(&r, body.episodes, &body.amortized)))
}

#[derive(Deserialize)]
struct IncentiveQuery {
    qualifying: Option<String>,
}

/// Incentive estimates for the Turkish presets. Qualifying spend defaults to
/// the budget's net total; the client may override it.
async fn get_incentives(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path(id): Path<String>,
    Query(q): Query<IncentiveQuery>,
) -> ApiResult<IncentiveReportDto> {
    let bid = parse_budget_id(&id)?;
    let inner = state.lock();
    effective_membership(&inner, uid, bid).ok_or(err(StatusCode::FORBIDDEN, "not a member"))?;
    let room = inner
        .budgets
        .get(&bid)
        .ok_or(err(StatusCode::NOT_FOUND, "no such budget"))?;
    let qualifying = match q.qualifying {
        Some(s) => Decimal::from_str(s.trim())
            .map_err(|_| err(StatusCode::BAD_REQUEST, "geçersiz tutar"))?,
        None => evaluate(&room.doc.budget).net_total,
    };
    Ok(Json(IncentiveReportDto::turkish_for(qualifying)))
}

/// Accounting GL export (text/csv) — the SmartAccounting hand-off analog.
async fn get_accounting_csv(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path(id): Path<String>,
) -> Result<Response, (StatusCode, String)> {
    let bid = parse_budget_id(&id)?;
    let inner = state.lock();
    effective_membership(&inner, uid, bid).ok_or(err(StatusCode::FORBIDDEN, "not a member"))?;
    let room = inner
        .budgets
        .get(&bid)
        .ok_or(err(StatusCode::NOT_FOUND, "no such budget"))?;
    let csv = budgetcut_export::accounting_csv(&room.doc.budget);
    Ok((
        [(axum::http::header::CONTENT_TYPE, "text/csv; charset=utf-8")],
        csv,
    )
        .into_response())
}

#[derive(Deserialize)]
struct CompareQuery {
    a: String,
    b: String,
}

/// Compare two budgets/versions/locations the caller can read (by category #).
async fn get_compare(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Query(q): Query<CompareQuery>,
) -> ApiResult<ComparisonDto> {
    let aid = parse_budget_id(&q.a)?;
    let b_id = parse_budget_id(&q.b)?;
    let inner = state.lock();
    effective_membership(&inner, uid, aid).ok_or(err(StatusCode::FORBIDDEN, "A: not a member"))?;
    effective_membership(&inner, uid, b_id).ok_or(err(StatusCode::FORBIDDEN, "B: not a member"))?;
    let a = inner
        .budgets
        .get(&aid)
        .ok_or(err(StatusCode::NOT_FOUND, "A yok"))?
        .doc
        .budget
        .clone();
    let b = inner
        .budgets
        .get(&b_id)
        .ok_or(err(StatusCode::NOT_FOUND, "B yok"))?
        .doc
        .budget
        .clone();
    Ok(Json(ComparisonDto::build(
        &a,
        &evaluate(&a),
        &b,
        &evaluate(&b),
    )))
}

#[derive(Serialize)]
struct LibraryItem {
    id: String,
    name: String,
    fringes: usize,
    globals: usize,
}

/// List the caller's saved setup libraries.
async fn list_libraries(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
) -> Json<Vec<LibraryItem>> {
    let inner = state.lock();
    let mut out: Vec<LibraryItem> = inner
        .libraries
        .iter()
        .filter(|(_, l)| l.owner == uid)
        .map(|(id, l)| LibraryItem {
            id: id.to_string(),
            name: l.name.clone(),
            fringes: l.lib.fringes.len(),
            globals: l.lib.globals.len(),
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Json(out)
}

#[derive(Deserialize)]
struct SaveLibraryReq {
    budget_id: String,
    name: String,
}

/// Extract a setup library from a budget the caller can read; save under them.
async fn save_library(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Json(body): Json<SaveLibraryReq>,
) -> ApiResult<IdResp> {
    let bid = parse_budget_id(&body.budget_id)?;
    let mut inner = state.lock();
    effective_membership(&inner, uid, bid).ok_or(err(StatusCode::FORBIDDEN, "not a member"))?;
    let lib = {
        let b = &inner
            .budgets
            .get(&bid)
            .ok_or(err(StatusCode::NOT_FOUND, "no such budget"))?
            .doc
            .budget;
        SetupLibrary::extract(&body.name, b)
    };
    let id = Uuid::now_v7();
    inner.libraries.insert(
        id,
        StoredLibrary {
            name: body.name.clone(),
            owner: uid,
            lib,
        },
    );
    Ok(Json(IdResp { id: id.to_string() }))
}

#[derive(Serialize)]
struct ApplyLibraryResp {
    added_fringes: usize,
    added_globals: usize,
}

/// Apply a saved library into a budget. Emits InsertFringe/InsertGlobal ops so
/// the change flows through the authoritative op-log and broadcasts. Caller must
/// own the library and have write access to the budget; items already present
/// (by code/name) are skipped.
async fn apply_library(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path((id, lib)): Path<(String, String)>,
) -> ApiResult<ApplyLibraryResp> {
    let bid = parse_budget_id(&id)?;
    let lib_id =
        Uuid::parse_str(&lib).map_err(|_| err(StatusCode::BAD_REQUEST, "bad library id"))?;
    let mut inner = state.lock();

    let stored = inner
        .libraries
        .get(&lib_id)
        .ok_or(err(StatusCode::NOT_FOUND, "library yok"))?;
    if stored.owner != uid {
        return Err(err(StatusCode::FORBIDDEN, "not your library"));
    }
    let lib = stored.lib.clone();

    let (have_fringes, have_globals): (
        std::collections::BTreeSet<String>,
        std::collections::BTreeSet<String>,
    ) = {
        let b = &inner
            .budgets
            .get(&bid)
            .ok_or(err(StatusCode::NOT_FOUND, "no such budget"))?
            .doc
            .budget;
        (
            b.fringes.values().map(|f| f.code.clone()).collect(),
            b.globals.values().map(|g| g.name.clone()).collect(),
        )
    };

    let mut added_fringes = 0;
    for f in &lib.fringes {
        if !have_fringes.contains(&f.code) {
            let mut f: Fringe = f.clone();
            f.id = FringeId::new();
            apply_member_op(&mut inner, uid, bid, OpKind::InsertFringe(f))?;
            added_fringes += 1;
        }
    }
    let mut added_globals = 0;
    for g in &lib.globals {
        if !have_globals.contains(&g.name) {
            let mut g: Global = g.clone();
            g.id = GlobalId::new();
            apply_member_op(&mut inner, uid, bid, OpKind::InsertGlobal(g))?;
            added_globals += 1;
        }
    }
    Ok(Json(ApplyLibraryResp {
        added_fringes,
        added_globals,
    }))
}

// --- Actuals / EFC (§16 Phase 3) ---

/// Estimate-vs-actual / EFC report + invoice lines.
///
/// KNOWN LIMITATION (consistent with every REST read view here — topsheet,
/// tree, incentives, compare): the report is computed over the whole budget for
/// any member. Department **scope** is enforced on the authoritative op stream
/// and snapshot (via `can_read`/`op_category`), but the server-rendered report
/// DTOs are not yet scope-filtered, so a department-scoped member sees
/// cross-department aggregates (and, here, per-vendor invoice lines). Scoping
/// the report DTOs by `membership.scope` is tracked as a cross-cutting follow-up
/// rather than patched into this one endpoint.
async fn get_actuals(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path(id): Path<String>,
) -> ApiResult<ActualsReportDto> {
    let bid = parse_budget_id(&id)?;
    let inner = state.lock();
    effective_membership(&inner, uid, bid).ok_or(err(StatusCode::FORBIDDEN, "not a member"))?;
    let room = inner
        .budgets
        .get(&bid)
        .ok_or(err(StatusCode::NOT_FOUND, "no such budget"))?;
    let r = evaluate(&room.doc.budget);
    Ok(Json(ActualsReportDto::build(&room.doc.budget, &r)))
}

#[derive(Deserialize)]
struct AddActualReq {
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
    /// Service type → tevkifat (VAT-withholding) rate, resolved server-side.
    #[serde(default)]
    tevkifat_kind: Option<String>,
}

fn parse_rate(s: &str) -> Decimal {
    Decimal::from_str(s.trim()).unwrap_or(Decimal::ZERO)
}

/// Record an actual (invoice/expense) against an account → emits an op.
async fn post_actual(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path(id): Path<String>,
    Json(body): Json<AddActualReq>,
) -> ApiResult<IdResp> {
    let bid = parse_budget_id(&id)?;
    let account = AccountId::from_uuid(
        Uuid::parse_str(&body.account)
            .map_err(|_| err(StatusCode::BAD_REQUEST, "bad account id"))?,
    );
    let net = Decimal::from_str(body.net.trim())
        .map_err(|_| err(StatusCode::BAD_REQUEST, "geçersiz net"))?;
    let actual = Actual {
        id: Uuid::now_v7(),
        account,
        date: body.date,
        vendor: body.vendor,
        description: body.description,
        net,
        stopaj_rate: parse_rate(&body.stopaj_rate),
        kdv_rate: parse_rate(&body.kdv_rate),
        tevkifat_rate: body
            .tevkifat_kind
            .as_deref()
            .map(tevkifat_rate)
            .unwrap_or(Decimal::ZERO),
    };
    let new_id = actual.id.to_string();
    let mut inner = state.lock();
    apply_member_op(&mut inner, uid, bid, OpKind::InsertActual(actual))?;
    Ok(Json(IdResp { id: new_id }))
}

async fn delete_actual(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path((id, actual)): Path<(String, String)>,
) -> ApiResult<Value> {
    let bid = parse_budget_id(&id)?;
    let aid =
        Uuid::parse_str(&actual).map_err(|_| err(StatusCode::BAD_REQUEST, "bad actual id"))?;
    let mut inner = state.lock();
    apply_member_op(&mut inner, uid, bid, OpKind::RemoveActual(aid))?;
    Ok(Json(serde_json::json!({"ok": true})))
}

// --- Expense settlement / "Hesap Kapama" (§16) ---

#[derive(Deserialize)]
struct SettlementQuery {
    advance: Option<String>,
}

/// Expense settlement (per-category rollup + advance reconciliation). `advance`
/// (the cash handed to the holder) is supplied per request; defaults to 0.
async fn get_settlement(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path(id): Path<String>,
    Query(q): Query<SettlementQuery>,
) -> ApiResult<SettlementReportDto> {
    let bid = parse_budget_id(&id)?;
    let inner = state.lock();
    effective_membership(&inner, uid, bid).ok_or(err(StatusCode::FORBIDDEN, "not a member"))?;
    let room = inner
        .budgets
        .get(&bid)
        .ok_or(err(StatusCode::NOT_FOUND, "no such budget"))?;
    let advance = q
        .advance
        .as_deref()
        .map(|s| Decimal::from_str(s.trim()).unwrap_or(Decimal::ZERO))
        .unwrap_or(Decimal::ZERO);
    Ok(Json(SettlementReportDto::build(&room.doc.budget, advance)))
}

#[derive(Deserialize)]
struct AddReceiptReq {
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

/// Record a settlement receipt (fiş) → emits an op.
async fn post_receipt(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path(id): Path<String>,
    Json(body): Json<AddReceiptReq>,
) -> ApiResult<IdResp> {
    let bid = parse_budget_id(&id)?;
    let gross = Decimal::from_str(body.gross.trim())
        .map_err(|_| err(StatusCode::BAD_REQUEST, "geçersiz tutar"))?;
    let receipt = Receipt {
        id: Uuid::now_v7(),
        date: body.date,
        vendor: body.vendor,
        receipt_no: body.receipt_no,
        category: body.category,
        description: body.description,
        gross,
        kdv_rate: parse_rate(&body.kdv_rate),
    };
    let new_id = receipt.id.to_string();
    let mut inner = state.lock();
    apply_member_op(&mut inner, uid, bid, OpKind::InsertReceipt(receipt))?;
    Ok(Json(IdResp { id: new_id }))
}

async fn delete_receipt(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path((id, receipt)): Path<(String, String)>,
) -> ApiResult<Value> {
    let bid = parse_budget_id(&id)?;
    let rid =
        Uuid::parse_str(&receipt).map_err(|_| err(StatusCode::BAD_REQUEST, "bad receipt id"))?;
    let mut inner = state.lock();
    apply_member_op(&mut inner, uid, bid, OpKind::RemoveReceipt(rid))?;
    Ok(Json(serde_json::json!({"ok": true})))
}

// --- Scheduling / stripboard (§16 Phase 2) ---

/// Stripboard + Day-Out-of-Days.
async fn get_schedule(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path(id): Path<String>,
) -> ApiResult<ScheduleDto> {
    let bid = parse_budget_id(&id)?;
    let inner = state.lock();
    effective_membership(&inner, uid, bid).ok_or(err(StatusCode::FORBIDDEN, "not a member"))?;
    let room = inner
        .budgets
        .get(&bid)
        .ok_or(err(StatusCode::NOT_FOUND, "no such budget"))?;
    Ok(Json(ScheduleDto::build(&room.doc.budget)))
}

#[derive(Deserialize)]
struct AddStripReq {
    day: u32,
    scene: String,
    #[serde(default)]
    set: String,
    #[serde(default)]
    eighths: u32,
    #[serde(default)]
    elements: Vec<String>,
}

/// Add a stripboard strip → emits an op.
async fn post_strip(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path(id): Path<String>,
    Json(body): Json<AddStripReq>,
) -> ApiResult<IdResp> {
    let bid = parse_budget_id(&id)?;
    let strip = Strip {
        id: Uuid::now_v7(),
        day: body.day.max(1), // 1-based; reject the phantom day 0
        scene: body.scene,
        set: body.set,
        eighths: body.eighths,
        elements: body.elements,
    };
    let new_id = strip.id.to_string();
    let mut inner = state.lock();
    apply_member_op(&mut inner, uid, bid, OpKind::InsertStrip(strip))?;
    Ok(Json(IdResp { id: new_id }))
}

async fn delete_strip(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path((id, strip)): Path<(String, String)>,
) -> ApiResult<Value> {
    let bid = parse_budget_id(&id)?;
    let sid = Uuid::parse_str(&strip).map_err(|_| err(StatusCode::BAD_REQUEST, "bad strip id"))?;
    let mut inner = state.lock();
    apply_member_op(&mut inner, uid, bid, OpKind::RemoveStrip(sid))?;
    Ok(Json(serde_json::json!({"ok": true})))
}

// --- Purchase orders + approval workflow (§ commitments) ---

/// PO list + committed totals.
async fn get_purchase_orders(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path(id): Path<String>,
) -> ApiResult<PurchaseOrdersDto> {
    let bid = parse_budget_id(&id)?;
    let inner = state.lock();
    effective_membership(&inner, uid, bid).ok_or(err(StatusCode::FORBIDDEN, "not a member"))?;
    let room = inner
        .budgets
        .get(&bid)
        .ok_or(err(StatusCode::NOT_FOUND, "no such budget"))?;
    Ok(Json(PurchaseOrdersDto::build(&room.doc.budget)))
}

#[derive(Deserialize)]
struct AddPoReq {
    account: String,
    #[serde(default)]
    date: String,
    #[serde(default)]
    vendor: String,
    #[serde(default)]
    description: String,
    amount: String,
}

/// Create a Draft PO → emits an op.
async fn post_po(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path(id): Path<String>,
    Json(body): Json<AddPoReq>,
) -> ApiResult<IdResp> {
    let bid = parse_budget_id(&id)?;
    let account = AccountId::from_uuid(
        Uuid::parse_str(&body.account)
            .map_err(|_| err(StatusCode::BAD_REQUEST, "bad account id"))?,
    );
    let amount = Decimal::from_str(body.amount.trim())
        .map_err(|_| err(StatusCode::BAD_REQUEST, "geçersiz tutar"))?;
    let po = PurchaseOrder {
        id: Uuid::now_v7(),
        account,
        date: body.date,
        vendor: body.vendor,
        description: body.description,
        amount,
        status: POStatus::Draft,
    };
    let new_id = po.id.to_string();
    let mut inner = state.lock();
    apply_member_op(&mut inner, uid, bid, OpKind::InsertPurchaseOrder(po))?;
    Ok(Json(IdResp { id: new_id }))
}

fn read_po(inner: &Inner, bid: BudgetId, pid: Uuid) -> Result<PurchaseOrder, (StatusCode, String)> {
    inner
        .budgets
        .get(&bid)
        .ok_or(err(StatusCode::NOT_FOUND, "no such budget"))?
        .doc
        .budget
        .purchase_orders
        .get(&pid)
        .cloned()
        .ok_or(err(StatusCode::NOT_FOUND, "PO yok"))
}

/// Approve a PO (Draft → Approved); LWW whole-record re-insert.
async fn approve_po(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path((id, po)): Path<(String, String)>,
) -> ApiResult<Value> {
    let bid = parse_budget_id(&id)?;
    let pid = Uuid::parse_str(&po).map_err(|_| err(StatusCode::BAD_REQUEST, "bad po id"))?;
    let mut inner = state.lock();
    let mut p = read_po(&inner, bid, pid)?;
    p.status = POStatus::Approved;
    apply_member_op(&mut inner, uid, bid, OpKind::InsertPurchaseOrder(p))?;
    Ok(Json(serde_json::json!({"ok": true})))
}

/// Convert a PO to an actual (→ Converted). Emits an InsertActual + the status
/// update so the committed cost becomes a recorded actual.
async fn convert_po(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path((id, po)): Path<(String, String)>,
) -> ApiResult<Value> {
    let bid = parse_budget_id(&id)?;
    let pid = Uuid::parse_str(&po).map_err(|_| err(StatusCode::BAD_REQUEST, "bad po id"))?;
    let mut inner = state.lock();
    let mut p = read_po(&inner, bid, pid)?;
    if p.status == POStatus::Converted {
        return Ok(Json(serde_json::json!({"ok": true, "already": true})));
    }
    let actual = Actual {
        // Deterministic id from the PO so a concurrent convert on another node
        // re-inserts the SAME actual (LWW merge) instead of double-counting it.
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
    apply_member_op(&mut inner, uid, bid, OpKind::InsertActual(actual))?;
    p.status = POStatus::Converted;
    apply_member_op(&mut inner, uid, bid, OpKind::InsertPurchaseOrder(p))?;
    Ok(Json(serde_json::json!({"ok": true})))
}

async fn delete_po(
    State(state): State<Arc<AppState>>,
    AuthUser(uid): AuthUser,
    Path((id, po)): Path<(String, String)>,
) -> ApiResult<Value> {
    let bid = parse_budget_id(&id)?;
    let pid = Uuid::parse_str(&po).map_err(|_| err(StatusCode::BAD_REQUEST, "bad po id"))?;
    let mut inner = state.lock();
    apply_member_op(&mut inner, uid, bid, OpKind::RemovePurchaseOrder(pid))?;
    Ok(Json(serde_json::json!({"ok": true})))
}
