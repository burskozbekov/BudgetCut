# Self-hosting BudgetCut (quickstart)

> Budgets are highly confidential. BudgetCut is designed so a production can run
> the sync server on its own infrastructure — budget data never has to leave the
> production's network. KVKK/GDPR-friendly.

## Run it

```sh
# 1. (optional) set a signing secret
export BUDGETCUT_JWT_SECRET="$(openssl rand -hex 32)"

# 2. bring up the sync server (+ a Postgres ready for the durable op-log)
docker compose up --build
```

The server listens on **http://localhost:8787**. Quick smoke test:

```sh
curl -s localhost:8787/health                                   # -> ok
TOK=$(curl -s localhost:8787/auth/register -H 'content-type: application/json' \
  -d '{"email":"me@prod.tr","password":"hunter2"}' | python3 -c 'import sys,json;print(json.load(sys.stdin)["token"])')
curl -s localhost:8787/budgets -H "authorization: Bearer $TOK" \
  -H 'content-type: application/json' -d '{"name":"Dizi","seed_template":true}'
```

Without Docker: `cargo run -p budgetcut-server` (binds `BUDGETCUT_BIND`, default `127.0.0.1:8787`).

## What runs today vs. next

- **Today:** auth (argon2id + JWT), server-enforced RBAC (§9), an append-only
  op-log applied through the shared `budgetcut-core` reducer, snapshot /
  op-submit endpoints, and a WebSocket op stream — all with **in-memory** state,
  so it runs with no database.
- **Next (server iteration):** persist the op-log + materialized state to the
  bundled **Postgres** (`db` service, `DATABASE_URL`), refresh-token rotation,
  and at-rest encryption.

## Security baseline (§15)

TLS terminates at your reverse proxy. argon2id password hashing; JWT access
tokens; all RBAC enforced server-side; the server never sends out-of-scope
financial data to department-scoped clients (it filters the op stream and
snapshot). Set a strong `BUDGETCUT_JWT_SECRET`. Rate-limiting and refresh-token
reuse detection land with the Postgres backend.
