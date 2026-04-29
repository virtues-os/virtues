# Virtues — Actions System One-Pager

> Print-friendly architectural reference. Updated 2026-04-28.

## The Paradigm in One Paragraph

Everything that *does work* in Virtues — syncs, transforms, agents, EOD jobs — is an **Action**. An Action is a row in `app_actions` that points at a Rust binary (`function_name`) and/or an LLM instruction (`agent`). The runner spawns the binary as a subprocess with a JSON stdin/stdout contract, optionally feeds the result into the agent phase, and records the run in `app_action_runs`. Secrets for any provider live in a single unified `credentials` Vault, encrypted with AES-256-GCM. Adding a new OAuth provider = **1 source row + 1 proxy route + 1 sync binary**. No new auth Rust.

---

## System Map

```mermaid
flowchart LR
    subgraph Triggers
      T1[Cron tick]
      T2[Webhook POST]
      T3[Manual /run]
      T4[LLM tool call]
    end

    T1 & T2 & T3 & T4 --> RUN[core::action_runner<br/>dispatcher]

    RUN --> COND{condition<br/>SQL ok?}
    COND -- no --> SKIP[skipped]
    COND -- yes --> CONC{concurrency<br/>gate}
    CONC --> RUNROW[(app_action_runs<br/>status=running)]

    RUNROW --> CRED[Load + decrypt<br/>credential]
    CRED --> SUB[Spawn binary<br/>actions/src/bin/X]
    SUB -->|stdin JSON| BIN[[Rust subprocess<br/>uses virtues-helpers]]
    BIN -->|stdout JSON| SUB
    SUB --> AGENT{agent set?}
    AGENT -- yes --> LLM[LLM loop]
    AGENT -- no --> DONE
    LLM --> DONE[(run row<br/>success/error)]

    BIN <--> DB[(SQLite<br/>data_*, wiki_*)]
    CRED <--> VAULT[(credentials<br/>AES-256-GCM)]
```

---

## Core Tables

```mermaid
erDiagram
    credentials ||--o{ app_actions : "credential_id"
    app_actions ||--o{ app_action_runs : "action_id"
    app_action_runs ||--o{ app_action_runs : "parent_run_id"
    credentials ||--o{ app_mcp_servers : "credential_id"

    credentials {
      TEXT id PK
      TEXT source_id "ios|google|plaid|mcp:*"
      TEXT status "pending|active|revoked|reauth_required|error"
      TEXT secrets_ciphertext "encrypted JSON"
      TEXT secret_lookup_hash "HMAC, webhook O(1)"
      TEXT scopes
      TEXT expires_at
      TEXT next_refresh_at "sweeper scans"
      TEXT metadata "plaintext context"
    }

    app_actions {
      TEXT id PK
      TEXT name
      TEXT owner "system|user"
      TEXT function_name "binary in actions/src/bin"
      TEXT agent "LLM instruction"
      TEXT triggers "JSON: cron|webhook|manual|tool"
      TEXT cron_schedule
      TEXT condition "SQL"
      TEXT config "user+code state"
      TEXT memory "agent memory"
      TEXT credential_id FK
      INT enabled
    }

    app_action_runs {
      TEXT id PK
      TEXT action_id FK
      TEXT status "running|success|error|skipped|cancelled"
      TEXT trigger
      TEXT started_at
      TEXT completed_at
      INT records_processed
      TEXT result_summary "stdout, ≤8KB"
      TEXT error "stderr, ≤4KB"
      TEXT parent_run_id
    }
```

**Ontology side** (separate from actions, written *by* actions):
`wiki_people`, `wiki_places`, `wiki_orgs`, plus `data_health_*`, `data_location_*`, `data_finance_*` — all keyed by `source_connection_id` and timestamp.

---

## Three Auth Kinds (the whole story)

| Kind | Used by | Flow | Secret shape |
|---|---|---|---|
| `self_issued_bearer` | iOS, Mac | Server mints token → QR → device stores → device sends on each webhook | `{token}` + `lookup_hash` for O(1) match |
| `via_proxy` | Google, Plaid, OAuth providers | Browser → proxy (holds client_secret) → exchange token → core finalizes | `{access_token, refresh_token, expires_at}` |
| `api_key` | MCP servers, LLM keys | User pastes string → encrypt + store | `{token}` or multi-field |

**Status machine:** `pending → active ⇄ reauth_required/error → revoked` (terminal).

---

## Action Lifecycle (Subprocess Contract)

```mermaid
sequenceDiagram
    participant R as Runner (core)
    participant V as Vault
    participant B as Binary subprocess
    participant DB as SQLite

    R->>R: cron/webhook fires
    R->>DB: insert app_action_runs (running)
    R->>V: fetch + decrypt credential
    V-->>R: plaintext secrets
    R->>B: spawn, stdin = {config, credentials, payload}
    B->>DB: read/write data_* / wiki_*
    B-->>R: stdout = {result, config}
    R->>DB: persist updated config
    opt agent set
      R->>R: LLM loop with result
    end
    R->>DB: update run row (success|error)
```

**Exit 0 = success.** Anything else = error, captured as `error` (≤4KB).

---

## Layout on Disk

```
actions/                        → 11 binary targets
  src/bin/{name}/main.rs        → ios_healthkit, google_calendar_sync, …
  templates.toml                → [[source]] + [[action]] declarations
  helpers/ (virtues-helpers)    → contract, auth/, crypto/, db, entity, dedup

core/
  src/action_runner/mod.rs      → unified dispatcher
  src/scheduler/actions.rs      → Action / ActionRun models + CRUD
  src/scheduler/mod.rs          → cron ticker
  src/server/webhook.rs         → POST /webhook/{action_id}
  src/api/auth.rs               → 5 handlers: pair_initiate/complete,
                                  oauth_start/callback, apikey_complete
  migrations/
    002_entities.sql            → wiki_people / wiki_places / wiki_orgs
    003_ontology.sql            → data_* tables
    050_actions_cutover.sql     → app_actions + app_action_runs
    055_credentials_create.sql  → unified credentials vault

ACTIONS.md                      → 707-line full spec (source of truth)
```

---

## Templates.toml — User vs Code Ownership

| Field | Owner | On reconcile |
|---|---|---|
| `cron_schedule`, `config`, `enabled`, `memory` | **User** | Preserved |
| `function_name`, `triggers`, `agent`, `condition`, `owner`, `name` | **Code** | Overwritten |

`per_credential = true` ⇒ fan out one `app_actions` row per active credential of the linked source.

---

## The Forcing Function

> *"The next 10 redirect-based providers should require only one `[[source]]` row + one proxy route + one sync binary. If any requires more, the architecture has drifted."* — ACTIONS.md

If you're writing new auth code to add a provider, stop and re-read the auth/ helpers.

---

## Quick Reference

- **Encryption key:** `VIRTUES_ENCRYPTION_KEY` (32 bytes, base64)
- **Algorithm:** AES-256-GCM, 12-byte nonce per write
- **HMAC pepper domains:** `credentials.lookup.v1`, `oauth.state.v1`
- **Webhook auth:** `Bearer <token>` → HMAC → `secret_lookup_hash` → O(1) credential lookup → assert matches `action.credential_id`
- **Truncation:** `result_summary ≤ 8KB`, `error ≤ 4KB`
- **Concurrency default:** skip if previous run still active (override with `concurrency_mode='parallel'`)
