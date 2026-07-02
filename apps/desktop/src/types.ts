// TS mirrors of the budgetcut-store DTOs (camelCase fields match serde output).

export interface FormulaDto {
  is_expr: boolean;
  text: string;
}

export interface TopsheetCategory {
  id: string;
  number: string;
  name: string;
  atl_btl: "ATL" | "BTL" | null;
  subtotal: string;
  fringe_total: string;
  total: string;
}

export interface Topsheet {
  budget_name: string;
  base_currency: string;
  categories: TopsheetCategory[];
  atl_total: string;
  btl_total: string;
  fringes_total: string;
  grand_total: string;
  charges_total: string;
  credits_total: string;
  net_total: string;
  error_count: number;
}

export interface AppliedFringeView {
  code: string;
  rate: string;
}

export interface DetailRow {
  id: string;
  description: string;
  name: string | null;
  amount: FormulaDto;
  multiplier: FormulaDto;
  rate: FormulaDto;
  unit: string;
  currency: string;
  fringes: AppliedFringeView[];
  subtotal: string;
  fringe_total: string;
  line_total: string;
  error: boolean;
}

export interface AccountNode {
  id: string;
  number: string;
  name: string;
  subtotal: string;
  fringe_total: string;
  total: string;
  details: DetailRow[];
}

export interface CategoryNode {
  id: string;
  number: string;
  name: string;
  atl_btl: "ATL" | "BTL" | null;
  subtotal: string;
  fringe_total: string;
  total: string;
  accounts: AccountNode[];
}

export interface Tree {
  budget_name: string;
  categories: CategoryNode[];
}

export interface FringeTool {
  code: string;
  name: string;
  kind: string;
  mode: string;
  rate: string;
  posting_level: string;
}
export interface GlobalTool {
  name: string;
  description: string;
  value: FormulaDto;
}
export interface UnitTool {
  code: string;
  name: string;
  factor: string;
}
export interface Tools {
  fringes: FringeTool[];
  globals: GlobalTool[];
  units: UnitTool[];
}

// --- MMB-parity analytics DTOs (mirror budgetcut-core::view) ---

export interface AmortInputRow {
  label: string;
  total: string; // decimal string
  over_episodes: number;
}
export interface SeriesSummary {
  episodes: number;
  pattern_episode: string;
  pattern_total: string;
  amort_total: string;
  series_total: string;
  per_episode_all_in: string;
}
export interface ComparisonRow {
  number: string;
  name: string;
  a_total: string;
  b_total: string;
  diff: string;
}
export interface Comparison {
  a_name: string;
  b_name: string;
  rows: ComparisonRow[];
  a_total: string;
  b_total: string;
  diff: string;
}
export interface IncentiveLine {
  jurisdiction: string;
  rate: string; // fraction, e.g. "0.3"
  cap: string | null;
  estimate: string;
}
export interface IncentiveReport {
  qualifying_spend: string;
  lines: IncentiveLine[];
}
export interface LibraryItem {
  id: string;
  name: string;
  fringes: number;
  globals: number;
}

// --- Actuals / EFC (mirror budgetcut-core::view) ---

export interface ActualLine {
  id: string;
  account_number: string;
  account_name: string;
  vendor: string;
  description: string;
  net: string;
  brut: string;
  stopaj: string;
  kdv: string;
  tevkifat_kdv: string;
  payable: string;
}
export interface ActualVarianceRow {
  account_number: string;
  account_name: string;
  estimate: string;
  actual: string;
  variance: string;
  efc: string;
  over: boolean;
}
export interface ActualsReport {
  rows: ActualVarianceRow[];
  estimate_total: string;
  actual_total: string;
  variance_total: string;
  efc_total: string;
  lines: ActualLine[];
}
export interface AddActualInput {
  account: string;
  date?: string;
  vendor?: string;
  description?: string;
  net: string; // decimal string
  stopaj_rate?: string; // fraction string, e.g. "0.17"
  kdv_rate?: string;
  tevkifat_kind?: string | null;
}

// --- Settlement / Hesap Kapama (mirror budgetcut-core::view) ---

export interface ReceiptLine {
  id: string;
  date: string;
  vendor: string;
  receipt_no: string;
  category: string;
  description: string;
  gross: string;
  kdv: string;
  net: string;
}
export interface SettlementCategory {
  category: string;
  gross: string;
  kdv: string;
  net: string;
}
export interface SettlementReport {
  categories: SettlementCategory[];
  gross_total: string;
  kdv_total: string;
  net_total: string;
  advance: string;
  balance: string;
  refund: boolean;
  lines: ReceiptLine[];
}
export interface AddReceiptInput {
  date?: string;
  vendor?: string;
  receipt_no?: string;
  category: string;
  description?: string;
  gross: string; // KDV-inclusive decimal string
  kdv_rate?: string; // fraction string, e.g. "0.10"
}

// --- Scheduling / stripboard (mirror budgetcut-core::view) ---

export interface StripRow {
  id: string;
  day: number;
  scene: string;
  set: string;
  eighths: number;
  elements: string[];
}
export interface DoodRow {
  element: string;
  start_day: number;
  finish_day: number;
  work_days: number;
  hold_days: number;
}
export interface Schedule {
  strips: StripRow[];
  dood: DoodRow[];
  total_days: number;
  total_eighths: number;
}
export interface AddStripInput {
  day: number;
  scene: string;
  set?: string;
  eighths?: number;
  elements?: string[];
}

