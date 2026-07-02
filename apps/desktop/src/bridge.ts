// The single seam between the React UI and the Rust core. In the Tauri app it
// calls real commands (which run budgetcut-core); in a plain browser it falls
// back to a fixture that was itself produced by the engine, so the UI always
// renders genuine numbers and never does business math itself (§4/§11).

import type {
  Topsheet,
  Tree,
  Tools,
  SeriesSummary,
  IncentiveReport,
  AmortInputRow,
  ActualsReport,
  AddActualInput,
  SettlementReport,
  AddReceiptInput,
  Schedule,
  AddStripInput,
  PurchaseOrders,
  AddPoInput,
  LiveRates,
} from "./types";
import demo from "./fixtures/demo.json";

export const inTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

export const bridge = {
  inTauri,

  async getTopsheet(): Promise<Topsheet> {
    return inTauri ? invoke<Topsheet>("get_topsheet") : ((demo as any).topsheet as Topsheet);
  },

  async getTree(): Promise<Tree> {
    return inTauri ? invoke<Tree>("get_tree") : ((demo as any).tree as Tree);
  },

  async getTools(): Promise<Tools> {
    return inTauri ? invoke<Tools>("get_tools") : ((demo as any).tools as Tools);
  },

  /** Set a named global's constant value (native only). */
  async setGlobalByName(name: string, value: string): Promise<void> {
    if (inTauri) await invoke("set_global_by_name", { name, value });
  },

  /** Edit a detail field (native only). field ∈ description|name|amount|rate. */
  async setDetailField(detail: string, field: string, value: string): Promise<void> {
    if (inTauri) await invoke("set_detail_field", { detail, field, value });
  },

  /** Append a blank line to an account (native only); returns its id. */
  async addLine(account: string): Promise<string> {
    return inTauri ? invoke<string>("add_line", { account }) : "";
  },

  /** Delete a line (native only). */
  async removeLine(detail: string): Promise<void> {
    if (inTauri) await invoke("remove_line", { detail });
  },

  /** Replace a line's applied fringes (native only). */
  async setDetailFringes(detail: string, fringes: { code: string; rate?: string }[]): Promise<void> {
    if (inTauri) await invoke("set_detail_fringes", { detail, fringes });
  },

  // --- MMB-parity analytics (offline): same core math as the server ---

  /** Amort & pattern series summary (native only). */
  async series(episodes: number, amortized: AmortInputRow[]): Promise<SeriesSummary | null> {
    return inTauri ? invoke<SeriesSummary>("series_summary", { episodes, amortized }) : null;
  },

  /** Incentive estimates; qualifying defaults to net total (native only). */
  async incentives(qualifying?: string): Promise<IncentiveReport | null> {
    return inTauri ? invoke<IncentiveReport>("incentive_report", { qualifying: qualifying ?? null }) : null;
  },

  /** Accounting GL export as CSV text (native only). */
  async accountingCsv(): Promise<string> {
    return inTauri ? invoke<string>("accounting_csv") : "";
  },

  /** Estimate-vs-actual / EFC report (native only). */
  async getActuals(): Promise<ActualsReport | null> {
    return inTauri ? invoke<ActualsReport>("get_actuals") : null;
  },

  /** Record an actual (native only); returns its id. */
  async addActual(arg: AddActualInput): Promise<string> {
    return inTauri ? invoke<string>("add_actual", { arg }) : "";
  },

  /** Delete a recorded actual (native only). */
  async removeActual(actual: string): Promise<void> {
    if (inTauri) await invoke("remove_actual", { actual });
  },

  /** Expense settlement reconciled against an advance (native only). */
  async getSettlement(advance?: string): Promise<SettlementReport | null> {
    return inTauri ? invoke<SettlementReport>("get_settlement", { advance: advance ?? null }) : null;
  },

  /** Record a settlement receipt (native only); returns its id. */
  async addReceipt(arg: AddReceiptInput): Promise<string> {
    return inTauri ? invoke<string>("add_receipt", { arg }) : "";
  },

  /** Delete a settlement receipt (native only). */
  async removeReceipt(receipt: string): Promise<void> {
    if (inTauri) await invoke("remove_receipt", { receipt });
  },

  /** Stripboard + Day-Out-of-Days (native only). */
  async getSchedule(): Promise<Schedule | null> {
    return inTauri ? invoke<Schedule>("get_schedule") : null;
  },

  /** Add a stripboard strip (native only); returns its id. */
  async addStrip(arg: AddStripInput): Promise<string> {
    return inTauri ? invoke<string>("add_strip", { arg }) : "";
  },

  /** Delete a stripboard strip (native only). */
  async removeStrip(strip: string): Promise<void> {
    if (inTauri) await invoke("remove_strip", { strip });
  },

  /** Purchase orders + committed totals (native only). */
  async getPurchaseOrders(): Promise<PurchaseOrders | null> {
    return inTauri ? invoke<PurchaseOrders>("get_purchase_orders") : null;
  },

  /** Create a Draft PO (native only); returns its id. */
  async addPo(arg: AddPoInput): Promise<string> {
    return inTauri ? invoke<string>("add_po", { arg }) : "";
  },

  /** Approve a PO (native only). */
  async approvePo(po: string): Promise<void> {
    if (inTauri) await invoke("approve_po", { po });
  },

  /** Convert a PO to an actual (native only). */
  async convertPo(po: string): Promise<void> {
    if (inTauri) await invoke("convert_po", { po });
  },

  /** Delete a PO (native only). */
  async removePo(po: string): Promise<void> {
    if (inTauri) await invoke("remove_po", { po });
  },

  /** Replace the local budget with the real BOŞ BÜTÇE sample (native only). */
  async loadSample(): Promise<void> {
    if (inTauri) await invoke("load_sample");
  },

  /** TCMB USD/EUR + İstanbul pump prices, fetched natively (native only). */
  async liveRates(): Promise<LiveRates | null> {
    return inTauri ? invoke<LiveRates>("live_rates") : null;
  },
};
