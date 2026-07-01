import { useTranslation } from "react-i18next";
import { useApp } from "../store";

export default function SetupTools() {
  const { t } = useTranslation();
  const tools = useApp((s) => s.tools);
  if (!tools) return null;

  return (
    <div>
      <h2 className="tools-h">{t("tools_fringes")}</h2>
      <table>
        <thead>
          <tr>
            <th>{t("tools_code")}</th>
            <th>{t("tools_name")}</th>
            <th>{t("tools_kind")}</th>
            <th>{t("tools_mode")}</th>
            <th className="num">{t("tools_rate")}</th>
            <th>{t("tools_level")}</th>
          </tr>
        </thead>
        <tbody>
          {tools.fringes.map((f) => (
            <tr key={f.code}>
              <td className="code">{f.code}</td>
              <td>{f.name}</td>
              <td className="muted">{f.kind}</td>
              <td>
                <span className={`mode ${f.mode.includes("gross") ? "gu" : "add"}`}>{f.mode}</span>
              </td>
              <td className="num">{Number(f.rate) ? `${(Number(f.rate) * 100).toFixed(0)}%` : "—"}</td>
              <td className="muted">{f.posting_level}</td>
            </tr>
          ))}
        </tbody>
      </table>

      <h2 className="tools-h">{t("tools_globals")}</h2>
      <table>
        <thead>
          <tr>
            <th>{t("tools_name")}</th>
            <th>{t("col_desc")}</th>
            <th className="num">{t("tools_value")}</th>
          </tr>
        </thead>
        <tbody>
          {tools.globals.map((g) => (
            <tr key={g.name}>
              <td className="code">{g.name}</td>
              <td className="muted">{g.description}</td>
              <td className="num">{g.value.is_expr ? `=${g.value.text}` : g.value.text}</td>
            </tr>
          ))}
        </tbody>
      </table>

      <h2 className="tools-h">{t("tools_units")}</h2>
      <table>
        <thead>
          <tr>
            <th>{t("tools_code")}</th>
            <th>{t("tools_name")}</th>
            <th className="num">{t("tools_factor")}</th>
          </tr>
        </thead>
        <tbody>
          {tools.units.map((u) => (
            <tr key={u.code}>
              <td className="code">{u.code}</td>
              <td>{u.name}</td>
              <td className="num">{u.factor}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <p className="muted" style={{ marginTop: 12 }}>{t("tools_note")}</p>
    </div>
  );
}
