# ADR 0001 — One shared `budgetcut-core` crate (the keystone)

**Status:** Accepted · **Date:** 2026-06-30

## Context

A budget computed offline on a laptop, recomputed authoritatively on the server, and shown
on a collaborator's machine must yield **identical numbers**. If the client recomputed totals
in TypeScript and the server in Rust, the two would inevitably drift (rounding, fringe edge
cases, evaluation order).

## Decision

The domain model, calculation engine, mutation-op types, and the LWW reducer live in a single
Rust crate, **`budgetcut-core`**, compiled into *both* the Tauri client and the Axum server.

- The crate has **zero I/O and zero platform dependencies**, so it also compiles to WASM/mobile.
- The frontend never re-implements calculation logic. It reads computed numbers from the local
  DB (already computed by the core) or via a Tauri command that runs the core. **No business
  math in TypeScript.**
- Both client (optimistic) and server (authoritative) call the *same* `Document::apply` and the
  *same* `evaluate`.

## Consequences

- Numbers cannot diverge by construction; correctness is tested once, in one place.
- The crate is the natural home for the property and golden-file tests that gate everything else
  ("nothing proceeds until the engine is correct and convergent").
- Adds a Rust↔TS boundary (Tauri commands / serialized snapshots), which is acceptable and
  already part of the locked stack.
