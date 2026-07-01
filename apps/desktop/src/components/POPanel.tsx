import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useApp } from "../store";
import { money } from "../fmt";
import type { PurchaseOrders } from "../types";

/** Purchase orders + approval workflow: Draft → Approved → Converted (to an
 *  actual). Committed = approved + converted. Online + offline alike. */
export default function POPanel() {
  const { t } = useTranslation();
  const tree = useApp((s) => s.tree);
  const { loadPurchaseOrders, addPo, approvePo, convertPo, removePo } = useApp();

  const accounts = useMemo(
    () => (tree?.categories ?? []).flatMap((c) => c.accounts.map((a) => ({ id: a.id, label: `${a.number} ${a.name}` }))),
    [tree]
  );

  const [data, setData] = useState<PurchaseOrders | null>(null);
  const [account, setAccount] = useState("");
  const [vendor, setVendor] = useState("");
  const [description, setDescription] = useState("");
  const [amount, setAmount] = useState("");
  const [date, setDate] = useState("");
  const [busy, setBusy] = useState(false);

  const reload = () => loadPurchaseOrders().then(setData);
  useEffect(() => {
    reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  useEffect(() => {
    if (!account && accounts.length) setAccount(accounts[0].id);
  }, [accounts, account]);

  const save = async () => {
    if (!account || amount.trim() === "" || Number.isNaN(Number(amount))) return;
    setBusy(true);
    try {
      await addPo({ account, vendor, description, amount: amount.trim(), date });
      setAmount("");
      setVendor("");
      setDescription("");
      await reload();
    } finally {
      setBusy(false);
    }
  };

  const act = async (fn: (id: string) => Promise<void>, id: string) => {
    await fn(id);
    await reload();
  };

  const statusLabel = (s: string) =>
    s === "draft" ? t("po_draft") : s === "approved" ? t("po_approved") : t("po_converted");

  return (
    <div className="an-panel">
      <h2 className="tools-h">{t("po_add")}</h2>
      <div className="an-form">
        <div className="an-field">
          <label>{t("po_account")}</label>
          <select value={account} onChange={(e) => setAccount(e.target.value)}>
            {accounts.map((a) => (
              <option key={a.id} value={a.id}>{a.label}</option>
            ))}
          </select>
        </div>
        <div className="an-field">
          <label>{t("po_vendor")}</label>
          <input type="text" value={vendor} onChange={(e) => setVendor(e.target.value)} />
        </div>
        <div className="an-field">
          <label>{t("po_desc")}</label>
          <input type="text" value={description} onChange={(e) => setDescription(e.target.value)} />
        </div>
        <div className="an-field">
          <label>{t("po_amount")}</label>
          <input type="number" value={amount} onChange={(e) => setAmount(e.target.value)} />
        </div>
        <div className="an-field">
          <label>{t("po_date")}</label>
          <input type="text" value={date} onChange={(e) => setDate(e.target.value)} placeholder={t("ph_date")} />
        </div>
        <button className="auth-go" disabled={busy || !account} onClick={save}>{t("po_save")}</button>
      </div>

      {data && (
        <div className="kpi-grid">
          <div className="kpi">
            <div className="k-label">{t("po_draft_total")}</div>
            <div className="k-val">{money(data.draft_total)}</div>
          </div>
          <div className="kpi">
            <div className="k-label">{t("po_approved_total")}</div>
            <div className="k-val">{money(data.approved_total)}</div>
          </div>
          <div className="kpi hero">
            <div className="k-label">{t("po_committed")}</div>
            <div className="k-val">{money(data.committed_total)}</div>
          </div>
        </div>
      )}

      <h2 className="tools-h">{t("po_list")}</h2>
      {data && data.orders.length === 0 ? (
        <div className="empty">{t("po_no_orders")}</div>
      ) : (
        <table>
          <thead>
            <tr>
              <th>{t("po_date")}</th>
              <th>{t("po_account")}</th>
              <th>{t("po_vendor")}</th>
              <th>{t("po_desc")}</th>
              <th className="num">{t("po_amount")}</th>
              <th>{t("po_status")}</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {(data?.orders ?? []).map((o) => (
              <tr key={o.id}>
                <td className="muted">{o.date}</td>
                <td><span className="code">{o.account_number}</span> {o.account_name}</td>
                <td>{o.vendor}</td>
                <td className="muted">{o.description}</td>
                <td className="num">{money(o.amount)}</td>
                <td><span className={`po-badge ${o.status}`}>{statusLabel(o.status)}</span></td>
                <td className="po-actions">
                  {o.status === "draft" && (
                    <button onClick={() => act(approvePo, o.id)}>{t("po_approve")}</button>
                  )}
                  {o.status === "approved" && (
                    <button className="open" onClick={() => act(convertPo, o.id)}>{t("po_convert")}</button>
                  )}
                  <button className="del" onClick={() => act(removePo, o.id)}>×</button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
