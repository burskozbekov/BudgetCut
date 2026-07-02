import { Fragment, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useApp } from "../store";
import { money } from "../fmt";
import type { NetflixBudget as NBudget } from "../types";

// Localized label for a Netflix reporting group key.
export function groupLabel(t: (k: string) => string, key: string): string {
  return t(`nflx_grp_${key.toLowerCase()}`);
}

/** Netflix budget topsheet (Format 1): the locked-budget ladder — categories
 *  grouped into Netflix sections (ATL / BTL-Prod / Post / Music / VFX / Other /
 *  Misc) with section subtotals, then Total ATL / BTL / A+B / Grand. */
export default function NetflixBudget() {
  const { t } = useTranslation();
  const { loadNetflixBudget } = useApp();
  const [episodes, setEpisodes] = useState(8);
  const [data, setData] = useState<NBudget | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let live = true;
    loadNetflixBudget({ episodes })
      .then((d) => live && setData(d))
      .finally(() => live && setLoading(false));
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [episodes]);

  if (loading) return <div className="empty">…</div>;
  if (!data) return <div className="online-note">{t("nflx_unavailable")}</div>;

  return (
    <div className="nflx-panel">
      <div className="nflx-controls no-print">
        <label>{t("nflx_episodes")}</label>
        <input
          type="number"
          min={1}
          value={episodes}
          onChange={(e) => setEpisodes(Math.max(1, Number(e.target.value) || 1))}
        />
        {data.cost_per_episode && (
          <span className="nflx-chip">
            {t("nflx_cost_per_ep")}: <b>{money(data.cost_per_episode)}</b>
          </span>
        )}
        <button className="ta-btn" onClick={() => window.print()}>{t("nat_print")}</button>
      </div>

      <div className="print-doc-inline">
        <table className="nflx-table">
          <thead>
            <tr>
              <th className="nflx-l">{t("nflx_acct")}</th>
              <th className="nflx-l">{t("nflx_cat_desc")}</th>
              <th className="num">{t("col_subtotal")}</th>
              <th className="num">{t("col_fringes")}</th>
              <th className="num">{t("col_total")}</th>
            </tr>
          </thead>
          <tbody>
            {data.sections.map((s) => (
              <Fragment key={s.group_key}>
                <tr className="nflx-section-head">
                  <td colSpan={5}>
                    {groupLabel(t, s.group_key)}
                    <span className={`tag ${s.atl_btl.toLowerCase()}`}>{s.atl_btl}</span>
                  </td>
                </tr>
                {s.rows.map((row) => (
                  <tr key={row.number}>
                    <td className="nflx-l"><span className="code">{row.number}</span></td>
                    <td className="nflx-l">{row.name}</td>
                    <td className="num">{money(row.subtotal)}</td>
                    <td className="num">{money(row.fringe_total)}</td>
                    <td className="num">{money(row.total)}</td>
                  </tr>
                ))}
                <tr className="nflx-subtotal">
                  <td className="nflx-l" colSpan={2}>{t("nflx_section_total")} — {groupLabel(t, s.group_key)}</td>
                  <td className="num">{money(s.subtotal)}</td>
                  <td className="num">{money(s.fringe_total)}</td>
                  <td className="num">{money(s.total)}</td>
                </tr>
              </Fragment>
            ))}
          </tbody>
          <tfoot>
            <tr className="nflx-ladder"><td className="nflx-l" colSpan={4}>{t("nflx_total_atl")}</td><td className="num">{money(data.atl_total)}</td></tr>
            <tr className="nflx-ladder"><td className="nflx-l" colSpan={4}>{t("nflx_total_btl")}</td><td className="num">{money(data.btl_total)}</td></tr>
            <tr className="nflx-ladder"><td className="nflx-l" colSpan={4}>{t("nflx_total_ab")}</td><td className="num">{money(data.ab_total)}</td></tr>
            <tr className="nflx-grand"><td className="nflx-l" colSpan={4}>{t("nflx_grand")}</td><td className="num">{money(data.grand_total)}</td></tr>
          </tfoot>
        </table>
      </div>
    </div>
  );
}
