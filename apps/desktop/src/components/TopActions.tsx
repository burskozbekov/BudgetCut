import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useApp } from "../store";
import { checkForUpdate, type UpdateInfo } from "../update";

/** Top-left actions: check GitHub for a newer release, and (offline) reload the
 *  real BOŞ BÜTÇE sample budget. */
export default function TopActions() {
  const { t } = useTranslation();
  const online = useApp((s) => s.online);
  const loadSample = useApp((s) => s.loadSample);
  const [upd, setUpd] = useState<UpdateInfo | null>(null);
  const [checking, setChecking] = useState(false);
  const [busy, setBusy] = useState(false);

  const check = async () => {
    setChecking(true);
    try {
      const info = await checkForUpdate();
      setUpd(info);
      if (info.status === "available" && info.url) window.open(info.url, "_blank");
    } finally {
      setChecking(false);
    }
  };

  const reseed = async () => {
    if (!window.confirm(t("load_sample_confirm"))) return;
    setBusy(true);
    try {
      await loadSample();
    } finally {
      setBusy(false);
    }
  };

  const label = () => {
    if (checking) return t("upd_checking");
    switch (upd?.status) {
      case "available":
        return `${t("upd_available", { v: upd.latest })}`;
      case "current":
        return t("upd_current");
      case "unconfigured":
        return t("upd_unconfigured");
      case "error":
        return t("upd_error");
      default:
        return t("upd_check");
    }
  };

  return (
    <div className="top-actions">
      <button className="ta-btn" disabled={checking} onClick={check} title={t("upd_check")}>
        ⟳ {label()}
      </button>
      {upd?.status === "available" && upd.url && (
        <a className="ta-link" href={upd.url} target="_blank" rel="noreferrer">
          {t("upd_download")}
        </a>
      )}
      {!online && (
        <button className="ta-btn" disabled={busy} onClick={reseed} title={t("load_sample")}>
          {t("load_sample")}
        </button>
      )}
    </div>
  );
}
