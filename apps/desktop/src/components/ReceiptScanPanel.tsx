import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useApp } from "../store";
import { bridge } from "../bridge";
import type { BBox, ReceiptFields, SettingsView, AccountHint } from "../types";

// One receipt under review, mapping the model's fields onto AddActualInput.
interface ScanRow {
  key: string;
  bbox: BBox;
  cropUrl: string;
  fields: ReceiptFields;
  accountId: string;
  date: string;
  vendor: string;
  net: string; // decimal string, "." separator
  kdvPct: string; // percent string
  description: string;
  currency: string;
  selected: boolean;
  addedId?: string;
  error?: string;
  dup?: boolean;
}

const LOW = 0.7; // confidence threshold for the orange highlight

// Number → "1234.56" (dot decimals, the format AddActualInput.net expects).
function numStr(n: number | null | undefined): string {
  if (n == null || !Number.isFinite(n)) return "";
  return (Math.round(n * 100) / 100).toString();
}

// Derive net + KDV% from whatever the model returned.
function deriveNetKdv(f: ReceiptFields): { net: string; kdvPct: string } {
  let net = f.tutar_net;
  if (net == null && f.toplam_tutar != null) {
    net = f.kdv_tutari != null ? f.toplam_tutar - f.kdv_tutari : f.toplam_tutar;
  }
  let pct = 20;
  if (net && net > 0 && f.kdv_tutari != null) {
    const raw = (f.kdv_tutari / net) * 100;
    pct = [1, 10, 20].reduce((a, b) => (Math.abs(b - raw) < Math.abs(a - raw) ? b : a), 20);
  }
  return { net: numStr(net), kdvPct: String(pct) };
}

/** "Fiş ile Otomatik Fatura Kapama": one composite photo → per-receipt crops →
 *  Anthropic vision extraction → editable review table → batch Actuals (with
 *  undo + evidence attachments). Native-only. */
