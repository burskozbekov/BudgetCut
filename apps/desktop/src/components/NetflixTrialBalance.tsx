import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useApp } from "../store";
import { money } from "../fmt";
import type { NetflixTrialBalance as NTrial } from "../types";

/** Netflix trial balance (Format 4): cash-position snapshot — bank balance
 *  (manual/external), unsettled petty-cash by category, and open commitments
 *  by vendor, with a TOTAL. Derived rows are flagged; bank balance is entered. */
export default function NetflixTrialBalance() {
  const { t } = useTranslation();
  const { loadNetflixTrial } = useApp();
  const [bank, setBank] = useState("0");
  const [data, setData] = useState<NTrial | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let live = true;
    loadNetflixTrial({ bank_balance: bank })
      .then((d) => live && setData(d))
      .finally(() => live && setLoading(false));
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [bank]);

  if (loading) return <div className="empty">…</div>;
  if (!data) return <div className="online-note">{t("nflx_unavailable")}</div>;

  const kindLabel = (k: string) => t(`nflx_tb_kind_${k.toLowerCase()}`);

  return (
    <div className="nflx-panel">
      <div className="nflx-controls no-print">
        <label>{t("nflx_tb_bank")}</label>
        <input
          type="number"
          value={bank}
          onChange={(e) => setBank(e.target.value)}
          placeholder="0"
        />
        <button className="ta-btn" onClick={() => window.print()}>{t("nat_print")}</button>
      </div>

      <div className="print-doc-inline">
        <h3 className="nflx-h">{t("nflx_tb_title")} — {data.show_name}</h3>
        <table className="nflx-table nflx-trial">
          <thead>
            <tr>
              <th className="nflx-l">{t("nflx_tb_kind")}</th>
              <th className="nflx-l">{t("nflx_tb_name")}</th>
              <th className="num">{t("nflx_tb_amount")}</th>
              <th className="nflx-l">{t("nflx_tb_note")}</th>
            </tr>
          </thead>
          <tbody>
            {data.rows.map((r, i) => (
              <tr key={i}>
                <td className="nflx-l">
                  {kindLabel(r.kind)}
                  {!r.computed && <span className="tag manual">{t("nflx_tb_manual")}</span>}
                </td>
                <td className="nflx-l">{r.name}</td>
                <td className="num">{money(r.amount)}</td>
                <td className="nflx-l muted">{r.note}</td>
              </tr>
            ))}
          </tbody>
          <tfoot>
            <tr className="nflx-grand">
              <td className="nflx-l" colSpan={2}>{t("nflx_tb_total")}</td>
              <td className="num">{money(data.total)}</td>
              <td></td>
            </tr>
          </tfoot>
        </table>
      </div>
    </div>
  );
}
