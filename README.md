# BudgetCut

Cross-platform, offline-first, real-time collaborative **film & TV production budgeting**.
Turkish-first. Privacy-first / self-hostable. macOS desktop first → Windows/Linux → mobile companion.

BudgetCut combines the **calculation depth of Movie Magic Budgeting** with the
**real-time collaboration of Saturation.io**, plus two things neither offers together:
true **offline-first** operation and a **self-hostable, privacy-first** backend.

## Architecture (locked)

The single most important rule: **the calculation engine and sync op types live in ONE
shared Rust crate (`budgetcut-core`) compiled into both the client and the server**, so the
numbers can never diverge.

| Layer | Choice |
|---|---|
| Shared domain + calc + sync types | **`budgetcut-core`** (Rust, zero I/O, WASM-ready) |
| Client shell | Tauri 2 + React/TS/Vite/Tailwind |
| Local store | SQLite (cache; server is canonical) |
| Sync | Server-authoritative op-log, **per-field LWW** over HLC timestamps (no CRDT) |
| Server | Rust + Axum + tokio + Postgres + WebSocket |
| Money | `rust_decimal` fixed-point — never `f64` |
| IDs | UUIDv7 |

See [`docs/adr/`](docs/adr) for the rationale behind each decision.

## Repository layout

```
budgetcut/
├─ crates/
│  ├─ budgetcut-core/        # ✅ Domain model, calc engine, op-log + LWW reducer, seeds
│  ├─ budgetcut-store/       # ✅ Offline-first SQLite op-log + replay (desktop persistence)
│  ├─ budgetcut-server/      # ✅ Axum sync server: auth, RBAC, op-log, WS fan-out
│  ├─ budgetcut-export/      # ✅ XLSX + CSV reports from computed state
│  └─ budgetcut-importers/   # ✅ Generic/AICP CSV import (MMB XML/Excel = Phase 2)
├─ apps/
│  └─ desktop/               # ✅ Tauri 2 + React/TS app (offline MVP) + src-tauri commands
├─ Dockerfile, docker-compose.yml   # self-host: `docker compose up`
└─ docs/
   ├─ adr/                   # Architecture Decision Records
   └─ self-host.md           # Self-host quickstart
```

## Try it (no toolchain needed)

Non-developers: grab the packaged macOS app (`BudgetCut_*.dmg`), drag it to Applications, then
**right-click → Open** the first time (unsigned). It opens a real Turkish dizi budget you can
edit offline — see [TRYIT.md](TRYIT.md).

## Run it (from source)

```sh
cargo run -p budgetcut-core  --example topsheet      # engine: topsheet + live recalc + convergence
cargo run -p budgetcut-store --example offline       # offline-first: edit → persist → reopen from disk
cargo run -p budgetcut-export --example gen_report -- /tmp   # write an XLSX + CSV topsheet
cargo run -p budgetcut-server                        # sync server on http://127.0.0.1:8787
cd apps/desktop && npm install && npm run tauri dev  # the desktop app (native window)
```

## Status

**Phase 1 desktop MVP: all roadmap steps (§17) built, runnable, and tested (69 tests, clippy `-D warnings` clean).**

- **Core** — model, deterministic calc engine, op-log + HLC + per-field LWW reducer, Netflix CoA + Turkish presets, golden + property tests.
- **Store** — offline-first SQLite (op-log as truth, replay on open, outbox); round-trip tested.
- **Desktop** — Tauri 2 + React/TS/Vite UI (Turkish, i18n, Zustand), Topsheet + editable Account-Details grid; `src-tauri` commands over the store; compiles, launches, persists.
- **Server** — Axum + WebSocket, argon2id + JWT, server-enforced RBAC (viewer-cannot-write & department-filtered reads proven), op-log applied through the shared core reducer; sync round-trip converges two clients.
- **Export** — XLSX + CSV matching on-screen totals. **Import** — generic/AICP CSV with total-validation (MMB = Phase 2). **Self-host** — `docker compose up`.

<details><summary>Original Milestone 1 notes</summary>

**Milestone 1 — `budgetcut-core` (the keystone): complete.**

- Full domain model (Budget → Category → Account → Detail, Production Totals, Charges,
  Credits) + Setup Tools (Fringes, Globals, Units, Groups, Locations, Sets, Currencies).
- Deterministic calc engine: safe expression evaluator, global dependency graph with
  cycle detection (`#ERR`), **dual-mode fringes** (additive *and* Turkish gross-up),
  cutoff/cap, posting levels, multi-currency, rollups → ATL/BTL → Grand → −Credits → Net.
- Op-log with hybrid logical clocks and an idempotent **per-field LWW reducer** that
  converges under arbitrary op ordering.
- Seeded **Netflix global Chart of Accounts** (TR+EN, ATL/BTL) and **Turkish fringe
  presets** (Stopaj/SGK/Komisyon) as a ready-to-clone dizi template.
- Tests: golden-file tests validated against a **real Turkish dizi episode budget**,
  property tests for reducer convergence/idempotency and calc determinism, and a 5,000-line
  recalc benchmark (**~3.5 ms in release**, target <50 ms).

</details>

**Phase 2+ (next):** Postgres-backed durable op-log + refresh-token rotation; presence
avatars; MMB `.mbb`/XML import; full PDF report suite; production scheduling; actuals/EFC;
mobile companion.

## Develop

Requires a recent stable Rust toolchain.

```sh
cargo test --workspace          # run all tests
cargo clippy --workspace --all-targets
cargo fmt --all --check
cargo test --release -p budgetcut-core --test perf_recalc -- --nocapture   # see recalc timing
```

The frontend/Tauri workspace uses pnpm (`corepack enable` to get it). The desktop app is
wired up in the next milestone — see [`apps/desktop`](apps/desktop).

## License

AGPL-3.0-or-later (self-hostable; see ADR 0002).
