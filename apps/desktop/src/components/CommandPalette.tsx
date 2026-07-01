import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import i18n from "../i18n";
import { useApp } from "../store";

interface Cmd {
  id: string;
  label: string;
  hint?: string;
  run: () => void;
}

export default function CommandPalette() {
  const { t } = useTranslation();
  const { paletteOpen, setPalette, setView } = useApp();
  const [q, setQ] = useState("");
  const [sel, setSel] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  // ⌘K / Ctrl+K toggles the palette anywhere.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPalette(!useApp.getState().paletteOpen);
      } else if (e.key === "Escape") {
        setPalette(false);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [setPalette]);

  useEffect(() => {
    if (paletteOpen) {
      setQ("");
      setSel(0);
      setTimeout(() => inputRef.current?.focus(), 10);
    }
  }, [paletteOpen]);

  const cmds: Cmd[] = useMemo(
    () => [
      { id: "v-top", label: t("cmd_go_topsheet"), hint: t("nav_topsheet"), run: () => setView("topsheet") },
      { id: "v-det", label: t("cmd_go_details"), hint: t("nav_details"), run: () => setView("details") },
      { id: "v-tools", label: t("cmd_go_tools"), hint: t("nav_tools"), run: () => setView("tools") },
      { id: "lang-tr", label: t("cmd_lang_tr"), run: () => i18n.changeLanguage("tr") },
      { id: "lang-en", label: t("cmd_lang_en"), run: () => i18n.changeLanguage("en") },
    ],
    [t, setView]
  );

  const filtered = cmds.filter((c) => c.label.toLowerCase().includes(q.toLowerCase()));

  if (!paletteOpen) return null;

  const choose = (c?: Cmd) => {
    if (!c) return;
    c.run();
    setPalette(false);
  };

  return (
    <div className="palette-backdrop" onClick={() => setPalette(false)}>
      <div className="palette" onClick={(e) => e.stopPropagation()}>
        <input
          ref={inputRef}
          className="palette-input"
          placeholder={t("cmd_placeholder")}
          value={q}
          onChange={(e) => {
            setQ(e.target.value);
            setSel(0);
          }}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") setSel((s) => Math.min(s + 1, filtered.length - 1));
            else if (e.key === "ArrowUp") setSel((s) => Math.max(s - 1, 0));
            else if (e.key === "Enter") choose(filtered[sel]);
          }}
        />
        <div className="palette-list">
          {filtered.map((c, i) => (
            <div
              key={c.id}
              className={`palette-item ${i === sel ? "sel" : ""}`}
              onMouseEnter={() => setSel(i)}
              onClick={() => choose(c)}
            >
              <span>{c.label}</span>
              {c.hint && <span className="muted">{c.hint}</span>}
            </div>
          ))}
          {filtered.length === 0 && <div className="palette-item muted">—</div>}
        </div>
      </div>
    </div>
  );
}
