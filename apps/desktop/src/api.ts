// Online (server) data layer — used in the browser/collaboration mode where
// there is no local Rust engine. The server validates RBAC, applies ops through
// budgetcut-core, computes the views, and broadcasts changes over WebSocket.
// (The packaged desktop app uses the offline local bridge instead.)

import type {
  Topsheet,
  Tree,
  Tools,
  NationalSheet,
  SeriesSummary,
  IncentiveReport,
  Comparison,
  LibraryItem,
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
  NetflixHeaderInput,
  NetflixBudget,
  NetflixCostReport,
  NetflixCashInput,
  NetflixCashFlow,
  NetflixTrialInput,
  NetflixTrialBalance,
} from "./types";

// Build a query string from a params object, skipping empty/undefined values.
function qs(obj: object): string {
  const p = new URLSearchParams();
  for (const [k, v] of Object.entries(obj)) {
    if (v !== undefined && v !== null && v !== "") p.set(k, String(v));
  }
  const s = p.toString();
  return s ? `?${s}` : "";
}

const SERVER =
  (import.meta as any).env?.VITE_SERVER_URL ?? "http://127.0.0.1:8787";

export interface BudgetListItem {
  id: string;
  name: string;
  role: string;
}
export interface Session {
  token: string;
  refresh_token: string;
  user_id: string;
}

async function req<T>(path: string, opts: { method?: string; token?: string; body?: unknown } = {}): Promise<T> {
  const res = await fetch(SERVER + path, {
    method: opts.method ?? "GET",
    headers: {
      ...(opts.body ? { "content-type": "application/json" } : {}),
      ...(opts.token ? { authorization: `Bearer ${opts.token}` } : {}),
    },
    body: opts.body ? JSON.stringify(opts.body) : undefined,
  });
  if (!res.ok) {
    const text = await res.text();
    throw new Error(text || `${res.status} ${res.statusText}`);
  }
  const t = await res.text();
  return (t ? JSON.parse(t) : null) as T;
}

