// Display formatting only — the engine computes every number; here we just
// render the kuruş-precise decimal strings in tr-TR currency format.

const fmt = new Intl.NumberFormat("tr-TR", {
  style: "currency",
  currency: "TRY",
  minimumFractionDigits: 2,
});

export function money(decimalStr: string): string {
  const n = Number(decimalStr);
  if (Number.isNaN(n)) return decimalStr;
  return fmt.format(n);
}

export function isZero(decimalStr: string): boolean {
  return Number(decimalStr) === 0;
}
