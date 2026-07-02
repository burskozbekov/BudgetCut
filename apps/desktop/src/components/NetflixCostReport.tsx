import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useApp } from "../store";
import { money } from "../fmt";
import type { NetflixCostReport as NCost, NetflixCostRow } from "../types";
import { groupLabel } from "./NetflixBudget";

// First day of the current month → today, as YYYY-MM-DD defaults.
function monthStart(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-01`;
}
function today(): string {
  return new Date().toISOString().slice(0, 10);
}

/** Netflix cost report (Format 2): Actuals this Period / to Date / Commitments /
 *  Total+Comm / ETC / EFC / Budget / Variance, at group-summary and account
 *  level. Budget = calc estimate; actuals = brut; commitments = Approved POs. */
export default function NetflixCostReport() {
  const { t } = useTranslation();
  const { loadNetflixCost } = useApp();
  const [from, setFrom] = useState(monthStart());
  const [to, setTo] = useState(today());
  const [episodes, setEpisodes] = useState(8);
  const [data, setData] = useState<NCost | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let live = true;
    loadNetflixCost({ period_start: from, period_end: to, episodes })
      .then((d) => live && setData(d))
      .finally(() => live && setLoading(false));
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [from, to, episodes]);

  if (loading) return <div className="empty">…</div>;
  if (!data) return <div className="online-note">{t("nflx_unavailable")}</div>;

  const vCls = (r: NetflixCostRow) => "num diff " + (r.over ? "neg" : "");
  const row = (r: NetflixCostRow, label: string, cls = "") => (
    <tr key={label + r.number} className={cls}>
      <td className="nflx-l">{r.number && <span className="code">{r.number}</span>} {label}</td>
      <td className="num">{money(r.actuals_period)}</td>
      <td className="num">{money(r.actuals_to_date)}</td>
      <td className="num">{money(r.commitments)}</td>
      <td className="num">{money(r.total_costs)}</td>
      <td className="num">{money(r.etc)}</td>
      <td className="num">{money(r.efc)}</td>
      <td className="num">{money(r.budget)}</td>
      <td className={vCls(r)}>{money(r.variance)}</td>
    </tr>
  );

  const head = (
    <tr>
      <th className="nflx-l">{t("nflx_cr_account")}</th>
      <th className="num">{t("nflx_cr_period")}</th>
      <th className="num">{t("nflx_cr_todate")}</th>
      <th className="num">{t("nflx_cr_commit")}</th>
      <th className="num">{t("nflx_cr_total")}</th>
      <th className="num">{t("nflx_cr_etc")}</th>
      <th className="num">{t("nflx_cr_efc")}</th>
      <th className="num">{t("nflx_cr_budget")}</th>
      <th className="num">{t("nflx_cr_variance")}</th>
    </tr>
  );

  return (
    <div className="nflx-panel">
      <div className="nflx-controls no-print">
        <label>{t("nflx_period")}</label>
        <input type="date" value={from} onChange={(e) => setFrom(e.target.value)} />
        <span>→</span>
        <input type="date" value={to} onChange={(e) => setTo(e.target.value)} />
        <label>{t("nflx_episodes")}</label>
        <input type="number" min={1} value={episodes} onChange={(e) => setEpisodes(Math.max(1, Number(e.target.value) || 1))} />
        <button className="ta-btn" onClick={() => window.print()}>{t("nat_print")}</button>
      </div>

      <div className="nflx-summary-bar">
        <span>{t("nflx_total_production")}: <b>{money(data.total_production)}</b></span>
        {data.cost_per_episode && <span>{t("nflx_cost_per_ep")}: <b>{money(data.cost_per_episode)}</b></span>}
      </div>

      <div className="print-doc-inline">
        <h3 className="nflx-h">{t("nflx_cr_summary")}</h3>
        <table className="nflx-table nflx-cost">
          <thead>{head}</thead>
          <tbody>{data.group_rows.map((r) => row(r, groupLabel(t, r.group_key), "nflx-subtotal"))}</tbody>
          <tfoot>{row(data.grand, t("col_total"), "nflx-grand")}</tfoot>
        </table>

        <h3 className="nflx-h">{t("nflx_cr_detail")}</h3>
        <table className="nflx-table nflx-cost">
          <thead>{head}</thead>
          <tbody>{data.account_rows.map((r) => row(r, r.name))}</tbody>
        </table>
      </div>
    </div>
  );
}
