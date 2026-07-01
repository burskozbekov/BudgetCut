import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useApp } from "../store";
import type { Schedule } from "../types";

/** Scheduling / stripboard (§16): lay scenes out by shooting day and read the
 *  Day-Out-of-Days (start/finish/work/hold per element). Online + offline. */
export default function SchedulePanel() {
  const { t } = useTranslation();
  const { loadSchedule, addStrip, removeStrip } = useApp();

  const [sched, setSched] = useState<Schedule | null>(null);
  const [day, setDay] = useState("1");
  const [scene, setScene] = useState("");
  const [set, setSet] = useState("");
  const [eighths, setEighths] = useState("8");
  const [elements, setElements] = useState("");
  const [busy, setBusy] = useState(false);

  const reload = () => loadSchedule().then(setSched);
  useEffect(() => {
    reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const posInt = (v: string, min: number) => Math.max(min, Math.floor(Number(v)) || min);

  const save = async () => {
    if (scene.trim() === "") return;
    setBusy(true);
    try {
      await addStrip({
        day: posInt(day, 1),
        scene: scene.trim(),
        set: set.trim(),
        eighths: posInt(eighths, 0),
        elements: elements
          .split(",")
          .map((e) => e.trim())
          .filter((e) => e !== ""),
      });
      setScene("");
      setSet("");
      setElements("");
      await reload();
    } finally {
      setBusy(false);
    }
  };

  const del = async (id: string) => {
    await removeStrip(id);
    await reload();
  };

  return (
    <div className="an-panel">
      <h2 className="tools-h">{t("sch_add")}</h2>
      <div className="an-form">
        <div className="an-field">
          <label>{t("sch_day")}</label>
          <input type="number" min={1} step={1} value={day} onChange={(e) => setDay(e.target.value)} />
        </div>
        <div className="an-field">
          <label>{t("sch_scene")}</label>
          <input type="text" value={scene} onChange={(e) => setScene(e.target.value)} />
        </div>
        <div className="an-field">
          <label>{t("sch_set")}</label>
          <input type="text" value={set} onChange={(e) => setSet(e.target.value)} />
        </div>
        <div className="an-field">
          <label>{t("sch_eighths")}</label>
          <input type="number" min={0} step={1} value={eighths} onChange={(e) => setEighths(e.target.value)} />
        </div>
        <div className="an-field" style={{ flex: 1, minWidth: 200 }}>
          <label>{t("sch_elements")}</label>
          <input type="text" value={elements} onChange={(e) => setElements(e.target.value)} placeholder={t("ph_elements")} />
        </div>
        <button className="auth-go" disabled={busy} onClick={save}>{t("sch_save")}</button>
      </div>

      {sched && (
        <div className="kpi-grid">
          <div className="kpi hero">
            <div className="k-label">{t("sch_total_days")}</div>
            <div className="k-val">{sched.total_days}</div>
          </div>
          <div className="kpi">
            <div className="k-label">{t("sch_total_eighths")}</div>
            <div className="k-val">{sched.total_eighths}</div>
          </div>
        </div>
      )}

      <h2 className="tools-h">{t("sch_dood")}</h2>
      <table>
        <thead>
          <tr>
            <th>{t("sch_element")}</th>
            <th className="num">{t("sch_start")}</th>
            <th className="num">{t("sch_finish")}</th>
            <th className="num">{t("sch_work")}</th>
            <th className="num">{t("sch_hold")}</th>
          </tr>
        </thead>
        <tbody>
          {(sched?.dood ?? []).map((d) => (
            <tr key={d.element}>
              <td><span className="code">{d.element}</span></td>
              <td className="num">{d.start_day}</td>
              <td className="num">{d.finish_day}</td>
              <td className="num">{d.work_days}</td>
              <td className="num">{d.hold_days}</td>
            </tr>
          ))}
        </tbody>
      </table>

      <h2 className="tools-h">{t("sch_strips")}</h2>
      {sched && sched.strips.length === 0 ? (
        <div className="empty">{t("sch_no_strips")}</div>
      ) : (
        <table>
          <thead>
            <tr>
              <th className="num">{t("sch_day")}</th>
              <th>{t("sch_scene")}</th>
              <th>{t("sch_set")}</th>
              <th className="num">{t("sch_eighths")}</th>
              <th>{t("sch_element")}</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {(sched?.strips ?? []).map((s) => (
              <tr key={s.id}>
                <td className="num">{s.day}</td>
                <td>{s.scene}</td>
                <td className="muted">{s.set}</td>
                <td className="num">{s.eighths}</td>
                <td className="muted">{s.elements.join(", ")}</td>
                <td><button className="del" onClick={() => del(s.id)}>×</button></td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
