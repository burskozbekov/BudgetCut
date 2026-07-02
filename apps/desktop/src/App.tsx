import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useApp } from "./store";
import Topsheet from "./components/Topsheet";
import NationalSheetPanel from "./components/NationalSheetPanel";
import NetflixBudget from "./components/NetflixBudget";
import NetflixCostReport from "./components/NetflixCostReport";
import NetflixCashFlow from "./components/NetflixCashFlow";
import NetflixTrialBalance from "./components/NetflixTrialBalance";
import AccountDetails from "./components/AccountDetails";
import SetupTools from "./components/SetupTools";
import ReportBuilder from "./components/ReportBuilder";
import SeriesPanel from "./components/SeriesPanel";
import Comparison from "./components/Comparison";
import IncentivePanel from "./components/IncentivePanel";
import LibraryPanel from "./components/LibraryPanel";
import ActualsPanel from "./components/ActualsPanel";
import SettlementPanel from "./components/SettlementPanel";
import SchedulePanel from "./components/SchedulePanel";
import POPanel from "./components/POPanel";
import TopActions from "./components/TopActions";
import TopRates from "./components/TopRates";
import Logo from "./components/Logo";
import CommandPalette from "./components/CommandPalette";
import Login from "./components/Login";
import Dashboard from "./components/Dashboard";

function initials(id: string): string {
  const hex = id.replace(/[^a-z0-9]/gi, "");
  return (hex.slice(-2) || "··").toUpperCase();
}