// --- Purchase orders / commitments (mirror budgetcut-core::view) ---

export type POStatus = "draft" | "approved" | "converted";
export interface PurchaseOrder {
  id: string;
  account_number: string;
  account_name: string;
  date: string;
  vendor: string;
  description: string;
  amount: string;
  status: POStatus;
}
export interface PurchaseOrders {
  orders: PurchaseOrder[];
  draft_total: string;
  approved_total: string;
  converted_total: string;
  committed_total: string;
}
export interface AddPoInput {
  account: string;
  date?: string;
  vendor?: string;
  description?: string;
  amount: string;
}

// --- Receipt-photo → Actuals ("Fiş ile Otomatik Fatura Kapama"), native-only ---

export interface SettingsView {
  api_key_set: boolean;
  model: string;
}

export interface BBox {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface SegmentResult {
  width: number;
  height: number;
  boxes: BBox[];
}

export interface AccountHint {
  number: string;
  name: string;
}

export interface ReceiptFields {
  tedarikci: string | null;
  tarih: string | null;
  tutar_net: number | null;
  kdv_tutari: number | null;
  toplam_tutar: number | null;
  para_birimi: string;
  aciklama_onerisi: string | null;
  hesap_kodu_onerisi: string | null;
  alan_guven_skorlari: Record<string, number>;
  ham_ocr_metni: string;
}

export interface ExtractResult {
  fields: ReceiptFields;
  crop_data_url: string;
}

// --- Netflix reporting suite, mirror view::Netflix*Dto ---

export interface NetflixHeaderInput {
  budget_version?: string;
  episodes?: number | null;
  min_per_episode?: string;
  exec_producers?: string;
  director?: string;
  prepared_by?: string;
  budget_date?: string;
  shoot_weeks?: string;
  post_weeks?: string;
  fx_note?: string;
  signed_agreement?: string;
  period_no?: string;
  period_start?: string;
  period_end?: string;
}

export interface NetflixTopsheetRow {
  number: string;
  name: string;
  subtotal: string;
  fringe_total: string;
  total: string;
}
export interface NetflixSection {
  group_key: string;
  atl_btl: string;
  rows: NetflixTopsheetRow[];
  subtotal: string;
  fringe_total: string;
  total: string;
}
export interface NetflixBudget {
  budget_name: string;
  base_currency: string;
  budget_version: string;
  episodes: number | null;
  min_per_episode: string;
  cost_per_episode: string;
  exec_producers: string;
  director: string;
  prepared_by: string;
  budget_date: string;
  shoot_weeks: string;
  post_weeks: string;
  fx_note: string;
  sections: NetflixSection[];
  atl_total: string;
  btl_total: string;
  ab_total: string;
  grand_total: string;
  error_count: number;
}

export interface NetflixCostRow {
  number: string;
  name: string;
  group_key: string;
  actuals_period: string;
  actuals_to_date: string;
  commitments: string;
  total_costs: string;
  etc: string;
  efc: string;
  budget: string;
  variance: string;
  over: boolean;
}
export interface NetflixCostReport {
  budget_name: string;
  base_currency: string;
  period_no: string;
  period_start: string;
  period_end: string;
  episodes: number | null;
  total_production: string;
  cost_per_episode: string;
  signed_agreement: string;
  group_rows: NetflixCostRow[];
  account_rows: NetflixCostRow[];
  grand: NetflixCostRow;
}

export interface NetflixCashInput {
  project_start?: string;
  weeks?: number | null;
  level?: string;
}
export interface NetflixWeek {
  index: number;
  ending_date: string;
}
export interface NetflixCashRow {
  number: string;
  name: string;
  payments_ytd: string;
  weekly: string[];
}
export interface NetflixCashFlow {
  budget_name: string;
  base_currency: string;
  level: string;
  project_start: string;
  weeks: NetflixWeek[];
  rows: NetflixCashRow[];
  week_totals: string[];
  ytd_total: string;
  undated: string;
}

export interface NetflixTrialInput {
  bank_balance?: string;
  show_name?: string;
  season?: string;
  date?: string;
  period_ending?: string;
}
export interface TrialBalanceRow {
  kind: string;
  name: string;
  amount: string;
  note: string;
  computed: boolean;
}
export interface NetflixTrialBalance {
  budget_name: string;
  show_name: string;
  season: string;
  date: string;
  period_ending: string;
  base_currency: string;
  rows: TrialBalanceRow[];
  total: string;
}

// --- Ulusal Dizi Formatı (national dizi sheet), mirror view::NationalSheetDto ---

export interface NationalRow {
  kind: "category" | "line" | "subtotal" | "section" | "grand";
  label: string;
  name: string | null;
  atl_btl: string | null;
  adet: string;
  vergi_orani: string | null;
  kom_orani: string | null;
  birim_tutar: string | null;
  net_tutar: string;
  stopaj: string;
  ek_komisyon: string;
  g_toplam: string;
}

export interface NationalSheet {
  budget_name: string;
  rows: NationalRow[];
  atl_total: string;
  btl_total: string;
  net_grand: string;
  stopaj_grand: string;
  komisyon_grand: string;
  grand_total: string;
}

// --- Live rates (TCMB FX + İstanbul fuel), mirror importers::rates ---

export interface LiveRates {
  date: string | null;
  usd: string | null;
  eur: string | null;
  benzin: string | null;
  motorin: string | null;
}
