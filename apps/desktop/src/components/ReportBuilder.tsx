import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useApp } from "../store";
import { money } from "../fmt";

type ColKey = "number" | "name" | "atl_btl" | "subtotal" | "fringe_total" | "total";

/** Custom report builder (MMB "build custom reports": pick columns, exclude
 *  categories, live preview, export). Pure frontend over the topsheet data. */
export default function ReportBuilder() {
  const { t } = useTranslation();
  const top = useApp((s) => s.topsheet);
  const [cols, setCols] = useState<Record<ColKey, boolean>>({
    number: true,
    name: true,
    atl_btl: true,
    subtotal: true,
    fringe_total: false,
    total: true,
  });
  const [excluded, setExcluded] = useState<Set<string>>(new Set());

  const allCols: { key: ColKey; label: string; num?: boolean }[] = [
    { key: "number", label: t("col_code") },
    { key: "name", label: t("col_account") },
    { key: "atl_btl", label: "ATL/BTL" },
    { key: "subtotal", label: t("col_net"), num: true },
    { key: "fringe_total", label: t("col_reflect"), num: true },
    { key: "total", label: t("col_total"), num: true },
  ];

  const rows = useMemo(
    () => (top?.categories ?? []).filter((c) => !excluded.has(c.number) && Number(c.total) !== 0),
    [top, excluded]
  );
  const totals = useMemo(() => {
    const sum = (f: "subtotal" | "fringe_total" | "total") =>
      rows.reduce((a, c) => a + Number(c[f]), 0).toFixed(2);
    return { subtotal: sum("subtotal"), fringe_total: sum("fringe_total"), total: sum("total") };
  }, [rows]);

  const enabled = allCols.filter((c) => cols[c.key]);

  const exportCsv = () => {
    const header = enabled.map((c) => c.label).join(",");
    const body = rows
      .map((c) => enabled.map((col) => `"${String((c as any)[col.key] ?? "")}"`).join(","))
      .join("\n");
    const totalRow = enabled
      .map((col) => (col.key === "name" ? '"TOPLAM"' : ["subtotal", "fringe_total", "total"].includes(col.key) ? (totals as any)[col.key] : ""))
      .join(",");
    const csv = `${header}\n${body}\n${totalRow}\n`;
    const blob = new Blob([csv], { type: "text/csv;charset=utf-8" });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = "BudgetCut-Rapor.csv";
    a.click();
  };

  if (!top) return null;

  return (
    <div className="report-builder">
      <div className="rb-controls">
        <div className="rb-group">
          <div className="rb-h">{t("rb_columns")}</div>
          <div className="rb-chips">
            {allCols.map((c) => (
              <label key={c.key} className={`rb-chip ${cols[c.key] ? "on" : ""}`}>
                <input
                  type="checkbox"
                  checked={cols[c.key]}
                  onChange={(e) => setCols({ ...cols, [c.key]: e.target.checked })}
                />
                {c.label}
              </label>
            ))}
          </div>
        </div>
        <div className="rb-group">
          <div className="rb-h">{t("rb_exclude")}</div>
          <div className="rb-chips">
            {(top.categories ?? [])
              .filter((c) => Number(c.total) !== 0)
              .map((c) => (
                <label key={c.id} className={`rb-chip ${excluded.has(c.number) ? "off" : ""}`}>
                  <input
                    type="checkbox"
                    checked={!excluded.has(c.number)}
                    onChange={(e) => {
                      const next = new Set(excluded);
                      if (e.target.checked) next.delete(c.number);
                      else next.add(c.number);
                      setExcluded(next);
                    }}
                  />
                  {c.number} {c.name}
                </label>
              ))}
          </div>
        </div>
        <button className="auth-go rb-export" onClick={exportCsv}>{t("rb_export")}</button>
      </div>

      <table>
        <thead>
          <tr>{enabled.map((c) => <th key={c.key} className={c.num ? "num" : ""}>{c.label}</th>)}</tr>
        </thead>
        <tbody>
          {rows.map((c) => (
            <tr key={c.id}>
              {enabled.map((col) => {
                if (col.key === "atl_btl") return <td key={col.key}>{c.atl_btl && <span className={`tag ${c.atl_btl}`}>{c.atl_btl}</span>}</td>;
                if (col.num) return <td key={col.key} className="num">{money(String((c as any)[col.key]))}</td>;
                return <td key={col.key}>{col.key === "number" ? <span className="code">{c.number}</span> : (c as any)[col.key]}</td>;
              })}
            </tr>
          ))}
          <tr className="tot">
            {enabled.map((col) => {
              if (col.key === "name") return <td key={col.key}><b>{t("col_total")}</b></td>;
              if (["subtotal", "fringe_total", "total"].includes(col.key)) return <td key={col.key} className="num"><b>{money((totals as any)[col.key])}</b></td>;
              return <td key={col.key}></td>;
            })}
          </tr>
        </tbody>
      </table>
    </div>
  );
}
