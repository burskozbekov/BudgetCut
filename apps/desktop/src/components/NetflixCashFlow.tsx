import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useApp } from "../store";
import { money } from "../fmt";
import type { NetflixCashFlow as NCash } from "../types";

/** Netflix weekly cash flow (Format 3): time-phased cash-out matrix. Rows =
 *  accounts (Header=category / Detail=account), columns = week-ending dates,
 *  cell = VAT-included cash paid that week; PAYMENTS YTD = row total. */
export default function NetflixCashFlow() {
  const { t } = useTranslation();
  const { loadNetflixCash } = useApp();
  const [level, setLevel] = useState<"header" | "detail">("header");
  const [start, setStart] = useState("");
  const [data, setData] = useState<NCash | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let live = true;
    loadNetflixCash({ level, project_start: start || undefined })
      .then((d) => {
        if (!live) return;
        setData(d);
        if (d && !start) setStart(d.project_start); // adopt the derived start
      })
      .finally(() => live && setLoading(false));
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [level, start]);

  if (loading) return <div className="empty">…</div>;
  if (!data) return <div className="online-note">{t("nflx_unavailable")}</div>;

  return (
    <div className="nflx-panel">
      <div className="nflx-controls no-print">
        <div className="seg">
          <button className={level === "header" ? "on" : ""} onClick={() => setLevel("header")}>{t("nflx_cf_header_lvl")}</button>
          <button className={level === "detail" ? "on" : ""} onClick={() => setLevel("detail")}>{t("nflx_cf_detail_lvl")}</button>
        </div>
        <label>{t("nflx_cf_start")}</label>
        <input type="date" value={start} onChange={(e) => setStart(e.target.value)} />
        <span className="nflx-chip">{t("nflx_cf_ytd")}: <b>{money(data.ytd_total)}</b></span>
        <button className="ta-btn" onClick={() => window.print()}>{t("nat_print")}</button>
      </div>

      {data.rows.length === 0 ? (
        <div className="empty">{t("nflx_cf_empty")}</div>
      ) : (
        <div className="nflx-scroll print-doc-inline">
          <table className="nflx-table nflx-cash">
            <thead>
              <tr>
                <th className="nflx-l nflx-sticky">{t("nflx_cf_account")}</th>
                <th className="num nflx-ytd">{t("nflx_cf_ytd")}</th>
                {data.weeks.map((w) => (
                  <th key={w.index} className="num">
                    <div className="wk">H{w.index}</div>
                    <div className="wk-date">{w.ending_date.slice(5)}</div>
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {data.rows.map((r) => (
                <tr key={r.number}>
                  <td className="nflx-l nflx-sticky"><span className="code">{r.number}</span> {r.name}</td>
                  <td className="num nflx-ytd">{money(r.payments_ytd)}</td>
                  {r.weekly.map((v, i) => (
                    <td key={i} className={`num ${Number(v) === 0 ? "muted" : ""}`}>{Number(v) === 0 ? "–" : money(v)}</td>
                  ))}
                </tr>
              ))}
            </tbody>
            <tfoot>
              <tr className="nflx-grand">
                <td className="nflx-l nflx-sticky">{t("col_total")}</td>
                <td className="num nflx-ytd">{money(data.ytd_total)}</td>
                {data.week_totals.map((v, i) => (
                  <td key={i} className="num">{Number(v) === 0 ? "–" : money(v)}</td>
                ))}
              </tr>
            </tfoot>
          </table>
        </div>
      )}
      {Number(data.undated) > 0 && (
        <p className="an-hint">{t("nflx_cf_undated")}: {money(data.undated)}</p>
      )}
    </div>
  );
}
