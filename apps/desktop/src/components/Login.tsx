import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useApp } from "../store";
import { api } from "../api";
import Logo from "./Logo";

export default function Login() {
  const { t } = useTranslation();
  const { login, authError } = useApp();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [register, setRegister] = useState(false);
  const [busy, setBusy] = useState(false);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    await login(email, password, register);
    setBusy(false);
  };

  return (
    <div className="auth-wrap">
      <form className="auth-card" onSubmit={submit}>
        <div className="auth-brand">
          <Logo className="logo logo-lg" /> {t("app")}
        </div>
        <div className="auth-sub">{t("auth_sub")}</div>
        <label>{t("auth_email")}</label>
        <input type="email" value={email} required autoFocus onChange={(e) => setEmail(e.target.value)} />
        <label>{t("auth_password")}</label>
        <input type="password" value={password} required minLength={6} onChange={(e) => setPassword(e.target.value)} />
        {authError && <div className="auth-err">{authError}</div>}
        <button className="auth-go" disabled={busy} type="submit">
          {busy ? "…" : register ? t("auth_register") : t("auth_login")}
        </button>
        <button type="button" className="auth-toggle" onClick={() => setRegister(!register)}>
          {register ? t("auth_have_account") : t("auth_no_account")}
        </button>
        <div className="auth-server">{t("auth_server")}: {api.serverUrl}</div>
      </form>
    </div>
  );
}
