import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useApp } from "../store";
import { money } from "../fmt";
import type { IncentiveReport } from "../types";

/** Incentive / rebate estimation. Estimates jurisdiction rebates against the
 *  budget net total (or an overridden qualifying spend). Works online + offline. */
export default function IncentivePanel() {
  const { t } = useTranslation();
  const topsheet = useApp((s) => s.topsheet);
  const { runIncentives } = useApp();
  const [qualifying, setQualifying] = useState("");
  const [result, setResult] = useState<IncentiveReport | null>(null);

  const compute = async () => {
    const q = qualifying.trim();
    setResult(await runIncentives(q || undefined));
  };

  useEffect(() => {
    compute();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div className="an-panel">
      <div className="an-form">
        <div className="an-field">
          <label>{t("inc_qualifying")}</label>
          <input
            type="number"
            value={qualifying}
            placeholder={topsheet?.net_total ?? ""}
            onChange={(e) => setQualifying(e.target.value)}
          />
        </div>
        <button className="auth-go" onClick={compute}>{t("inc_calc")}</button>
      </div>
      <p className="an-hint">{t("inc_default")}</p>

      {result && (
        <>
          <p className="an-hint">{money(result.qualifying_spend)}</p>
          <table>
            <thead>
              <tr>
                <th>{t("inc_jurisdiction")}</th>
                <th className="num">{t("inc_rate")}</th>
                <th className="num">{t("inc_cap")}</th>
                <th className="num">{t("inc_estimate")}</th>
              </tr>
            </thead>
            <tbody>
              {result.lines.map((line) => (
                <tr key={line.jurisdiction}>
                  <td>{line.jurisdiction}</td>
                  <td className="num">{(Number(line.rate) * 100).toFixed(0)}%</td>
                  <td className="num">{line.cap ? money(line.cap) : "—"}</td>
                  <td className="num">{money(line.estimate)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
    </div>
  );
}
