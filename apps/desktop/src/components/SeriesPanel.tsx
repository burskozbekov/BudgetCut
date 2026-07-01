import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useApp } from "../store";
import { money } from "../fmt";
import type { AmortInputRow, SeriesSummary } from "../types";

// UI row carries a stable id so add/delete keeps input focus on the right row
// (the id is stripped before the rows go to the backend).
type Row = AmortInputRow & { _id: number };

/** Series amortization & pattern budgeting (MMB "series" tools). Works online
 *  and offline alike — runSeries routes to the server or local bridge. */
export default function SeriesPanel() {
  const { t } = useTranslation();
  const { runSeries } = useApp();
  const nextId = useRef(1);
  const newRow = (over: number): Row => ({ _id: nextId.current++, label: "", total: "", over_episodes: over });
  const [episodes, setEpisodes] = useState(8);
  const [rows, setRows] = useState<Row[]>([newRow(8)]);
  const [result, setResult] = useState<SeriesSummary | null>(null);

  // Episode counts feed u32 backend fields — floor + clamp so a pasted "3.7"
  // or "-2" can't reject the request.
  const posInt = (v: string) => Math.max(1, Math.floor(Number(v)) || 1);

  const compute = async () => {
    const amortized: AmortInputRow[] = rows
      .filter((r) => r.total.trim() !== "" && !Number.isNaN(Number(r.total)))
      .map((r) => ({ label: r.label, total: r.total, over_episodes: posInt(String(r.over_episodes)) }));
    try {
      setResult(await runSeries(episodes, amortized));
    } catch {
      setResult(null);
    }
  };

  const setRow = (id: number, patch: Partial<Row>) =>
    setRows(rows.map((r) => (r._id === id ? { ...r, ...patch } : r)));

  return (
    <div className="an-panel">
      <div className="an-form">
        <div className="an-field">
          <label>{t("series_episodes")}</label>
          <input
            type="number"
            min={1}
            step={1}
            value={episodes}
            onChange={(e) => setEpisodes(posInt(e.target.value))}
          />
        </div>
        <button className="auth-go" onClick={compute}>{t("series_calc")}</button>
      </div>

      <h2 className="tools-h">{t("series_amort")}</h2>
      {rows.map((r) => (
        <div key={r._id} className="an-amort-row">
          <input
            className="grow"
            type="text"
            placeholder={t("series_label")}
            value={r.label}
            onChange={(e) => setRow(r._id, { label: e.target.value })}
          />
          <input
            type="number"
            placeholder={t("series_total_amt")}
            value={r.total}
            onChange={(e) => setRow(r._id, { total: e.target.value })}
          />
          <input
            type="number"
            min={1}
            step={1}
            title={t("series_over")}
            value={r.over_episodes}
            onChange={(e) => setRow(r._id, { over_episodes: posInt(e.target.value) })}
          />
          <button className="del" onClick={() => setRows(rows.filter((x) => x._id !== r._id))}>×</button>
        </div>
      ))}
      <button className="auth-toggle" onClick={() => setRows([...rows, newRow(episodes)])}>
        {"+ " + t("series_add")}
      </button>

      {result && (
        <div className="kpi-grid">
          <div className="kpi">
            <div className="k-label">{t("series_pattern_ep")}</div>
            <div className="k-val">{money(result.pattern_episode)}</div>
          </div>
          <div className="kpi">
            <div className="k-label">{t("series_pattern_total")}</div>
            <div className="k-val">{money(result.pattern_total)}</div>
          </div>
          <div className="kpi">
            <div className="k-label">{t("series_amort_total")}</div>
            <div className="k-val">{money(result.amort_total)}</div>
          </div>
          <div className="kpi hero">
            <div className="k-label">{t("series_season_total")} ({result.episodes})</div>
            <div className="k-val">{money(result.series_total)}</div>
          </div>
          <div className="kpi">
            <div className="k-label">{t("series_per_ep")}</div>
            <div className="k-val">{money(result.per_episode_all_in)}</div>
          </div>
        </div>
      )}
    </div>
  );
}