export const api = {
  serverUrl: SERVER,

  register: (email: string, password: string) =>
    req<Session>("/auth/register", { method: "POST", body: { email, password } }),
  login: (email: string, password: string) =>
    req<Session>("/auth/login", { method: "POST", body: { email, password } }),

  listBudgets: (token: string) => req<BudgetListItem[]>("/budgets", { token }),
  createBudget: (token: string, name: string, template: string) =>
    req<{ id: string }>("/budgets", { method: "POST", token, body: { name, template } }),
  duplicateBudget: (token: string, id: string, name: string) =>
    req<{ id: string }>(`/budgets/${id}/duplicate`, { method: "POST", token, body: { name } }),

  topsheet: (token: string, id: string) => req<Topsheet>(`/budgets/${id}/topsheet`, { token }),
  tree: (token: string, id: string) => req<Tree>(`/budgets/${id}/tree`, { token }),
  nationalSheet: (token: string, id: string) =>
    req<NationalSheet>(`/budgets/${id}/national-sheet`, { token }),

  netflixBudget: (token: string, id: string, h: NetflixHeaderInput) =>
    req<NetflixBudget>(`/budgets/${id}/netflix/budget${qs(h)}`, { token }),
  netflixCostReport: (token: string, id: string, h: NetflixHeaderInput) =>
    req<NetflixCostReport>(`/budgets/${id}/netflix/cost-report${qs(h)}`, { token }),
  netflixCashFlow: (token: string, id: string, i: NetflixCashInput) =>
    req<NetflixCashFlow>(`/budgets/${id}/netflix/cash-flow${qs(i)}`, { token }),
  netflixTrialBalance: (token: string, id: string, i: NetflixTrialInput) =>
    req<NetflixTrialBalance>(`/budgets/${id}/netflix/trial-balance${qs(i)}`, { token }),
  tools: (token: string, id: string) => req<Tools>(`/budgets/${id}/tools`, { token }),

  setGlobal: (token: string, id: string, name: string, value: string) =>
    req(`/budgets/${id}/global`, { method: "POST", token, body: { name, value } }),
  addLine: (token: string, id: string, account: string) =>
    req<{ id: string }>(`/budgets/${id}/lines`, { method: "POST", token, body: { account } }),
  setField: (token: string, id: string, detail: string, field: string, value: string) =>
    req(`/budgets/${id}/details/${detail}/field`, { method: "POST", token, body: { field, value } }),
  removeLine: (token: string, id: string, detail: string) =>
    req(`/budgets/${id}/details/${detail}/delete`, { method: "POST", token }),
  setFringes: (token: string, id: string, detail: string, fringes: { code: string; rate?: string }[]) =>
    req(`/budgets/${id}/details/${detail}/fringes`, { method: "POST", token, body: { fringes } }),

  // --- MMB-parity analytics ---
  series: (token: string, id: string, episodes: number, amortized: AmortInputRow[]) =>
    req<SeriesSummary>(`/budgets/${id}/series`, { method: "POST", token, body: { episodes, amortized } }),
  incentives: (token: string, id: string, qualifying?: string) =>
    req<IncentiveReport>(
      `/budgets/${id}/incentives${qualifying ? `?qualifying=${encodeURIComponent(qualifying)}` : ""}`,
      { token }
    ),
  compare: (token: string, a: string, b: string) =>
    req<Comparison>(`/compare?a=${encodeURIComponent(a)}&b=${encodeURIComponent(b)}`, { token }),
  listLibraries: (token: string) => req<LibraryItem[]>("/libraries", { token }),
  saveLibrary: (token: string, budget_id: string, name: string) =>
    req<{ id: string }>("/libraries", { method: "POST", token, body: { budget_id, name } }),
  applyLibrary: (token: string, id: string, lib: string) =>
    req<{ added_fringes: number; added_globals: number }>(
      `/budgets/${id}/libraries/${lib}/apply`,
      { method: "POST", token }
    ),
  actuals: (token: string, id: string) => req<ActualsReport>(`/budgets/${id}/actuals`, { token }),
  addActual: (token: string, id: string, body: AddActualInput) =>
    req<{ id: string }>(`/budgets/${id}/actuals`, { method: "POST", token, body }),
  removeActual: (token: string, id: string, actual: string) =>
    req(`/budgets/${id}/actuals/${actual}/delete`, { method: "POST", token }),

  settlement: (token: string, id: string, advance?: string) =>
    req<SettlementReport>(
      `/budgets/${id}/settlement${advance ? `?advance=${encodeURIComponent(advance)}` : ""}`,
      { token }
    ),
  addReceipt: (token: string, id: string, body: AddReceiptInput) =>
    req<{ id: string }>(`/budgets/${id}/receipts`, { method: "POST", token, body }),
  removeReceipt: (token: string, id: string, receipt: string) =>
    req(`/budgets/${id}/receipts/${receipt}/delete`, { method: "POST", token }),

  schedule: (token: string, id: string) => req<Schedule>(`/budgets/${id}/schedule`, { token }),
  addStrip: (token: string, id: string, body: AddStripInput) =>
    req<{ id: string }>(`/budgets/${id}/strips`, { method: "POST", token, body }),
  removeStrip: (token: string, id: string, strip: string) =>
    req(`/budgets/${id}/strips/${strip}/delete`, { method: "POST", token }),

  purchaseOrders: (token: string, id: string) =>
    req<PurchaseOrders>(`/budgets/${id}/purchase-orders`, { token }),
  addPo: (token: string, id: string, body: AddPoInput) =>
    req<{ id: string }>(`/budgets/${id}/purchase-orders`, { method: "POST", token, body }),
  approvePo: (token: string, id: string, po: string) =>
    req(`/budgets/${id}/purchase-orders/${po}/approve`, { method: "POST", token }),
  convertPo: (token: string, id: string, po: string) =>
    req(`/budgets/${id}/purchase-orders/${po}/convert`, { method: "POST", token }),
  removePo: (token: string, id: string, po: string) =>
    req(`/budgets/${id}/purchase-orders/${po}/delete`, { method: "POST", token }),

  /** Public: TCMB USD/EUR + İstanbul pump prices (server-proxied, no CORS). */
  rates: () => req<LiveRates>("/rates"),

  async accountingCsv(token: string, id: string): Promise<string> {
    const res = await fetch(SERVER + `/budgets/${id}/accounting.csv`, {
      headers: { authorization: `Bearer ${token}` },
    });
    if (!res.ok) throw new Error((await res.text()) || `${res.status}`);
    return res.text();
  },

  connectWs(id: string, token: string, onMessage: (msg: any) => void): WebSocket {
    const wsUrl = SERVER.replace(/^http/, "ws") + `/budgets/${id}/ws?token=${encodeURIComponent(token)}`;
    const ws = new WebSocket(wsUrl);
    ws.onmessage = (e) => {
      try {
        onMessage(JSON.parse(e.data));
      } catch {
        /* ignore */
      }
    };
    return ws;
  },
};
