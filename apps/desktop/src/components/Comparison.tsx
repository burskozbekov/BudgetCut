import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useApp } from "../store";
import { money } from "../fmt";
import type { Comparison as ComparisonResult } from "../types";

/** Version/location comparison (MMB "compare budgets"): pick a second budget,
 *  diff its categories against the current one. Online-only; needs 2 budgets. */
export default function Comparison() {
  const { t } = useTranslation();
  const online = useApp((s) => s.online);
  const budgets = useApp((s) => s.budgets);
  const currentBudgetId = useApp((s) => s.currentBudgetId);
  const { runCompare } = useApp();

  const others = budgets.filter((b) => b.id !== currentBudgetId);
  const [otherId, setOtherId] = useState<string>(others[0]?.id ?? "");
  const [result, setResult] = useState<ComparisonResult | null>(null);

  const diffClass = (diff: string) =>
    "diff " + (Number(diff) > 0 ? "pos" : Number(diff) < 0 ? "neg" : "");

  const compute = async () => {
    if (!otherId) return;
    setResult(await runCompare(otherId));
  };

  if (!online) return <div className="online-note">{t("online_only")}</div>;
  if (others.length === 0) return <div className="online-note">{t("cmp_none")}</div>;

  return (
    <div className="an-panel">
      <div className="an-form">
        <div className="an-field">
          <label>{t("cmp_pick")}</label>
          <select value={otherId} onChange={(e) => setOtherId(e.target.value)}>
            {others.map((b) => (
              <option key={b.id} value={b.id}>{b.name}</option>
            ))}
          </select>
        </div>
        <button className="auth-go" onClick={compute}>{t("cmp_run")}</button>
      </div>

      {result && (
        <>
          <div className="an-hint">{result.a_name} ↔ {result.b_name}</div>
          <table>
            <thead>
              <tr>
                <th>{t("cmp_category")}</th>
                <th className="num">{result.a_name}</th>
                <th className="num">{result.b_name}</th>
                <th className="num">{t("cmp_diff")}</th>
              </tr>
            </thead>
            <tbody>
              {result.rows.map((row) => (
                <tr key={row.number}>
                  <td><span className="code">{row.number}</span> {row.name}</td>
                  <td className="num">{money(row.a_total)}</td>
                  <td className="num">{money(row.b_total)}</td>
                  <td className={diffClass(row.diff)}>{money(row.diff)}</td>
                </tr>
              ))}
              <tr className="tot">
                <td><b>{t("col_total")}</b></td>
                <td className="num">{money(result.a_total)}</td>
                <td className="num">{money(result.b_total)}</td>
                <td className={diffClass(result.diff)}>{money(result.diff)}</td>
              </tr>
            </tbody>
          </table>
        </>
      )}
    </div>
  );
}
