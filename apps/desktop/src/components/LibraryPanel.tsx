import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useApp } from "../store";
import type { LibraryItem } from "../types";

/** Reusable setup library (online-only: shared fringe/global presets stored on
 *  the org) plus accounting CSV export (works offline too — pure topsheet data). */
export default function LibraryPanel() {
  const { t } = useTranslation();
  const online = useApp((s) => s.online);
  const { getAccountingCsv, loadLibraries, saveLibrary, applyLibrary } = useApp();
  const [libs, setLibs] = useState<LibraryItem[]>([]);
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (online) loadLibraries().then(setLibs);
  }, [online, loadLibraries]);

  const downloadCsv = async () => {
    const csv = await getAccountingCsv();
    if (!csv) return;
    const blob = new Blob([csv], { type: "text/csv;charset=utf-8" });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = "BudgetCut-Muhasebe.csv";
    a.click();
  };

  const save = async () => {
    if (!name.trim()) return;
    setBusy(true);
    await saveLibrary(name.trim());
    setName("");
    setLibs(await loadLibraries());
    setBusy(false);
  };

  return (
    <div className="an-panel">
      <h2>{t("lib_accounting")}</h2>
      <div className="an-form">
        <button className="auth-go" onClick={downloadCsv}>{t("lib_accounting_dl")}</button>
      </div>

      <hr />

      {!online ? (
        <div className="online-note">{t("online_only")}</div>
      ) : (
        <>
          <div className="an-form">
            <div className="an-field">
              <label>{t("lib_save_name")}</label>
              <input type="text" value={name} onChange={(e) => setName(e.target.value)} />
            </div>
            <button className="auth-go" disabled={busy} onClick={save}>{t("lib_save")}</button>
          </div>
          <div className="lib-list">
            {libs.length === 0 ? (
              <div className="empty">{t("lib_empty")}</div>
            ) : (
              libs.map((lib) => (
                <div key={lib.id} className="lib-item">
                  <div>
                    {lib.name}
                    <div className="l-meta">{lib.fringes} · {lib.globals}</div>
                  </div>
                  <button className="auth-toggle" onClick={() => applyLibrary(lib.id)}>{t("lib_apply")}</button>
                </div>
              ))
            )}
          </div>
        </>
      )}
    </div>
  );
}
