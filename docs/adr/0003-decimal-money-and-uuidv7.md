# ADR 0003 — Decimal money and UUIDv7 ids

**Status:** Accepted · **Date:** 2026-06-30

## Money: `rust_decimal::Decimal`, never `f64`

Budget totals must be exact and reproducible across platforms. Binary floating point is neither
(`0.1 + 0.2 != 0.3`; results can differ by platform). We use `rust_decimal::Decimal` everywhere.

- Standard display/storage precision is **2 dp (kuruş)**, half-up, matching Turkish production
  accounting.
- Intermediate calculations keep **full precision** — nothing is rounded until a caller asks for
  a display value — so chained operations (e.g. a gross-up division followed by a rollup sum)
  don't accumulate rounding error.
- **Precision (re-association).** `Decimal` carries ~28 significant digits. When a running sum
  exceeds that it rounds, so `Σ(aᵢ + bᵢ)` can differ from `Σ aᵢ + Σ bᵢ` in the last digit. The
  engine therefore (a) iterates in a canonical `(position, id)` order before every sum, and (b)
  derives each rollup `total` as `subtotal + fringe_total` rather than independently accumulating
  per-line totals, so `total == subtotal + fringe` holds exactly. Both are covered by property tests.
- **Magnitude (overflow).** `rust_decimal`'s `+ - * /` **panic** on overflow (values beyond
  ~7.9e28). Since `evaluate` must be a total function that degrades bad cells to `#ERR` (§6),
  per-line arithmetic (formulas, the line product, fringes) uses *checked* ops that surface an
  `Overflow` error pinned to the cell; aggregate sums use *saturating* ops so a pathological total
  clamps rather than aborting the whole calc. Covered by an overflow → `#ERR` test.

## IDs: UUIDv7

All entities use UUIDv7: time-ordered (so they sort naturally in the op log and make good keys),
collision-free across offline clients, and strongly typed via per-entity newtypes (`DetailId`,
`AccountId`, …) so the compiler prevents id mix-ups.
