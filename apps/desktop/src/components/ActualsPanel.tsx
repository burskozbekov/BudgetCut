import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useApp } from "../store";
import { bridge } from "../bridge";
import { money } from "../fmt";
import ReceiptScanPanel from "./ReceiptScanPanel";
import type { ActualsReport } from "../types";

// Turkish VAT-withholding (tevkifat) service types — the values must match the
// budgetcut-core `tevkifat_rate` table exactly (they're domain terms, not UI
// copy, so they stay Turkish in both languages).
const TEVKIFAT_KINDS = [
  "Yük Taşımacılığı",
  "Ticari Reklam",
  "Temizlik",
  "İşgücü Temini",
  "Danışmanlık",
  "Etüt-Proje",
  "Yapım İşleri",
];

/** Actuals / EFC (§16 Phase 3): record invoices (with the Turkish FATURA tax
 *  math) and see estimate-vs-actual variance + EFC. Online and offline alike. */
export default function ActualsPanel() {
  const { t } = useTranslation();
  const tree = useApp((s) => s.tree);
  const { loadActuals, addActual, removeActual } = useApp();

  const accounts = useMemo(
    () => (tree?.categories ?? []).flatMap((c) => c.accounts.map((a) => ({ id: a.id, label: `${a.number} ${a.name}` }))),
    [tree]
  );

  const [report, setReport] = useState<ActualsReport | null>(null);
  const [account, setAccount] = useState("");
  const [vendor, setVendor] = useState("");
  const [description, setDescription] = useState("");
  const [net, setNet] = useState("");
  const [stopaj, setStopaj] = useState("0");
  const [kdv, setKdv] = useState("20");
  const [tevkifat, setTevkifat] = useState("");
  const [busy, setBusy] = useState(false);
  const [scanOpen, setScanOpen] = useState(false);

  useEffect(() => {
    loadActuals().then(setReport);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  useEffect(() => {
    if (!account && accounts.length) setAccount(accounts[0].id);
  }, [accounts, account]);

  // Percent input → fraction string the core expects ("17" → "0.17").
  const frac = (p: string) => {
    const n = Number(p);
    return Number.isFinite(n) ? (n / 100).toString() : "0";
  };

  const save = async () => {
    if (!account || net.trim() === "" || Number.isNaN(Number(net))) return;
    setBusy(true);
    try {
      await addActual({
        account,
        vendor,
        description,
        net: net.trim(),
        stopaj_rate: frac(stopaj),
        kdv_rate: frac(kdv),
        tevkifat_kind: tevkifat || null,
      });
      setNet("");
      setVendor("");
      setDescription("");
      setReport(await loadActuals());
    } finally {
      setBusy(false);
    }
  };

  const del = async (id: string) => {
    await removeActual(id);
    setReport(await loadActuals());
  };

  const varClass = (v: string) => {
    const n = Number(v);
    return `num diff ${n < 0 ? "pos" : n > 0 ? "neg" : ""}`;
  };

  return (
    <div className="an-panel">
      <div className="an-head-row">
        <h2 className="tools-h">{t("act_record")}</h2>
        {bridge.inTauri && (
          <button className="ta-btn rc-open" onClick={() => setScanOpen(true)}>📷 {t("rc_open")}</button>
        )}
      </div>
      {scanOpen && (
        <ReceiptScanPanel
          onClose={() => {
            setScanOpen(false);
            loadActuals().then(setReport);
          }}
        />
      )}
      <div className="an-form">
        <div className="an-field">
          <label>{t("act_account")}</label>
          <select value={account} onChange={(e) => setAccount(e.target.value)}>
            {accounts.map((a) => (
              <option key={a.id} value={a.id}>{a.label}</option>
            ))}
          </select>
        </div>
        <div className="an-field">
          <label>{t("act_vendor")}</label>
          <input type="text" value={vendor} onChange={(e) => setVendor(e.target.value)} />
        </div>
        <div className="an-field">
          <label>{t("act_desc")}</label>
          <input type="text" value={description} onChange={(e) => setDescription(e.target.value)} />
        </div>
        <div className="an-field">
          <label>{t("act_net")}</label>
          <input type="number" value={net} onChange={(e) => setNet(e.target.value)} />
        </div>
        <div className="an-field">
          <label>{t("act_stopaj")}</label>
          <input type="number" value={stopaj} onChange={(e) => setStopaj(e.target.value)} />
        </div>
        <div className="an-field">
          <label>{t("act_kdv")}</label>
          <input type="number" value={kdv} onChange={(e) => setKdv(e.target.value)} />
        </div>
        <div className="an-field">
          <label>{t("act_tevkifat")}</label>
          <select value={tevkifat} onChange={(e) => setTevkifat(e.target.value)}>
            <option value="">{t("act_tevkifat_none")}</option>
            {TEVKIFAT_KINDS.map((k) => (
              <option key={k} value={k}>{k}</option>
            ))}
          </select>
        </div>
        <button className="auth-go" disabled={busy || !account} onClick={save}>{t("act_save")}</button>
      </div>
      <p className="an-hint">{t("act_tevkifat_hint")}</p>

      <h2 className="tools-h">{t("act_variance_title")}</h2>
      <table>
        <thead>
          <tr>
            <th>{t("act_account")}</th>
            <th className="num">{t("act_estimate")}</th>
            <th className="num">{t("act_actual")}</th>
            <th className="num">{t("act_variance")}</th>
            <th className="num">{t("act_efc")}</th>
          </tr>
        </thead>
        <tbody>
          {(report?.rows ?? []).map((r) => (
            <tr key={r.account_number}>
              <td><span className="code">{r.account_number}</span> {r.account_name}</td>
              <td className="num">{money(r.estimate)}</td>
              <td className="num">{money(r.actual)}</td>
              <td className={varClass(r.variance)}>{money(r.variance)}</td>
              <td className="num">{money(r.efc)}</td>
            </tr>
          ))}
          {report && (
            <tr className="tot">
              <td><b>{t("col_total")}</b></td>
              <td className="num"><b>{money(report.estimate_total)}</b></td>
              <td className="num"><b>{money(report.actual_total)}</b></td>
              <td className={varClass(report.variance_total)}><b>{money(report.variance_total)}</b></td>
              <td className="num"><b>{money(report.efc_total)}</b></td>
            </tr>
          )}
        </tbody>
      </table>

      <h2 className="tools-h">{t("act_invoices")}</h2>
      {report && report.lines.length === 0 ? (
        <div className="empty">{t("act_no_invoices")}</div>
      ) : (
        <table>
          <thead>
            <tr>
              <th>{t("act_account")}</th>
              <th>{t("act_vendor")}</th>
              <th>{t("act_desc")}</th>
              <th className="num">{t("col_net")}</th>
              <th className="num">{t("act_brut")}</th>
              <th className="num">{t("act_col_stopaj")}</th>
              <th className="num">{t("act_col_kdv")}</th>
              <th className="num">{t("act_payable")}</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {(report?.lines ?? []).map((l) => (
              <tr key={l.id}>
                <td><span className="code">{l.account_number}</span> {l.account_name}</td>
                <td>{l.vendor}</td>
                <td className="muted">{l.description}</td>
                <td className="num">{money(l.net)}</td>
                <td className="num">{money(l.brut)}</td>
                <td className="num">{money(l.stopaj)}</td>
                <td className="num">{money(l.kdv)}</td>
                <td className="num">{money(l.payable)}</td>
                <td><button className="del" onClick={() => del(l.id)}>×</button></td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