export default function ReceiptScanPanel({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const tree = useApp((s) => s.tree);
  const refresh = useApp((s) => s.refresh);
  const loadActuals = useApp((s) => s.loadActuals);

  const accounts = useMemo(
    () =>
      (tree?.categories ?? []).flatMap((c) =>
        c.accounts.map((a) => ({ id: a.id, number: a.number, name: a.name })),
      ),
    [tree],
  );
  const hints: AccountHint[] = useMemo(
    () => accounts.map((a) => ({ number: a.number, name: a.name })),
    [accounts],
  );

  const [step, setStep] = useState<"pick" | "boxes" | "review">("pick");
  const [settings, setSettings] = useState<SettingsView | null>(null);
  const [keyInput, setKeyInput] = useState("");
  const [showKey, setShowKey] = useState(false);
  const [composite, setComposite] = useState<string>(""); // data URL
  const [dims, setDims] = useState<{ w: number; h: number }>({ w: 1, h: 1 });
  const [boxes, setBoxes] = useState<BBox[]>([]);
  const [rows, setRows] = useState<ScanRow[]>([]);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState("");
  const [error, setError] = useState("");
  const [addedIds, setAddedIds] = useState<string[]>([]);
  const imgRef = useRef<HTMLImageElement>(null);

  useEffect(() => {
    bridge.getSettings().then((s) => s && setSettings(s));
  }, []);

  const saveKey = async () => {
    const s = await bridge.setSettings(keyInput, null);
    if (s) {
      setSettings(s);
      setKeyInput("");
      setShowKey(false);
    }
  };

  const changeModel = async (model: string) => {
    const s = await bridge.setSettings(null, model);
    if (s) setSettings(s);
  };

  const MODELS = [
    { id: "claude-sonnet-5", label: `Sonnet 5 — ${t("rc_model_balanced")}` },
    { id: "claude-opus-4-8", label: `Opus 4.8 — ${t("rc_model_best")}` },
    { id: "claude-haiku-4-5-20251001", label: `Haiku 4.5 — ${t("rc_model_fast")}` },
  ];

  const onFile = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    setError("");
    const reader = new FileReader();
    reader.onload = async () => {
      const url = String(reader.result);
      setComposite(url);
      setBusy(true);
      setProgress(t("rc_segmenting"));
      try {
        const seg = await bridge.segmentReceipts(url);
        setDims({ w: seg.width, h: seg.height });
        setBoxes(seg.boxes.length ? seg.boxes : [centeredBox(seg.width, seg.height)]);
        setStep("boxes");
      } catch (err) {
        setError(String(err));
      } finally {
        setBusy(false);
        setProgress("");
      }
    };
    reader.readAsDataURL(file);
  };

  const centeredBox = (w: number, h: number): BBox => ({
    x: Math.round(w * 0.3),
    y: Math.round(h * 0.3),
    w: Math.round(w * 0.4),
    h: Math.round(h * 0.4),
  });

  // ---- box editor (drag to move, corner handle to resize) ----
  const drag = useRef<{ i: number; mode: "move" | "resize"; sx: number; sy: number; orig: BBox } | null>(null);
  const scale = () => {
    const el = imgRef.current;
    if (!el) return 1;
    return dims.w / el.clientWidth;
  };
  const onPointerDown = (e: React.PointerEvent, i: number, mode: "move" | "resize") => {
    e.stopPropagation();
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    drag.current = { i, mode, sx: e.clientX, sy: e.clientY, orig: { ...boxes[i] } };
  };
  const onPointerMove = (e: React.PointerEvent) => {
    if (!drag.current) return;
    const s = scale();
    const dx = (e.clientX - drag.current.sx) * s;
    const dy = (e.clientY - drag.current.sy) * s;
    setBoxes((bs) =>
      bs.map((b, idx) => {
        if (idx !== drag.current!.i) return b;
        const o = drag.current!.orig;
        if (drag.current!.mode === "move") {
          return {
            ...b,
            x: Math.max(0, Math.min(dims.w - o.w, Math.round(o.x + dx))),
            y: Math.max(0, Math.min(dims.h - o.h, Math.round(o.y + dy))),
          };
        }
        return {
          ...b,
          w: Math.max(20, Math.min(dims.w - o.x, Math.round(o.w + dx))),
          h: Math.max(20, Math.min(dims.h - o.y, Math.round(o.h + dy))),
        };
      }),
    );
  };
  const onPointerUp = () => {
    drag.current = null;
  };

  const pct = (v: number, total: number) => `${(v / total) * 100}%`;

  // ---- scan every box (per-crop Anthropic call) ----
  const scan = async () => {
    if (!settings?.api_key_set) {
      setError(t("rc_need_key"));
      return;
    }
    setBusy(true);
    setError("");
    const existing = await loadActuals();
    const out: ScanRow[] = [];
    for (let i = 0; i < boxes.length; i++) {
      setProgress(t("rc_scanning", { n: i + 1, total: boxes.length }));
      try {
        const res = await bridge.extractReceipt(composite, boxes[i], hints);
        const f = res.fields;
        const { net, kdvPct } = deriveNetKdv(f);
        const acc = accounts.find((a) => a.number === (f.hesap_kodu_onerisi ?? ""));
        const vendor = f.tedarikci ?? "";
        const date = f.tarih ?? "";
        const dup = !!existing?.lines?.some(
          (l) =>
            l.vendor.trim().toLowerCase() === vendor.trim().toLowerCase() &&
            vendor.trim() !== "" &&
            Math.abs(Number(l.net) - Number(net || 0)) < 0.5,
        );
        out.push({
          key: `r${i}`,
          bbox: boxes[i],
          cropUrl: res.crop_data_url,
          fields: f,
          accountId: acc?.id ?? accounts[0]?.id ?? "",
          date,
          vendor,
          net,
          kdvPct,
          description: f.aciklama_onerisi ?? "",
          currency: f.para_birimi || "TRY",
          selected: true,
          dup,
        });
      } catch (err) {
        out.push({
          key: `r${i}`,
          bbox: boxes[i],
          cropUrl: "",
          fields: {} as ReceiptFields,
          accountId: accounts[0]?.id ?? "",
          date: "",
          vendor: "",
          net: "",
          kdvPct: "20",
          description: "",
          currency: "TRY",
          selected: false,
          error: String(err),
        });
      }
    }
    setRows(out);
    setStep("review");
    setBusy(false);
    setProgress("");
  };

  const setRow = (key: string, patch: Partial<ScanRow>) =>
    setRows((rs) => rs.map((r) => (r.key === key ? { ...r, ...patch } : r)));

  const low = (r: ScanRow, field: string) => (r.fields.alan_guven_skorlari?.[field] ?? 1) < LOW;

  // ---- commit selected rows as Actuals + attach evidence ----
  const addSelected = async () => {
    setBusy(true);
    setError("");
    const ids: string[] = [];
    for (const r of rows.filter((x) => x.selected && !x.addedId && !x.error)) {
      const kdv = Number(r.kdvPct);
      // Flag invalid rows visibly rather than silently dropping them.
      if (!r.accountId || r.net.trim() === "" || Number.isNaN(Number(r.net)) || Number.isNaN(kdv)) {
        setRow(r.key, { error: t("rc_invalid_row") });
        continue;
      }
      try {
        const id = await bridge.addActual({
          account: r.accountId,
          date: r.date,
          vendor: r.vendor,
          description: r.description,
          net: r.net,
          stopaj_rate: "0",
          kdv_rate: (kdv / 100).toString(),
        });
        if (id) {
          await bridge.saveReceiptAttachment(id, composite, r.cropUrl);
          setRow(r.key, { addedId: id });
          ids.push(id);
        }
      } catch (err) {
        setRow(r.key, { error: String(err) });
      }
    }
    setAddedIds((a) => [...a, ...ids]);
    await refresh();
    setBusy(false);
  };

  const undo = async () => {
    setBusy(true);
    for (const id of addedIds) {
      await bridge.removeActual(id).catch(() => {});
      await bridge.removeReceiptAttachment(id).catch(() => {});
    }
    setRows((rs) => rs.map((r) => (r.addedId ? { ...r, addedId: undefined } : r)));
    setAddedIds([]);
    await refresh();
    setBusy(false);
  };

  if (!bridge.inTauri) {
    return (
      <div className="rc-overlay">
        <div className="rc-modal">
          <div className="rc-head"><h2>{t("rc_title")}</h2><button className="del" onClick={onClose}>×</button></div>
          <div className="online-note">{t("rc_native_only")}</div>
        </div>
      </div>
    );
  }

  const addedCount = rows.filter((r) => r.addedId).length;

  return (
    <div className="rc-overlay" onClick={onClose}>
      <div className="rc-modal" onClick={(e) => e.stopPropagation()}>
        <div className="rc-head">
          <h2>{t("rc_title")}</h2>
          <button className="del" onClick={onClose}>×</button>
        </div>

        {/* Model + API key settings */}
        {settings && (
          <div className="rc-keybar">
            <label>{t("rc_model")}</label>
            <select value={settings.model} onChange={(e) => changeModel(e.target.value)}>
              {MODELS.every((m) => m.id !== settings.model) && (
                <option value={settings.model}>{settings.model}</option>
              )}
              {MODELS.map((m) => (
                <option key={m.id} value={m.id}>{m.label}</option>
              ))}
            </select>
            {settings.api_key_set && !showKey ? (
              <>
                <span className="rc-key-ok">✓ {t("rc_key_ok")}</span>
                <button className="ta-link" onClick={() => setShowKey(true)}>{t("rc_change_key")}</button>
              </>
            ) : (
              <>
                {!settings.api_key_set && <span>{t("rc_need_key")}</span>}
                <input
                  type="password"
                  placeholder="sk-ant-…"
                  value={keyInput}
                  onChange={(e) => setKeyInput(e.target.value)}
                />
                <button className="ta-btn" disabled={!keyInput.trim()} onClick={saveKey}>{t("rc_save_key")}</button>
              </>
            )}
          </div>
        )}

        {error && <div className="rc-error">{error}</div>}
        {busy && <div className="rc-progress">{progress || "…"}</div>}

        {step === "pick" && (
          <div className="rc-pick">
            <p className="an-hint">{t("rc_pick_hint")}</p>
            <label className="ta-btn rc-file">
              {t("rc_choose")}
              <input type="file" accept="image/*" capture="environment" hidden onChange={onFile} />
            </label>
          </div>
        )}

        {step === "boxes" && (
          <div className="rc-boxes">
            <div className="rc-toolbar no-print">
              <span className="an-hint">{t("rc_boxes_hint")}</span>
              <button className="ta-btn" onClick={() => setBoxes((b) => [...b, centeredBox(dims.w, dims.h)])}>
                {t("rc_add_box")}
              </button>
              <button className="auth-go" disabled={busy || boxes.length === 0} onClick={scan}>
                {t("rc_scan", { n: boxes.length })}
              </button>
            </div>
            <div
              className="rc-canvas"
              onPointerMove={onPointerMove}
              onPointerUp={onPointerUp}
            >
              <img ref={imgRef} src={composite} alt="composite" draggable={false} />
              {boxes.map((b, i) => (
                <div
                  key={i}
                  className="rc-box"
                  style={{ left: pct(b.x, dims.w), top: pct(b.y, dims.h), width: pct(b.w, dims.w), height: pct(b.h, dims.h) }}
                  onPointerDown={(e) => onPointerDown(e, i, "move")}
                >
                  <span className="rc-box-n">{i + 1}</span>
                  <button
                    className="rc-box-del"
                    onPointerDown={(e) => e.stopPropagation()}
                    onClick={() => setBoxes((bs) => bs.filter((_, idx) => idx !== i))}
                  >×</button>
                  <span className="rc-box-resize" onPointerDown={(e) => onPointerDown(e, i, "resize")} />
                </div>
              ))}
            </div>
          </div>
        )}

        {step === "review" && (
          <div className="rc-review">
            <div className="rc-toolbar no-print">
              <button className="ta-btn" onClick={() => setStep("boxes")}>← {t("rc_back_boxes")}</button>
              <span className="an-hint">{t("rc_review_hint")}</span>
              <button className="auth-go" disabled={busy} onClick={addSelected}>
                {t("rc_add_selected", { n: rows.filter((r) => r.selected && !r.addedId && !r.error).length })}
              </button>
              {addedCount > 0 && <button className="ta-btn" onClick={undo}>{t("rc_undo", { n: addedIds.length })}</button>}
            </div>
            <table className="rc-table">
              <thead>
                <tr>
                  <th></th><th></th>
                  <th>{t("rc_vendor")}</th>
                  <th>{t("rc_date")}</th>
                  <th className="num">{t("rc_net")}</th>
                  <th className="num">{t("rc_kdv")}</th>
                  <th>{t("rc_account")}</th>
                  <th>{t("rc_desc")}</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {rows.map((r) => (
                  <tr key={r.key} className={r.addedId ? "rc-added" : r.error ? "rc-row-err" : ""}>
                    <td>
                      <input type="checkbox" checked={r.selected} disabled={!!r.addedId || !!r.error}
                        onChange={(e) => setRow(r.key, { selected: e.target.checked })} />
                    </td>
                    <td>{r.cropUrl && <img className="rc-thumb" src={r.cropUrl} alt="crop" />}</td>
                    {r.error ? (
                      <td colSpan={6} className="rc-err-cell">{r.error}</td>
                    ) : (
                      <>
                        <td>
                          <input className={low(r, "tedarikci") ? "rc-low" : ""} value={r.vendor}
                            onChange={(e) => setRow(r.key, { vendor: e.target.value })} />
                          {r.dup && <span className="rc-dup" title={t("rc_dup_tip")}>⚠</span>}
                        </td>
                        <td>
                          <input className={low(r, "tarih") ? "rc-low" : ""} placeholder="GG.AA.YYYY" value={r.date}
                            onChange={(e) => setRow(r.key, { date: e.target.value })} />
                        </td>
                        <td className="num">
                          <input className={"num " + (low(r, "tutar_net") || low(r, "toplam_tutar") ? "rc-low" : "")}
                            value={r.net} onChange={(e) => setRow(r.key, { net: e.target.value })} />
                        </td>
                        <td className="num">
                          <input className="num" value={r.kdvPct} onChange={(e) => setRow(r.key, { kdvPct: e.target.value })} />
                        </td>
                        <td>
                          <select className={low(r, "hesap_kodu_onerisi") ? "rc-low" : ""} value={r.accountId}
                            onChange={(e) => setRow(r.key, { accountId: e.target.value })}>
                            {accounts.map((a) => (
                              <option key={a.id} value={a.id}>{a.number} {a.name}</option>
                            ))}
                          </select>
                        </td>
                        <td>
                          <input value={r.description} onChange={(e) => setRow(r.key, { description: e.target.value })} />
                        </td>
                      </>
                    )}
                    <td>{r.addedId ? <span className="rc-ok">✓</span> : ""}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  );
}
