import { useTranslation } from "react-i18next";
import { useApp } from "../store";
import { money, isZero } from "../fmt";

export default function Topsheet() {
  const { t } = useTranslation();
  const top = useApp((s) => s.topsheet);
  if (!top) return null;

  return (
    <>
      <table>
        <thead>
          <tr>
            <th>{t("col_account")}</th>
            <th className="num">{t("col_reflect")}</th>
            <th className="num">{t("col_total")}</th>
          </tr>
        </thead>
        <tbody>
          {top.categories.map((c) => (
            <tr key={c.id} className={isZero(c.total) ? "zero" : ""}>
              <td>
                <span className="code">{c.number}</span>
                {c.name}
                {c.atl_btl && <span className={`tag ${c.atl_btl}`} style={{ marginLeft: 8 }}>{c.atl_btl}</span>}
              </td>
              <td className="num">{money(c.fringe_total)}</td>
              <td className="num">{money(c.total)}</td>
            </tr>
          ))}
        </tbody>
      </table>

      <div className="totals">
        <div className="card">
          <div className="k">{t("atl")}</div>
          <div className="v">{money(top.atl_total)}</div>
        </div>
        <div className="card">
          <div className="k">{t("btl")}</div>
          <div className="v">{money(top.btl_total)}</div>
        </div>
        <div className="card">
          <div className="k">{t("fringes")}</div>
          <div className="v">{money(top.fringes_total)}</div>
        </div>
        <div className="card net">
          <div className="k">{t("net_total")}</div>
          <div className="v">{money(top.net_total)}</div>
        </div>
      </div>
      {top.error_count > 0 && (
        <p className="err" style={{ marginTop: 14 }}>
          {t("errors", { n: top.error_count })}
        </p>
      )}
    </>
  );
}
