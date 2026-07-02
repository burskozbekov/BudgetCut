import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useApp } from "../store";
import { money } from "../fmt";
import type { NationalSheet, NationalRow } from "../types";

// Fraction ("0.17") → Turkish percent ("%17"), trimming trailing zeros.
function pct(v: string | null): string {
  if (v == null || v === "") return "";
  const n = Number(v);
  if (!Number.isFinite(n)) return v;
  const p = n * 100;
  const s = Number.isInteger(p) ? String(p) : p.toFixed(2).replace(/0+$/, "").replace(/\.$/, "");
  return `%${s}`;
}

// Money, but blank cells stay blank (header rows carry no amounts).
const m = (v: string) => (v === "" ? "" : money(v));

/** Ulusal Dizi Formatı (§ national dizi budget layout): a faithful rendering of
 *  the BOŞ BÜTÇE workbook — İSİM / ADET / VERGİ-STOPAJ / KOM. ORANI / BİRİM /
 *  NET / STOPAJ / EK-KOMİSYON / G.TOPLAM, with per-section TOPLAM lines, the
 *  ATL/BTL subtotals and the DİREKT MALİYET grand total. Works online + native. */
export default function NationalSheetPanel() {
  const { t } = useTranslation();
  const { loadNationalSheet } = useApp();
  const [sheet, setSheet] = useState<NationalSheet | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let alive = true;
    loadNationalSheet()
      .then((s) => alive && setSheet(s))
      .finally(() => alive && setLoading(false));
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (loading) return <div className="empty">…</div>;
  if (!sheet) return <div className="online-note">{t("nat_unavailable")}</div>;

  const rowClass = (r: NationalRow) =>
    r.kind === "category"
      ? "nat-cat"
      : r.kind === "subtotal"
        ? "nat-sub"
        : r.kind === "section"
          ? "nat-section"
          : r.kind === "grand"
            ? "nat-grand"
            : "nat-line";

  return (
    <div className="nat-panel">
      <div className="nat-toolbar no-print">
        <div className="nat-summary">
          <span><b>ATL</b> {money(sheet.atl_total)}</span>
          <span><b>BTL</b> {money(sheet.btl_total)}</span>
          <span className="nat-grand-chip"><b>{t("nat_grand")}</b> {money(sheet.grand_total)}</span>
        </div>
        <button className="ta-btn" onClick={() => window.print()}>{t("nat_print")}</button>
      </div>

      <div className="nat-doc">
        <div className="nat-doc-head">
          <h3>{sheet.budget_name}</h3>
          <span className="nat-doc-sub">{t("nat_title")}</span>
        </div>
        <table className="nat-table">
          <thead>
            <tr>
              <th className="nat-l">{t("nat_col_desc")}</th>
              <th className="num">{t("nat_col_adet")}</th>
              <th className="num">{t("nat_col_vergi")}</th>
              <th className="num">{t("nat_col_kom")}</th>
              <th className="num">{t("nat_col_birim")}</th>
              <th className="num">{t("nat_col_net")}</th>
              <th className="num">{t("nat_col_stopaj")}</th>
              <th className="num">{t("nat_col_ek")}</th>
              <th className="num">{t("nat_col_gtoplam")}</th>
            </tr>
          </thead>
          <tbody>
            {sheet.rows.map((r, i) => (
              <tr key={i} className={rowClass(r)}>
                <td className="nat-l">
                  {r.label}
                  {r.atl_btl && <span className={`tag ${r.atl_btl.toLowerCase()}`}>{r.atl_btl}</span>}
                  {r.name && <span className="nat-name">{r.name}</span>}
                </td>
                <td className="num">{r.adet}</td>
                <td className="num muted">{pct(r.vergi_orani)}</td>
                <td className="num muted">{pct(r.kom_orani)}</td>
                <td className="num">{r.birim_tutar ? money(r.birim_tutar) : ""}</td>
                <td className="num">{m(r.net_tutar)}</td>
                <td className="num">{m(r.stopaj)}</td>
                <td className="num">{m(r.ek_komisyon)}</td>
                <td className="num nat-g">{m(r.g_toplam)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
