import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { bridge } from "../bridge";
import { api } from "../api";
import type { LiveRates } from "../types";

// tr-TR display for a decimal string like "46.6706" → "46,67".
function n2(v: string | null): string {
  if (!v) return "—";
  const n = Number(v);
  return Number.isFinite(n)
    ? n.toLocaleString("tr-TR", { minimumFractionDigits: 2, maximumFractionDigits: 2 })
    : "—";
}

const REFRESH_MS = 6 * 60 * 60 * 1000; // TCMB updates once per business day

/** Top-right live panel: today's TCMB USD/EUR selling rates + İstanbul pump
 *  prices. Native app fetches directly; browser goes through the server proxy.
 *  Fails soft to "—" with no impact on the budget views. */
export default function TopRates() {
  const { t } = useTranslation();
  const [rates, setRates] = useState<LiveRates | null>(null);
  const [busy, setBusy] = useState(false);

  const load = async () => {
    setBusy(true);
    try {
      const r = bridge.inTauri ? await bridge.liveRates() : await api.rates();
      setRates(r);
    } catch {
      setRates(null);
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    load();
    const id = setInterval(load, REFRESH_MS);
    return () => clearInterval(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const title = rates?.date
    ? `${t("rates_title")} — TCMB ${rates.date}`
    : t("rates_title");

  return (
    <button className={`rates ${busy ? "busy" : ""}`} onClick={load} title={title}>
      <span className="r-item">
        <span className="r-k">$</span> {n2(rates?.usd ?? null)}
      </span>
      <span className="r-item">
        <span className="r-k">€</span> {n2(rates?.eur ?? null)}
      </span>
      <span className="r-item" title={t("rates_fuel_title")}>
        <span className="r-k">⛽</span> {n2(rates?.benzin ?? null)}
        <span className="r-unit">₺/lt</span>
      </span>
    </button>
  );
}
