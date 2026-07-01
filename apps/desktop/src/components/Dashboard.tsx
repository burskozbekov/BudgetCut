import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useApp } from "../store";

export default function Dashboard() {
  const { t } = useTranslation();
  const { budgets, createBudget, duplicateBudget, openBudget, loadBudgets, logout } = useApp();
  const [name, setName] = useState("Yeni Dizi — Bölüm 1");
  const [template, setTemplate] = useState("dizi");
  const [busy, setBusy] = useState(false);

  const create = async () => {
    if (!name.trim()) return;
    setBusy(true);
    await createBudget(name.trim(), template);
    setBusy(false);
  };

  return (
    <div className="dash">
      <div className="dash-head">
        <div className="brand">
          <span className="logo" /> {t("app")}
        </div>
        <button className="auth-toggle" onClick={logout}>{t("logout")}</button>
      </div>

      <div className="dash-body">
        <h1>{t("dash_title")}</h1>
        <div className="dash-create">
          <input value={name} onChange={(e) => setName(e.target.value)} placeholder={t("dash_name")} />
          <label className="seed">
            {t("dash_template")}
            <select value={template} onChange={(e) => setTemplate(e.target.value)}>
              <option value="dizi">{t("dash_tpl_dizi")}</option>
              <option value="netflix">{t("dash_tpl_netflix")}</option>
              <option value="">{t("dash_tpl_empty")}</option>
            </select>
          </label>
          <button className="auth-go" disabled={busy} onClick={create}>{t("dash_new")}</button>
          <button className="auth-toggle" onClick={loadBudgets}>{t("dash_refresh")}</button>
        </div>

        {budgets.length === 0 ? (
          <div className="empty">{t("dash_empty")}</div>
        ) : (
          <div className="budget-grid">
            {budgets.map((b) => (
              <div key={b.id} className="budget-card" onClick={() => openBudget(b.id, b.name)}>
                <div className="bc-name">{b.name}</div>
                <div className="bc-meta">
                  <span className={`role ${b.role}`}>{b.role}</span>
                </div>
                <div className="bc-actions">
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      duplicateBudget(b.id, `${b.name} (kopya)`);
                    }}
                  >
                    {t("dash_duplicate")}
                  </button>
                  <button className="open" onClick={(e) => { e.stopPropagation(); openBudget(b.id, b.name); }}>
                    {t("dash_open")} →
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
