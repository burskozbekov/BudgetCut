# ADR 0002 — Server-authoritative, per-field LWW sync (not CRDT)

**Status:** Accepted · **Date:** 2026-06-30

## Context

We need real-time multi-user editing with offline support, *and* strict role-based access
control (including department-scoped users who must never receive out-of-scope financial data).
Budget data is structured and departmentally partitioned; same-cell conflicts are rare.

## Decision

- **Server-authoritative op log.** Every change is an `Op` carrying a hybrid-logical-clock
  (HLC) timestamp and author id. The server validates each op against RBAC, assigns/keeps the
  authoritative HLC, persists it to an append-only log, and broadcasts it to permitted subscribers.
- **Per-field Last-Write-Wins**, keyed by HLC, applied by the *same* reducer on client and server.
  The op with the higher HLC wins a contested field. We explicitly **do not** use Yjs/Automerge.
- **Offline-first.** The client applies ops optimistically, queues them in an outbox, and replays
  on reconnect; the server may reject queued ops if permissions changed.

## Why LWW is sufficient here

RBAC already forces server-side write validation, so a server is in the loop regardless — the
main thing CRDTs buy (serverless P2P merge) isn't our model. For structured, partitioned budget
data with rare same-cell contention, per-field LWW + presence indicators is simpler and adequate.

## Convergence

For a given *set* of ops, the materialized `Budget` is independent of application order. The
reducer guarantees this with per-field registers (a write applies only if its HLC strictly
exceeds the stored one), existence registers shared by insert/remove, and a pending buffer for
field ops that arrive before their entity's insert. This is enforced by property tests.

## Fallback

If we ever decide not to maintain our own sync server, **PowerSync** is the documented fallback.
Default remains: build our own Rust sync server (self-hostable; AGPL).
