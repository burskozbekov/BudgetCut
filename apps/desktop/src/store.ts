import { create } from "zustand";
import { bridge } from "./bridge";
import { api, type BudgetListItem } from "./api";
import type {
  Topsheet,
  Tree,
  Tools,
  NationalSheet,
  NetflixHeaderInput,
  NetflixBudget,
  NetflixCostReport,
  NetflixCashInput,
  NetflixCashFlow,
  NetflixTrialInput,
  NetflixTrialBalance,
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
} from "./types";

type View =
  | "topsheet"
  | "national"
  | "nflx_budget"
  | "nflx_cost"
  | "nflx_cash"
  | "nflx_trial"
  | "details"
  | "tools"
  | "report"
  | "series"
  | "compare"
  | "incentive"
  | "library"
  | "actuals"
  | "settlement"
  | "schedule"
  | "po";
type Mode = "boot" | "login" | "dashboard" | "budget";

// Online when not running inside the native (offline-local) Tauri shell.
const ONLINE = !bridge.inTauri;
const LS_TOKEN = "budgetcut.token";
const LS_USER = "budgetcut.user";

interface Presence {
  user: string;
  ts: number;
}

interface AppState {
  online: boolean;
  mode: Mode;
  token: string | null;
  userId: string | null;
  authError: string | null;
  budgets: BudgetListItem[];
  currentBudgetId: string | null;
  currentBudgetName: string;

  view: View;
  topsheet: Topsheet | null;
  tree: Tree | null;
  tools: Tools | null;
  shootDays: number;
  loading: boolean;
  paletteOpen: boolean;
  presence: Presence[];
  ws: WebSocket | null;

  init: () => Promise<void>;
  login: (email: string, password: string, register: boolean) => Promise<void>;
  logout: () => void;
  loadBudgets: () => Promise<void>;
  createBudget: (name: string, template: string) => Promise<void>;
  duplicateBudget: (id: string, name: string) => Promise<void>;
  openBudget: (id: string, name: string) => Promise<void>;
  leaveBudget: () => void;

  setView: (v: View) => void;
  setPalette: (open: boolean) => void;
  refresh: () => Promise<void>;
  setShootDays: (n: number) => Promise<void>;
  editDetail: (detail: string, field: string, value: string) => Promise<void>;
  editFringes: (detail: string, fringes: { code: string; rate?: string }[]) => Promise<void>;
  addLine: (account: string) => Promise<void>;
  removeLine: (detail: string) => Promise<void>;

  // MMB-parity analytics. Online uses the server; offline (native) uses the
  // local bridge where supported. Compare + library are online-only (they need
  // multiple budgets / org storage) and return null/[] offline.
  runSeries: (episodes: number, amortized: AmortInputRow[]) => Promise<SeriesSummary | null>;
  runIncentives: (qualifying?: string) => Promise<IncentiveReport | null>;
  getAccountingCsv: () => Promise<string>;
  runCompare: (otherId: string) => Promise<Comparison | null>;
  loadLibraries: () => Promise<LibraryItem[]>;
  saveLibrary: (name: string) => Promise<void>;
  applyLibrary: (libId: string) => Promise<void>;

  // Ulusal Dizi Formatı — the national dizi sheet layout.
  loadNationalSheet: () => Promise<NationalSheet | null>;

  // Netflix reporting suite. Works online (server) and offline (native).
  loadNetflixBudget: (h: NetflixHeaderInput) => Promise<NetflixBudget | null>;
  loadNetflixCost: (h: NetflixHeaderInput) => Promise<NetflixCostReport | null>;
  loadNetflixCash: (i: NetflixCashInput) => Promise<NetflixCashFlow | null>;
  loadNetflixTrial: (i: NetflixTrialInput) => Promise<NetflixTrialBalance | null>;

  // Actuals / EFC. Works online (server) and offline (native bridge).
  loadActuals: () => Promise<ActualsReport | null>;
  addActual: (input: AddActualInput) => Promise<void>;
  removeActual: (id: string) => Promise<void>;

