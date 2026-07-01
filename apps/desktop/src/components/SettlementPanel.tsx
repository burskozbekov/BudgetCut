import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useApp } from "../store";
import { money } from "../fmt";
import type { SettlementReport } from "../types";

// Production expense categories (GİDER) — Turkish domain terms, the rollup key.
const CATEGORIES = [
  "TOPLU TAŞIMA",
  "YEMEK",
  "OFİS İKRAM",
  "YAKIT",
  "KÖPRÜ GEÇİŞ",
  "OTOPARK",
  "ARAÇ BAKIM",
  "OGS-HGS",
  "SEYAHAT",
  "TELEFON",
  "KONAKLAMA",
  "MEKAN GİDERLERİ",
  "SET SARF",
  "KAMERA SARF",
  "IŞIK SARF",
  "SES SARF",
  "SAÇ MAKYAJ",
  "SAĞLIK",
  "KIRTASİYE",
  "SANAT-DEKOR",
  "SANAT SARF",
  "AKSESUAR YEMEK",
  "KOSTÜM",
  "KOSTÜM SARF",
  "KURU TEMİZLEME",
  "BAĞIŞ KDVSİZ",
  "DİĞER",
];
const KDV_RATES = ["0", "1", "10", "20"];

// The settlement form's header + signatory metadata (matches the printed
// "icmal" document). Stored locally per budget — it's document-prep metadata,
// not synced budget data.
interface Header {
  company: string;
  vkn: string;
  project: string;
  department: string;
  formNo: string;
  spender: string;
  holder: string;
  holderRole: string;
  control: string;
  controlRole: string;
}
const EMPTY_HEADER: Header = {
  company: "",
  vkn: "",
  project: "",
  department: "",
  formNo: "",
  spender: "",
  holder: "",
  holderRole: "",
  control: "",
  controlRole: "",
};

/** Expense settlement / "Hesap Kapama" (§16): record KDV-inclusive receipts,
 *  extract VAT backwards, roll up by category, reconcile a cash advance.
 *  Works online and offline alike, and prints the official "icmal" form. */
