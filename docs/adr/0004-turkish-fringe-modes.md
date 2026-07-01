# ADR 0004 — Fringes have a *mode* (Additive vs Gross-Up) and per-line rate overrides

**Status:** Accepted · **Date:** 2026-06-30

## Context

Movie Magic models a fringe as an additive percentage (or flat amount) of a base. Analysis of a
**real Turkish dizi episode budget** (`BOŞ BÜTÇE.xlsx`) showed this is insufficient for Turkish
production payroll, which combines two fundamentally different fringe behaviours per line:

- **Stopaj** (income-tax withholding) is a **gross-up**: the talent is paid a net, and the budget
  must carry the grossed-up cost. `brüt = net / (1 − r)`, so `stopaj = brüt − net`.
- **SGK / agency commission** is **additive**: `kom = net × r`, added on top.

`G.TOPLAM = brüt + kom`. Verified against the workbook (e.g. director net ₺660.000 @ 17% →
₺795.180,72; bodyguards 6 × ₺6.480 @ 25%/20% → ₺59.616). The per-line `VERGİ ORANI` / `KOM. ORANI`
columns vary row to row (0.17, 0.20, 0.25, 0.38…).

## Decision

1. `Fringe` carries a **`mode`**: `Additive` (`base × r`) or `GrossUp` (`base / (1 − r) − base`),
   in addition to its `kind` (`Percent` / `Flat`) and `posting_level`.
2. A fringe *application* (`AppliedFringe`) may carry a **`rate_override`**, so one reusable
   "Stopaj" tool serves every line at that line's rate — mirroring the spreadsheet columns.
3. The Turkish presets (Stopaj as gross-up; SGK İşveren, İşsizlik, Komisyon as additive) ship in
   the Library by default. The Netflix CoA (TR+EN, ATL/BTL) ships as the dizi template skeleton.

## Consequences

- The engine is a strict superset of MMB's fringe model (additive is just one mode).
- The golden-file test reproduces real episode figures to the kuruş, so any regression in the
  payroll math fails CI.
- Gross-up guards against `r ≥ 1` (returns an evaluation error → `#ERR` rather than dividing by
  a non-positive denominator).