  // Settlement / Hesap Kapama. Works online and offline.
  loadSettlement: (advance?: string) => Promise<SettlementReport | null>;
  addReceipt: (input: AddReceiptInput) => Promise<void>;
  removeReceipt: (id: string) => Promise<void>;

  // Scheduling / stripboard. Works online and offline.
  loadSchedule: () => Promise<Schedule | null>;
  addStrip: (input: AddStripInput) => Promise<void>;
  removeStrip: (id: string) => Promise<void>;

  // Replace the local budget with the real BOŞ BÜTÇE sample (offline/native).
  loadSample: () => Promise<void>;

  // Purchase orders / commitments. Works online and offline.
  loadPurchaseOrders: () => Promise<PurchaseOrders | null>;
  addPo: (input: AddPoInput) => Promise<void>;
  approvePo: (id: string) => Promise<void>;
  convertPo: (id: string) => Promise<void>;
  removePo: (id: string) => Promise<void>;
}

export const useApp = create<AppState>((set, get) => ({
  online: ONLINE,
  mode: "boot",
  token: null,
  userId: null,
  authError: null,
  budgets: [],
  currentBudgetId: null,
  currentBudgetName: "",

  view: "topsheet",
  topsheet: null,
  tree: null,
  tools: null,
  shootDays: 45,
  loading: true,
  paletteOpen: false,
  presence: [],
  ws: null,

  init: async () => {
    if (!ONLINE) {
      // Offline native app: open the local budget straight away.
      set({ mode: "budget" });
      await get().refresh();
      return;
    }
    const token = localStorage.getItem(LS_TOKEN);
    const userId = localStorage.getItem(LS_USER);
    if (token) {
      set({ token, userId });
      try {
        await get().loadBudgets();
        set({ mode: "dashboard" });
        return;
      } catch {
        localStorage.removeItem(LS_TOKEN);
      }
    }
    set({ mode: "login", loading: false });
  },

  login: async (email, password, register) => {
    set({ authError: null });
    try {
      const s = register ? await api.register(email, password) : await api.login(email, password);
      localStorage.setItem(LS_TOKEN, s.token);
      localStorage.setItem(LS_USER, s.user_id);
      set({ token: s.token, userId: s.user_id });
      await get().loadBudgets();
      set({ mode: "dashboard" });
    } catch (e: any) {
      set({ authError: String(e?.message ?? e) });
    }
  },

  logout: () => {
    localStorage.removeItem(LS_TOKEN);
    localStorage.removeItem(LS_USER);
    get().leaveBudget();
    set({ token: null, userId: null, mode: "login", budgets: [] });
  },

  loadBudgets: async () => {
    const token = get().token!;
    set({ budgets: await api.listBudgets(token) });
  },

  createBudget: async (name, template) => {
    await api.createBudget(get().token!, name, template);
    await get().loadBudgets();
  },

  duplicateBudget: async (id, name) => {
    await api.duplicateBudget(get().token!, id, name);
    await get().loadBudgets();
  },

  openBudget: async (id, name) => {
    set({ currentBudgetId: id, currentBudgetName: name, mode: "budget", presence: [] });
    await get().refresh();
    // Live channel: re-fetch on remote ops, track presence; heartbeat our own.
    const token = get().token!;
    const ws = api.connectWs(id, token, (msg) => {
      if (msg.type === "op") {
        get().refresh();
      } else if (msg.type === "presence") {
        const now = Date.now();
        const others = get().presence.filter((p) => p.user !== msg.user && now - p.ts < 12000);
        set({ presence: [...others, { user: msg.user, ts: now }] });
      }
    });
    ws.onopen = () => {
      const ping = () => ws.readyState === 1 && ws.send(JSON.stringify({ viewing: true }));
      ping();
      (ws as any)._hb = setInterval(ping, 5000);
    };
    set({ ws });
  },

  leaveBudget: () => {
    const ws = get().ws;
    if (ws) {
      clearInterval((ws as any)._hb);
      ws.close();
    }
    set({ ws: null, presence: [], currentBudgetId: null, mode: get().online ? "dashboard" : "budget" });
  },

  setView: (view) => set({ view }),
  setPalette: (paletteOpen) => set({ paletteOpen }),

  refresh: async () => {
    set({ loading: true });
    const { online, token, currentBudgetId } = get();
    if (online) {
      if (!token || !currentBudgetId) {
        set({ loading: false });
        return;
      }
      const [topsheet, tree, tools] = await Promise.all([
        api.topsheet(token, currentBudgetId),
        api.tree(token, currentBudgetId),
        api.tools(token, currentBudgetId),
      ]);
      set({ topsheet, tree, tools, loading: false });
    } else {
      const [topsheet, tree, tools] = await Promise.all([bridge.getTopsheet(), bridge.getTree(), bridge.getTools()]);
      set({ topsheet, tree, tools, loading: false });
    }
  },

  setShootDays: async (n) => {
    set({ shootDays: n });
    const { online, token, currentBudgetId } = get();
    if (online && token && currentBudgetId) await api.setGlobal(token, currentBudgetId, "CEKIM_GUN", String(n));
    else await bridge.setGlobalByName("CEKIM_GUN", String(n));
    await get().refresh();
  },

  editDetail: async (detail, field, value) => {
    const { online, token, currentBudgetId } = get();
    if (online && token && currentBudgetId) await api.setField(token, currentBudgetId, detail, field, value);
    else await bridge.setDetailField(detail, field, value);
    await get().refresh();
  },

  editFringes: async (detail, fringes) => {
    const { online, token, currentBudgetId } = get();
    if (online && token && currentBudgetId) await api.setFringes(token, currentBudgetId, detail, fringes);
    else await bridge.setDetailFringes(detail, fringes);
    await get().refresh();
  },

  addLine: async (account) => {
    const { online, token, currentBudgetId } = get();
    if (online && token && currentBudgetId) await api.addLine(token, currentBudgetId, account);
    else await bridge.addLine(account);
    await get().refresh();
  },

  removeLine: async (detail) => {
    const { online, token, currentBudgetId } = get();
    if (online && token && currentBudgetId) await api.removeLine(token, currentBudgetId, detail);
    else await bridge.removeLine(detail);
    await get().refresh();
  },

  runSeries: async (episodes, amortized) => {
    const { online, token, currentBudgetId } = get();
    if (online && token && currentBudgetId) return api.series(token, currentBudgetId, episodes, amortized);
    return bridge.series(episodes, amortized);
  },

  runIncentives: async (qualifying) => {
    const { online, token, currentBudgetId } = get();
    if (online && token && currentBudgetId) return api.incentives(token, currentBudgetId, qualifying);
    return bridge.incentives(qualifying);
  },

  getAccountingCsv: async () => {
    const { online, token, currentBudgetId } = get();
    if (online && token && currentBudgetId) return api.accountingCsv(token, currentBudgetId);
    return bridge.accountingCsv();
  },

  runCompare: async (otherId) => {
    const { online, token, currentBudgetId } = get();
    if (!online || !token || !currentBudgetId) return null; // online-only
    return api.compare(token, currentBudgetId, otherId);
  },

  loadLibraries: async () => {
    const { online, token } = get();
    if (!online || !token) return []; // online-only
    return api.listLibraries(token);
  },

  saveLibrary: async (name) => {
    const { online, token, currentBudgetId } = get();
    if (online && token && currentBudgetId) await api.saveLibrary(token, currentBudgetId, name);
  },

  applyLibrary: async (libId) => {
    const { online, token, currentBudgetId } = get();
    if (online && token && currentBudgetId) {
      await api.applyLibrary(token, currentBudgetId, libId);
      await get().refresh();
    }
  },

  loadNationalSheet: async () => {
    const { online, token, currentBudgetId } = get();
    if (online && token && currentBudgetId) return api.nationalSheet(token, currentBudgetId);
    return bridge.nationalSheet();
  },

  loadNetflixBudget: async (h) => {
    const { online, token, currentBudgetId } = get();
    if (online && token && currentBudgetId) return api.netflixBudget(token, currentBudgetId, h);
    return bridge.netflixBudget(h);
  },
  loadNetflixCost: async (h) => {
    const { online, token, currentBudgetId } = get();
    if (online && token && currentBudgetId) return api.netflixCostReport(token, currentBudgetId, h);
    return bridge.netflixCostReport(h);
  },
  loadNetflixCash: async (i) => {
    const { online, token, currentBudgetId } = get();
    if (online && token && currentBudgetId) return api.netflixCashFlow(token, currentBudgetId, i);
    return bridge.netflixCashFlow(i);
  },
  loadNetflixTrial: async (i) => {
    const { online, token, currentBudgetId } = get();
    if (online && token && currentBudgetId) return api.netflixTrialBalance(token, currentBudgetId, i);
    return bridge.netflixTrialBalance(i);
  },

  loadActuals: async () => {
    const { online, token, currentBudgetId } = get();
    if (online && token && currentBudgetId) return api.actuals(token, currentBudgetId);
    return bridge.getActuals();
  },

  addActual: async (input) => {
    const { online, token, currentBudgetId } = get();
    if (online && token && currentBudgetId) await api.addActual(token, currentBudgetId, input);
    else await bridge.addActual(input);
    await get().refresh(); // estimate side may be referenced elsewhere
  },

  removeActual: async (id) => {
    const { online, token, currentBudgetId } = get();
    if (online && token && currentBudgetId) await api.removeActual(token, currentBudgetId, id);
    else await bridge.removeActual(id);
    await get().refresh();
  },

  loadSettlement: async (advance) => {
    const { online, token, currentBudgetId } = get();
    if (online && token && currentBudgetId) return api.settlement(token, currentBudgetId, advance);
    return bridge.getSettlement(advance);
  },

  addReceipt: async (input) => {
    const { online, token, currentBudgetId } = get();
    if (online && token && currentBudgetId) await api.addReceipt(token, currentBudgetId, input);
    else await bridge.addReceipt(input);
  },

  removeReceipt: async (id) => {
    const { online, token, currentBudgetId } = get();
    if (online && token && currentBudgetId) await api.removeReceipt(token, currentBudgetId, id);
    else await bridge.removeReceipt(id);
  },

  loadSchedule: async () => {
    const { online, token, currentBudgetId } = get();
    if (online && token && currentBudgetId) return api.schedule(token, currentBudgetId);
    return bridge.getSchedule();
  },

  addStrip: async (input) => {
    const { online, token, currentBudgetId } = get();
    if (online && token && currentBudgetId) await api.addStrip(token, currentBudgetId, input);
    else await bridge.addStrip(input);
  },

  removeStrip: async (id) => {
    const { online, token, currentBudgetId } = get();
    if (online && token && currentBudgetId) await api.removeStrip(token, currentBudgetId, id);
    else await bridge.removeStrip(id);
  },

  loadSample: async () => {
    await bridge.loadSample();
    await get().refresh();
  },

  loadPurchaseOrders: async () => {
    const { online, token, currentBudgetId } = get();
    if (online && token && currentBudgetId) return api.purchaseOrders(token, currentBudgetId);
    return bridge.getPurchaseOrders();
  },

  addPo: async (input) => {
    const { online, token, currentBudgetId } = get();
    if (online && token && currentBudgetId) await api.addPo(token, currentBudgetId, input);
    else await bridge.addPo(input);
  },

  approvePo: async (id) => {
    const { online, token, currentBudgetId } = get();
    if (online && token && currentBudgetId) await api.approvePo(token, currentBudgetId, id);
    else await bridge.approvePo(id);
  },

  convertPo: async (id) => {
    const { online, token, currentBudgetId } = get();
    if (online && token && currentBudgetId) await api.convertPo(token, currentBudgetId, id);
    else await bridge.convertPo(id);
  },

  removePo: async (id) => {
    const { online, token, currentBudgetId } = get();
    if (online && token && currentBudgetId) await api.removePo(token, currentBudgetId, id);
    else await bridge.removePo(id);
  },
}));