export default function App() {
  const { t } = useTranslation();
  const s = useApp();
  const { mode, view, setView, loading, topsheet, shootDays, setShootDays, setPalette, online, presence, leaveBudget } = s;
  const nflxActive = view.startsWith("nflx_");
  const [nflxOpen, setNflxOpen] = useState(false);
  const showNflx = nflxOpen || nflxActive;

  useEffect(() => {
    s.init();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (mode === "boot") return <div className="empty" style={{ marginTop: "20vh" }}>…</div>;
  if (mode === "login") return <Login />;
  if (mode === "dashboard") return <Dashboard />;

  const NavButton = ({ id, label }: { id: typeof view; label: string }) => (
    <button className={view === id ? "active" : ""} onClick={() => setView(id)}>
      {label}
    </button>
  );
  const heads: Record<string, { title: string; sub: string }> = {
    topsheet: { title: t("topsheet"), sub: t("topsheet_sub") },
    national: { title: t("nat_title"), sub: t("nat_sub") },
    nflx_budget: { title: t("nflx_budget"), sub: t("nflx_budget_sub") },
    nflx_cost: { title: t("nflx_cost"), sub: t("nflx_cost_sub") },
    nflx_cash: { title: t("nflx_cash"), sub: t("nflx_cash_sub") },
    nflx_trial: { title: t("nflx_trial"), sub: t("nflx_trial_sub") },
    details: { title: t("details"), sub: t("details_sub") },
    tools: { title: t("tools"), sub: t("tools_sub") },
    report: { title: t("nav_report"), sub: t("report_sub") },
    series: { title: t("nav_series"), sub: t("series_sub") },
    compare: { title: t("nav_compare"), sub: t("compare_sub") },
    incentive: { title: t("nav_incentive"), sub: t("incentive_sub") },
    library: { title: t("nav_library"), sub: t("library_sub") },
    actuals: { title: t("nav_actuals"), sub: t("actuals_sub") },
    settlement: { title: t("nav_settlement"), sub: t("settlement_sub") },
    schedule: { title: t("nav_schedule"), sub: t("schedule_sub") },
    po: { title: t("nav_po"), sub: t("po_sub") },
  };

  return (
    <div className="app">
      <div className="brand">
        <div className="brand-name">
          <Logo />
          {t("app")}
        </div>
        <TopActions />
      </div>

      <div className="topbar">
        <div className="title">
          {online && (
            <button className="back" onClick={leaveBudget} title={t("back_to_budgets")}>←</button>
          )}
          {topsheet?.budget_name ?? "…"}
        </div>
        <div className="meta">
          <TopRates />
          <div className="avatars" title="Presence">
            <span className="ava you">{online && s.userId ? initials(s.userId) : "SB"}</span>
            {online
              ? presence.map((p) => (
                  <span key={p.user} className="ava a2" title={p.user}>{initials(p.user)}</span>
                ))
              : null}
          </div>
          <button className="kbtn" onClick={() => setPalette(true)} title="⌘K">
            <span>⌘K</span> {t("palette_hint")}
          </button>
          <span className={`pill ${online ? "online" : ""}`}>
            <span className="dot" />
            {online ? t("connected") : t("offline")}
          </span>
        </div>
      </div>

      <nav className="nav">
        <div className="section">{t("nav_views")}</div>
        <NavButton id="topsheet" label={t("nav_topsheet")} />
        <NavButton id="national" label={t("nav_national")} />

        <button
          className={`nav-parent ${nflxActive ? "active" : ""}`}
          onClick={() => setNflxOpen((o) => !o)}
        >
          <span className={`caret ${showNflx ? "open" : ""}`}>▸</span>
          {t("nav_netflix")}
        </button>
        {showNflx && (
          <div className="nav-sub">
            <NavButton id="nflx_budget" label={t("nav_nflx_budget")} />
            <NavButton id="nflx_cost" label={t("nav_nflx_cost")} />
            <NavButton id="nflx_cash" label={t("nav_nflx_cash")} />
            <NavButton id="nflx_trial" label={t("nav_nflx_trial")} />
          </div>
        )}

        <NavButton id="details" label={t("nav_details")} />
        <NavButton id="tools" label={t("nav_tools")} />
        <NavButton id="report" label={t("nav_report")} />
        <NavButton id="series" label={t("nav_series")} />
        <NavButton id="compare" label={t("nav_compare")} />
        <NavButton id="incentive" label={t("nav_incentive")} />
        <NavButton id="library" label={t("nav_library")} />
        <NavButton id="actuals" label={t("nav_actuals")} />
        <NavButton id="settlement" label={t("nav_settlement")} />
        <NavButton id="schedule" label={t("nav_schedule")} />
        <NavButton id="po" label={t("nav_po")} />
      </nav>

      <main className="main">
        <div className="view-head">
          <div>
            <h1>{heads[view].title}</h1>
            <div className="sub">{heads[view].sub}</div>
          </div>
        </div>

        {(view === "topsheet" || view === "details") && (
          <div className="controls">
            <label htmlFor="sd">{t("shoot_days")}</label>
            <input
              id="sd"
              type="number"
              min={1}
              value={shootDays}
              onChange={(e) => setShootDays(Number(e.target.value))}
            />
            <span className="hint">{t("shoot_days_hint_native")}</span>
          </div>
        )}

        {loading ? (
          <div className="empty">…</div>
        ) : view === "topsheet" ? (
          <Topsheet />
        ) : view === "national" ? (
          <NationalSheetPanel />
        ) : view === "nflx_budget" ? (
          <NetflixBudget />
        ) : view === "nflx_cost" ? (
          <NetflixCostReport />
        ) : view === "nflx_cash" ? (
          <NetflixCashFlow />
        ) : view === "nflx_trial" ? (
          <NetflixTrialBalance />
        ) : view === "details" ? (
          <AccountDetails />
        ) : view === "tools" ? (
          <SetupTools />
        ) : view === "report" ? (
          <ReportBuilder />
        ) : view === "series" ? (
          <SeriesPanel />
        ) : view === "compare" ? (
          <Comparison />
        ) : view === "incentive" ? (
          <IncentivePanel />
        ) : view === "library" ? (
          <LibraryPanel />
        ) : view === "actuals" ? (
          <ActualsPanel />
        ) : view === "settlement" ? (
          <SettlementPanel />
        ) : view === "schedule" ? (
          <SchedulePanel />
        ) : (
          <POPanel />
        )}
      </main>

      <CommandPalette />
    </div>
  );
}
