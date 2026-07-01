import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useApp } from "../store";
import { money } from "../fmt";
import type { CategoryNode, DetailRow, FormulaDto } from "../types";

/** Inline-editable cell, committing through the store (server or local). */
function EditCell({ initial, field, detail, numeric }: { initial: string; field: string; detail: string; numeric?: boolean }) {
  const editDetail = useApp((s) => s.editDetail);
  const [v, setV] = useState(initial);
  const commit = () => {
    if (v !== initial) editDetail(detail, field, v);
  };
  return (
    <input
      className={`cell-input ${numeric ? "num" : ""}`}
      value={v}
      onChange={(e) => setV(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") (e.target as HTMLInputElement).blur();
      }}
    />
  );
}

const fdisp = (f: FormulaDto) => (f.is_expr ? `=${f.text}` : f.text);
const pct = (r: string) => `${Math.round(Number(r) * 100)}%`;

/** Applied-fringes cell: chips (Apply-Tools) + a picker to add one. */
function FringeCell({ d }: { d: DetailRow }) {
  const { tools, editFringes } = useApp();
  const [open, setOpen] = useState(false);
  const available = (tools?.fringes ?? []).filter((f) => !d.fringes.some((x) => x.code === f.code));
  const [code, setCode] = useState("");
  const [rate, setRate] = useState("");

  const cur = () => d.fringes.map((x) => ({ code: x.code, rate: x.rate }));

  const add = () => {
    const c = code || available[0]?.code;
    if (!c) return;
    const r = rate || tools?.fringes.find((f) => f.code === c)?.rate || "0";
    editFringes(d.id, [...cur(), { code: c, rate: r }]);
    setOpen(false);
    setCode("");
    setRate("");
  };
  const remove = (c: string) => editFringes(d.id, cur().filter((x) => x.code !== c));

  return (
    <div className="fringe-cell">
      {d.fringes.map((f) => (
        <span key={f.code} className={`fchip ${f.code.includes("STOPAJ") ? "gu" : "add"}`}>
          {f.code.replace("TR_", "")} {pct(f.rate)}
          <button className="fx" onClick={() => remove(f.code)}>×</button>
        </span>
      ))}
      {available.length > 0 && (
        <span className="fadd-wrap">
          <button className="fadd" onClick={() => setOpen(!open)}>+ fringe</button>
          {open && (
            <div className="fpop" onClick={(e) => e.stopPropagation()}>
              <select value={code || available[0].code} onChange={(e) => { setCode(e.target.value); setRate(tools?.fringes.find((f) => f.code === e.target.value)?.rate || ""); }}>
                {available.map((f) => (
                  <option key={f.code} value={f.code}>{f.name}</option>
                ))}
              </select>
              <input className="frate" placeholder="oran (0.20)" value={rate} onChange={(e) => setRate(e.target.value)} />
              <button className="fapply" onClick={add}>Ekle</button>
            </div>
          )}
        </span>
      )}
    </div>
  );
}

function CategoryBlock({ cat }: { cat: CategoryNode }) {
  const { t } = useTranslation();
  const { addLine, removeLine } = useApp();
  const hasLines = cat.accounts.some((a) => a.details.length > 0);
  const [open, setOpen] = useState(hasLines);

  return (
    <div className="cat-block">
      <div className="cat-head" onClick={() => setOpen(!open)}>
        <div className="left">
          <span className="muted">{open ? "▾" : "▸"}</span>
          <span className="code">{cat.number}</span>
          {cat.name}
          {cat.atl_btl && <span className={`tag ${cat.atl_btl}`}>{cat.atl_btl}</span>}
        </div>
        <div className="amt">{money(cat.total)}</div>
      </div>

      {open &&
        cat.accounts
          .filter((a) => a.details.length > 0)
          .map((a) => (
            <div key={a.id}>
              <div className="acct-name">
                <span className="code">{a.number}</span>
                {a.name}
                <button className="add-line" onClick={() => addLine(a.id)}>+ {t("col_desc")}</button>
              </div>
              <table className="grid">
                <thead>
                  <tr>
                    <th>{t("col_desc")}</th>
                    <th className="num">{t("col_qty")}</th>
                    <th className="num">×</th>
                    <th>{t("col_unit")}</th>
                    <th className="num">{t("col_rate")}</th>
                    <th className="num">{t("col_net")}</th>
                    <th>{t("tools_fringes")}</th>
                    <th className="num">{t("col_reflect")}</th>
                    <th className="num">{t("col_grand")}</th>
                    <th></th>
                  </tr>
                </thead>
                <tbody>
                  {a.details.map((d) => (
                    <tr key={d.id} className={`detail-row ${d.error ? "err" : ""}`}>
                      <td><EditCell detail={d.id} field="description" initial={d.description} /></td>
                      <td className="num"><EditCell detail={d.id} field="amount" initial={fdisp(d.amount)} numeric /></td>
                      <td className="num"><EditCell detail={d.id} field="multiplier" initial={fdisp(d.multiplier)} numeric /></td>
                      <td className="muted">{d.unit}</td>
                      <td className="num"><EditCell detail={d.id} field="rate" initial={fdisp(d.rate)} numeric /></td>
                      <td className="num">{money(d.subtotal)}</td>
                      <td><FringeCell d={d} /></td>
                      <td className="num muted">{money(d.fringe_total)}</td>
                      <td className="num">{money(d.line_total)}</td>
                      <td className="num"><button className="del-line" title="Sil" onClick={() => removeLine(d.id)}>×</button></td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ))}
    </div>
  );
}

export default function AccountDetails() {
  const tree = useApp((s) => s.tree);
  if (!tree) return null;
  const visible = tree.categories.filter((c) => c.accounts.some((a) => a.details.length > 0));
  return (
    <div>
      {visible.map((c) => (
        <CategoryBlock key={c.id} cat={c} />
      ))}
    </div>
  );
}
