# @budgetcut/desktop — Tauri 2 desktop app

The macOS-first desktop shell (Phase 1, build order §17 step 3). **Next milestone** —
not yet implemented; this directory is a placeholder so the monorepo layout matches the brief.

When built, it will be:

```
apps/desktop/
├─ package.json            # React 18 + TypeScript + Vite + Tailwind
├─ index.html
├─ src/                    # React frontend
│  ├─ grid/                # TanStack Table + TanStack Virtual budget grid (the product)
│  ├─ views/               # Topsheet, Account Details, Setup Tools, Apply Tools, Versions
│  ├─ i18n/                # react-i18next — Turkish default (tr-TR), English a config flip
│  └─ store/               # Zustand/Jotai — derived numbers come from the Rust core only
└─ src-tauri/              # Rust: Tauri commands, SQLite (WAL), embeds budgetcut-core
   ├─ Cargo.toml           # depends on ../../crates/budgetcut-core
   ├─ tauri.conf.json      # minimal capability set (§15)
   └─ migrations/          # sqlx migrations (canonical tables, outbox, HLC watermark)
```

## Design principles (carried from the brief)

- **No business math in TypeScript.** The UI reads computed numbers from the local SQLite
  cache (written by the Rust core) or via a Tauri command that runs `budgetcut-core`.
- **The data grid is the product** — thousands of rows must scroll and edit at 60 fps;
  keyboard-first (insert line, subtotal, type-ahead for globals/units).
- **Turkish-first** — every string externalized via i18n from line one; `1.234,56` / `₺`.
- Local SQLite is a **cache**; the server is canonical. Outbox queues offline ops.

Get pnpm with `corepack enable`, then (once scaffolded) `pnpm install` and `pnpm desktop:dev`.