export default function SettlementPanel() {
  const { t } = useTranslation();
  const { loadSettlement, addReceipt, removeReceipt } = useApp();
  const budgetId = useApp((s) => s.currentBudgetId);
  const budgetName = useApp((s) => s.currentBudgetName);
  const hKey = `budgetcut.settlement.${budgetId ?? "local"}`;

  const [report, setReport] = useState<SettlementReport | null>(null);
  const [header, setHeader] = useState<Header>(EMPTY_HEADER);
  const [date, setDate] = useState("");
  const [vendor, setVendor] = useState("");
  const [receiptNo, setReceiptNo] = useState("");
  const [category, setCategory] = useState(CATEGORIES[0]);
  const [description, setDescription] = useState("");
  const [gross, setGross] = useState("");
  const [kdv, setKdv] = useState("10");
  const [advance, setAdvance] = useState("");
  const [busy, setBusy] = useState(false);

  const reload = () => loadSettlement(advance.trim() || undefined).then(setReport);
  useEffect(() => {
    reload();
    try {
      const saved = localStorage.getItem(hKey);
      if (saved) setHeader({ ...EMPTY_HEADER, ...JSON.parse(saved) });
    } catch {
      /* ignore corrupt header */
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const setH = (patch: Partial<Header>) => {
    const next = { ...header, ...patch };
    setHeader(next);
    try {
      localStorage.setItem(hKey, JSON.stringify(next));
    } catch {
      /* quota/full — header just won't persist */
    }
  };

  const frac = (p: string) => {
    const n = Number(p);
    return Number.isFinite(n) ? (n / 100).toString() : "0";
  };

  const save = async () => {
    if (gross.trim() === "" || Number.isNaN(Number(gross))) return;
    setBusy(true);
    try {
      await addReceipt({
        date,
        vendor,
        receipt_no: receiptNo,
        category,
        description,
        gross: gross.trim(),
        kdv_rate: frac(kdv),
      });
      setGross("");
      setVendor("");
      setReceiptNo("");
      setDescription("");
      await reload();
    } finally {
      setBusy(false);
    }
  };

  const del = async (id: string) => {
    await removeReceipt(id);
    await reload();
  };

  const hf = (k: keyof Header, label: string) => (
    <div className="an-field">
      <label>{label}</label>
      <input type="text" value={header[k]} onChange={(e) => setH({ [k]: e.target.value })} />
    </div>
  );

  return (
    <div className="an-panel">
      <div className="set-head-row">
        <h2 className="tools-h" style={{ margin: 0 }}>{t("set_form_info")}</h2>
        <button className="auth-toggle" onClick={() => window.print()}>{t("set_print")}</button>
      </div>
      <div className="an-form">
        {hf("company", t("set_company"))}
        {hf("vkn", t("set_vkn"))}
        {hf("project", t("set_project"))}
        {hf("department", t("set_department"))}
        {hf("formNo", t("set_form_no"))}
        {hf("spender", t("set_spender"))}
        {hf("holder", t("set_holder"))}
        {hf("holderRole", t("set_role"))}
        {hf("control", t("set_control"))}
        {hf("controlRole", t("set_role"))}
      </div>

      <h2 className="tools-h">{t("set_record")}</h2>
      <div className="an-form">
        <div className="an-field">
          <label>{t("set_date")}</label>
          <input type="text" value={date} onChange={(e) => setDate(e.target.value)} placeholder={t("ph_date")} />
        </div>
        <div className="an-field">
          <label>{t("set_vendor")}</label>
          <input type="text" value={vendor} onChange={(e) => setVendor(e.target.value)} />
        </div>
        <div className="an-field">
          <label>{t("set_receipt_no")}</label>
          <input type="text" value={receiptNo} onChange={(e) => setReceiptNo(e.target.value)} />
        </div>
        <div className="an-field">
          <label>{t("set_category")}</label>
          <select value={category} onChange={(e) => setCategory(e.target.value)}>
            {CATEGORIES.map((c) => (
              <option key={c} value={c}>{c}</option>
            ))}
          </select>
        </div>
        <div className="an-field">
          <label>{t("set_desc")}</label>
          <input type="text" value={description} onChange={(e) => setDescription(e.target.value)} />
        </div>
        <div className="an-field">
          <label>{t("set_gross")}</label>
          <input type="number" value={gross} onChange={(e) => setGross(e.target.value)} />
        </div>
        <div className="an-field">
          <label>{t("set_kdv")}</label>
          <select value={kdv} onChange={(e) => setKdv(e.target.value)}>
            {KDV_RATES.map((r) => (
              <option key={r} value={r}>{r}</option>
            ))}
          </select>
        </div>
        <button className="auth-go" disabled={busy} onClick={save}>{t("set_save")}</button>
      </div>

      <h2 className="tools-h">{t("set_rollup")}</h2>
      <table>
        <thead>
          <tr>
            <th>{t("set_category")}</th>
            <th className="num">{t("set_col_kdvli")}</th>
            <th className="num">{t("act_col_kdv")}</th>
            <th className="num">{t("set_col_kdvsiz")}</th>
          </tr>
        </thead>
        <tbody>
          {(report?.categories ?? []).map((c) => (
            <tr key={c.category}>
              <td><span className="code">{c.category}</span></td>
              <td className="num">{money(c.gross)}</td>
              <td className="num">{money(c.kdv)}</td>
              <td className="num">{money(c.net)}</td>
            </tr>
          ))}
          {report && (
            <tr className="tot">
              <td><b>{t("set_grand_total")}</b></td>
              <td className="num"><b>{money(report.gross_total)}</b></td>
              <td className="num"><b>{money(report.kdv_total)}</b></td>
              <td className="num"><b>{money(report.net_total)}</b></td>
            </tr>
          )}
        </tbody>
      </table>

      {report && (
        <div className="kpi-grid">
          <div className="an-field" style={{ alignSelf: "center" }}>
            <label>{t("set_advance")}</label>
            <input type="number" value={advance} onChange={(e) => setAdvance(e.target.value)} onBlur={reload} />
          </div>
          <div className="kpi">
            <div className="k-label">{t("set_total_kdv")}</div>
            <div className="k-val">{money(report.kdv_total)}</div>
          </div>
          <div className="kpi hero">
            <div className="k-label">{report.refund ? t("set_refund") : t("set_reimburse")}</div>
            <div className="k-val">{money(report.balance)}</div>
          </div>
        </div>
      )}

      <h2 className="tools-h">{t("set_receipts")}</h2>
      {report && report.lines.length === 0 ? (
        <div className="empty">{t("set_no_receipts")}</div>
      ) : (
        <table>
          <thead>
            <tr>
              <th>{t("set_date")}</th>
              <th>{t("set_vendor")}</th>
              <th>{t("set_category")}</th>
              <th>{t("set_desc")}</th>
              <th className="num">{t("set_col_kdvli")}</th>
              <th className="num">{t("act_col_kdv")}</th>
              <th className="num">{t("set_col_kdvsiz")}</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {(report?.lines ?? []).map((l) => (
              <tr key={l.id}>
                <td className="muted">{l.date}</td>
                <td>{l.vendor}</td>
                <td><span className="code">{l.category}</span></td>
                <td className="muted">{l.description}</td>
                <td className="num">{money(l.gross)}</td>
                <td className="num">{money(l.kdv)}</td>
                <td className="num">{money(l.net)}</td>
                <td><button className="del" onClick={() => del(l.id)}>×</button></td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {/* Print-only "icmal" form (matches the official sheet). React escapes
          all interpolated text, so user input can't inject markup. */}
      {report && (
        <div className="print-doc">
          <h1 className="pd-title">{t("set_doc_title")}</h1>
          <table className="pd-header">
            <tbody>
              <tr>
                <td><b>{t("set_company")}:</b> {header.company}</td>
                <td><b>{t("set_vkn")}:</b> {header.vkn}</td>
                <td><b>{t("set_project")}:</b> {header.project || budgetName}</td>
              </tr>
              <tr>
                <td><b>{t("set_department")}:</b> {header.department}</td>
                <td><b>{t("set_form_no")}:</b> {header.formNo}</td>
                <td><b>{t("set_date")}:</b> {date}</td>
              </tr>
            </tbody>
          </table>

          <table className="pd-lines">
            <thead>
              <tr>
                <th>#</th>
                <th>{t("set_date")}</th>
                <th>{t("set_vendor")}</th>
                <th>{t("set_receipt_no")}</th>
                <th>{t("set_desc")}</th>
                <th>{t("set_category")}</th>
                <th className="num">{t("set_col_kdvli")}</th>
                <th className="num">{t("act_col_kdv")}</th>
                <th className="num">{t("set_col_kdvsiz")}</th>
              </tr>
            </thead>
            <tbody>
              {report.lines.map((l, i) => (
                <tr key={l.id}>
                  <td>{i + 1}</td>
                  <td>{l.date}</td>
                  <td>{l.vendor}</td>
                  <td>{l.receipt_no}</td>
                  <td>{l.description}</td>
                  <td>{l.category}</td>
                  <td className="num">{money(l.gross)}</td>
                  <td className="num">{money(l.kdv)}</td>
                  <td className="num">{money(l.net)}</td>
                </tr>
              ))}
              <tr className="pd-total">
                <td colSpan={6}>{t("set_grand_total")}</td>
                <td className="num">{money(report.gross_total)}</td>
                <td className="num">{money(report.kdv_total)}</td>
                <td className="num">{money(report.net_total)}</td>
              </tr>
            </tbody>
          </table>

          <table className="pd-summary">
            <tbody>
              <tr>
                <td><b>{t("set_total_kdv")}:</b> {money(report.kdv_total)}</td>
                <td><b>{t("set_advance")}:</b> {money(report.advance)}</td>
                <td>
                  <b>{report.refund ? t("set_refund") : t("set_reimburse")}:</b> {money(report.balance)}
                </td>
              </tr>
            </tbody>
          </table>

          <table className="pd-sign">
            <tbody>
              <tr>
                <td>
                  <div className="pd-sig-label">{t("set_spender")}</div>
                  <div className="pd-sig-name">{header.spender}</div>
                </td>
                <td>
                  <div className="pd-sig-label">{t("set_holder")}</div>
                  <div className="pd-sig-name">{header.holder}</div>
                  <div className="pd-sig-role">{header.holderRole}</div>
                </td>
                <td>
                  <div className="pd-sig-label">{t("set_control")}</div>
                  <div className="pd-sig-name">{header.control}</div>
                  <div className="pd-sig-role">{header.controlRole}</div>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
