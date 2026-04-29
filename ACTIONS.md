# Actions, Sources, and the Vault

> An action is a function. Like a Lambda — it runs when triggered, reads input, writes output, exits. All actions are compiled Rust binaries spawned as subprocesses by the runner.
> A source is a row in `actions/templates.toml` declaring a provider (iPhone, Google, Plaid, an MCP server, your custom IoT device) — its display metadata and how it authenticates.
> A credential is the encrypted secret a user holds for a source. The user calls them "passwords." All credentials live in one Vault — the `credentials` table — encrypted at rest.
> The subprocess paradigm exists so you can ship your own. Drop a binary in `actions/src/bin/{name}/`, register a template, restart, done.

This is the strategic charter. Single source of truth.

---

## The mental model in five lines

1. **An action is a Rust binary** at `actions/src/bin/{name}/`. The runner spawns it as a subprocess with `{config, credentials, payload}` JSON on stdin and reads `{result, config}` JSON from stdout.
2. **Triggers fire actions.** `cron`, `webhook`, `manual`, `tool`. Same subprocess contract — the trigger shapes which fields of the input envelope are populated.
3. **Templates** in `actions/templates.toml` declare what exists. Two top-level kinds: `[[source]]` (catalog tile + auth declaration) and `[[action]]` (runnable behavior). Reconciled into `app_actions` rows at startup.
4. **Per-credential fan-out**: when a template references a source, the runner creates one action row per active credential of that source. iOS pairing → 6 action rows (HealthKit, Location, EventKit, FinanceKit, Contacts, Microphone), each with its own `credential_id`.
5. **Auth flows are core HTTP handlers, not actions.** OAuth callback, pair-initiate/complete, api-key paste live in `core/src/api/auth.rs` as thin axum routes calling the **auth helpers** crate. The actions table stays clean — only user-facing behaviors live there.

---

## Why subprocess (the actual reason)

A subprocess is **user-shippable behavior**. Drop a binary, restart, it runs. No core recompile, no PR upstream, no waiting on us. That's the entire point. Everything that *can* be a subprocess *is* one — so you own it.

Core holds only what must exist before any subprocess can run:

- **The runner** (spawns subprocesses, manages runs)
- **HTTP server + webhook router** (dispatches before any action exists)
- **Scheduler** (fires cron triggers)
- **Auth handlers** (5 thin axum routes that drive the source catalog connect flows; they call the helpers crate)
- **DB connection setup, encryption key loading**

Everything else — sync logic, transforms, ingest, agent loops, the cron-driven `credential_refresh` sweeper — is a subprocess. **You can swap, fork, or replace any of it without touching core.**

This is not an isolation boundary. The subprocess inherits the master key via env, talks to the DB directly, has full filesystem access. It's your code on your box; the trust boundary is *you*, not the process. We use subprocess for **shippability**, not security.

There are two other runtimes in the system that are deliberately NOT actions:

| Runtime | Why separate |
|---|---|
| **Python sandbox** (Docker-per-request, `core/src/tools/code_interpreter/`) | LLM-generated, untrusted; isolated container, no network, bounded resources. A *tool* the agent calls, not an action. |
| **OAuth proxy** (`apps/oauth-proxy`, Node/TS) | Holds third-party OAuth client_id/secret. Runs at our domain, not the user's box. |

---

## Schema

### `app_actions`

```sql
app_actions (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    owner           TEXT NOT NULL DEFAULT 'user'      -- 'system' | 'user'
                        CHECK (owner IN ('system', 'user')),

    function_name   TEXT,                             -- subprocess binary name
    agent           TEXT,                             -- LLM prompt (runs after subprocess, if both)
    triggers        TEXT NOT NULL DEFAULT '["cron"]', -- JSON array
    cron_schedule   TEXT,                             -- "*/15 * * * *" — required when 'cron' in triggers
    condition       TEXT,                             -- SQL expression: skip run if falsy

    enabled         INTEGER NOT NULL DEFAULT 1,
    config          TEXT NOT NULL DEFAULT '{}',       -- settings + code-managed state (cursors)
    memory          TEXT,                             -- agent memory blob, persisted across runs
    credential_id   TEXT REFERENCES credentials(id),  -- per-credential fan-out target

    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
    CHECK (function_name IS NOT NULL OR agent IS NOT NULL)
);
```

**`config`** holds both user-facing settings (`calendar_ids`, `webhook_url`) and code-managed state (`sync_token`, `last_seen_id`) in one JSON blob. The action returns an updated `config` from stdout; the runner saves it back. That's how cursors persist.

**`CHECK (function_name OR agent)`** invariant: every action has *something* to run.

### `app_action_runs`

```sql
app_action_runs (
    id                TEXT PRIMARY KEY,
    action_id         TEXT REFERENCES app_actions(id),
    status            TEXT NOT NULL,                  -- 'running' | 'success' | 'error' | 'cancelled' | 'skipped'
    trigger           TEXT NOT NULL,                  -- 'cron' | 'manual' | 'tool' | 'webhook'
    started_at        TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at      TEXT,
    records_processed INTEGER NOT NULL DEFAULT 0,
    error             TEXT,
    parent_run_id     TEXT REFERENCES app_action_runs(id),
    result_summary    TEXT,
    created_at        TEXT NOT NULL DEFAULT (datetime('now'))
);
```

### `credentials` — the Vault

```sql
CREATE TABLE credentials (
    id                  TEXT PRIMARY KEY,

    source_id           TEXT NOT NULL,                          -- 'ios', 'google', 'plaid', 'mcp:github', 'user:bank_chase', ...
    name                TEXT NOT NULL,                          -- user-facing label (e.g. "adam@jaces.com")

    status              TEXT NOT NULL                           -- lifecycle state
                            CHECK (status IN ('pending', 'active', 'revoked', 'reauth_required', 'error')),
    status_reason       TEXT,                                   -- 'user_revoked', 'token_expired', 'item_login_required', ...

    secrets_ciphertext  TEXT NOT NULL,                          -- encrypted JSON; shape per auth kind
    secret_lookup_hash  TEXT,                                   -- HMAC; non-null only for self-issued bearers

    scopes              TEXT,                                   -- JSON array; nullable; OAuth-only
    expires_at          TEXT,                                   -- nullable
    next_refresh_at     TEXT,                                   -- nullable; cron sweeps WHERE this < now()

    metadata            TEXT NOT NULL DEFAULT '{}',             -- plaintext non-secret context

    last_seen_at        TEXT,
    created_at          TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at          TEXT NOT NULL DEFAULT (datetime('now'))
);
```

**`secrets_ciphertext`** is encrypted JSON. The shape is whatever the auth flow stored:

| Auth kind | Decrypted JSON shape |
|---|---|
| `self_issued_bearer` | `{ "token": "..." }` |
| `via_proxy` (OAuth) | `{ "access_token": "...", "refresh_token": "...", "expires_at": "..." }` |
| `via_proxy` (Plaid) | `{ "access_token": "..." }` (no expiry/refresh; `metadata.item_id` carries the Plaid Item) |
| `api_key` | `{ "token": "..." }` (or `{ "field_a": "...", "field_b": "..." }` for multi-field) |

The connector author doesn't pick this shape — the proxy does (for `via_proxy`), the pairing flow does (for `self_issued_bearer`), or the `fields` declaration does (for `api_key`).

---

## The three power levels

| Level | function_name | agent | Example |
|---|:---:|:---:|---|
| **Function-only** | yes | — | iOS HealthKit ingest, embedding indexer, credential_refresh |
| **Function + agent** | yes | yes | Day-end summary: resolve sleep (function), then write autobiography (LLM) |
| **Agent-only** | — | yes | "Summarize yesterday's spending by category" |

One schema, three power levels. A function-only action gains LLM augmentation by adding `agent`. No schema change, no new action type.

---

## Triggers

| Trigger | How it fires | Use |
|---|---|---|
| `cron` | Scheduler ticks; checks `cron_schedule` | Periodic sync, refresh, housekeeping |
| `webhook` | Device POSTs to `/webhook/{action_id}` with HMAC bearer | iOS device pushes |
| `manual` | User clicks "Run" in UI | One-shot debugging, agent prompts |
| `tool` | LLM agent calls `run_action` | Composable agent toolchains |

**`webhook` actions must have a `credential_id`** — the bearer token resolves to one credential, and the runner enforces that the credential matches the action's `credential_id`. Anti-spoofing.

**Auth flows are NOT triggers.** OAuth callback, pair-initiate, etc. are core HTTP handlers in `core/src/api/auth.rs` calling the auth helpers crate. They never spawn a subprocess.

---

## Sources (the catalog)

A `[[source]]` row in `actions/templates.toml` declares a provider — display metadata + auth declaration. No Rust const, no DSL, no auto-discovery.

```toml
[[source]]
id = "ios"
display_name = "iPhone"
icon = "ri:smartphone-line"
description = "Pair an iPhone to ingest HealthKit, Location, EventKit, FinanceKit, Contacts, Microphone."
auth = { kind = "self_issued_bearer" }

[[source]]
id = "google"
display_name = "Google"
icon = "ri:google-fill"
description = "Calendar, Mail, Drive."
auth = { kind = "via_proxy", start_path = "/google/start" }

[[source]]
id = "plaid"
display_name = "Plaid"
icon = "ri:bank-card-line"
description = "Connect bank accounts via Plaid Hosted Link."
auth = { kind = "via_proxy", start_path = "/plaid/start" }

[[source]]
id = "mcp:github"
display_name = "GitHub MCP"
icon = "ri:github-fill"
description = "Personal access token for the GitHub MCP server."
auth = { kind = "api_key", fields = ["token"] }
```

Adding a source = one `[[source]]` row + (for `via_proxy`) one route in the proxy + the corresponding sync action binaries. **Zero new auth Rust per provider.**

### Three auth kinds

| Kind | Used by | Connect flow |
|---|---|---|
| `self_issued_bearer` | iOS, future Mac, custom paired IoT | Server mints bearer token, device stores it, device sends as `Authorization: Bearer <token>` on webhooks |
| `via_proxy` | OAuth providers (Google, Notion, Spotify, Strava…), Plaid Hosted Link, future Stripe Connect | Browser redirects through `apps/oauth-proxy`, which holds all third-party client_id/secret. Self-hosted Virtues never holds a third-party secret. |
| `api_key` | MCP servers, future BYO LLM keys, ad-hoc | User pastes a string; core encrypts and stores |

These three cover every provider on the roadmap. Adding a fourth requires a real provider that doesn't fit, a new helper, and a charter event.

---

## How each auth flow works

All five auth flows are **core axum handlers** in `core/src/api/auth.rs`, each ~30–50 lines, calling the **`virtues-auth` helpers crate**.

### `self_issued_bearer` — pair-initiate / pair-complete

```
1. Browser POST /api/pairing/initiate body={source_id: "ios", name: "My iPhone"}
2. Handler validates source.auth.kind == self_issued_bearer
   calls auth::mint_pending_credential(db, source_id, name)
   returns { credential_id, qr_payload }
3. iOS app scans QR with credential_id
4. iOS app POST /api/pairing/complete/:credential_id body={device_id, device_info, token}
5. Handler calls auth::finalize_self_issued_bearer(db, credential_id, token, metadata)
   — encrypts token, computes HMAC for secret_lookup_hash, sets status='active'
   then triggers reconcile_templates (fans out per-credential action rows)
   then auth::fanout_action_ids → returns { action_ids: { ios_healthkit, ios_location, ... } }
6. iOS authenticates webhook posts via Authorization: Bearer <token> against secret_lookup_hash (O(1))
```

### `via_proxy` — OAuth and Plaid

```
1. Browser POST /api/connect/google/start
2. Handler validates source.auth.kind == via_proxy
   calls auth::sign_oauth_state(source_id, existing_credential_id?)
   builds redirect URL: "{proxy}/google/start?return_url=...&state=<signed_token>"
   returns { redirect_url }
3. Browser redirects through proxy → Google → user consents → Google redirects to proxy
4. Proxy exchanges code for tokens, mints one-time exchange_token, redirects browser back
5. Browser GET /oauth/callback?state=...&exchange_token=...
6. Handler:
   - calls auth::verify_oauth_state(state) → claims (validates HMAC + expiry)
   - calls auth::proxy_exchange(source_id, exchange_token) → { secrets, metadata, expires_in }
   - calls auth::finalize_credential (mint pending if no existing_credential_id, then UPDATE WHERE status='pending')
   - triggers reconcile_templates
   - 302 → /sources?connected={source_id}
```

**Plaid uses the same code path.** The proxy abstracts that Plaid uses Hosted Link instead of OAuth, that there's no refresh, that `item_id` lives in `metadata`. Self-hosted Virtues sees an opaque `{secrets, metadata, expires_in}` and stores it.

### `api_key` — paste-a-string

```
1. Frontend renders form from source.auth.fields
2. Browser POST /api/connect/mcp:github/complete body={name, fields: {token: "..."}}
3. Handler calls auth::finalize_apikey_credential(db, source_id, name, fields)
   — encrypts fields as secrets_ciphertext, mints credential row, status='active'
   triggers reconcile_templates
   returns { credential_id }
```

### `credential_refresh` — the one auth-related action

`via_proxy` credentials with `expires_at` need periodic refresh. This is the only auth-related thing that runs on a schedule, so it's a subprocess action like any other cron job:

```
actions/src/bin/credential_refresh/main.rs

scan: credentials WHERE next_refresh_at < now() AND status = 'active'
for each credential:
    auth::proxy_refresh(source_id, refresh_token) → fresh tokens
    auth::finalize_credential(...)               → write back
```

`[[action]]` row in `templates.toml`, `triggers = ["cron", "manual"]`, `default_cron = "0 */15 * * * *"`.

---

## OAuth CSRF state — signed token, no table

The state parameter is a self-contained signed token:

```
<base64url(json({source_id, existing_credential_id?, expires_at, nonce}))>.<hex(hmac_sha256)>
```

The HMAC key is a pepper derived from `VIRTUES_ENCRYPTION_KEY` with the domain separator `oauth.state.v1`. The `oauth_callback` handler verifies the signature in constant time and confirms `expires_at > now()` (10-minute window).

Why no table:
- The state's whole job is **CSRF defense** — proving "we're the ones who initiated this flow." HMAC signing accomplishes that without persistent state.
- Replay within the 10-minute window is harmless: the state parameter doesn't authenticate anything; the proxy's one-time exchange token does.

Implementation: `crates/virtues-crypto/` (`sign_oauth_state` / `verify_oauth_state`), wrapped by `crates/virtues-auth/`.

---

## The credential status state machine

```
                         ┌──────────┐
       initial create ─► │ pending  │  (handshake in flight)
                         └────┬─────┘
                              │ exchange completes
                              ▼
                         ┌──────────┐
                         │  active  │ ◄────┐
                         └────┬─────┘      │
                              │            │ refresh / reauth succeeds
              ┌───────────────┼────────────┘
              ▼               ▼
   ┌──────────────────┐  ┌──────────┐
   │ reauth_required  │  │  error   │
   └────────┬─────────┘  └────┬─────┘
            │                 │
            ▼                 ▼
                         ┌──────────┐
       user revoke  ───► │ revoked  │   (terminal)
                         └──────────┘
```

The runner refuses to dispatch any action whose credential is not `active`. Per-credential action rows for non-active credentials are gated at dispatch time.

| Status | UI signal |
|---|---|
| `pending` | Spinner / "waiting for device" |
| `active` | Green |
| `reauth_required` | Yellow + "Reconnect" |
| `error` | Red + retry |
| `revoked` | Greyed; reconnect mints a new credential |

---

## Encryption

- **Master key**: `VIRTUES_ENCRYPTION_KEY` env var, 32 bytes base64. Stable for the life of the deployment.
- **AES-256-GCM** with a 12-byte random nonce per encryption (`crates/virtues-crypto/`).
- **HMAC pepper** for `secret_lookup_hash` is derived from the master key with domain separator `credentials.lookup.v1`. No second env var.
- **HMAC pepper** for OAuth state token signing uses domain separator `oauth.state.v1`.
- **What's encrypted**: `secrets_ciphertext` only. `metadata`, `source_id`, `name`, `scopes`, `status` stay plaintext.
- **Self-issued-bearer lookup**: `secret_lookup_hash = HMAC-SHA256(pepper, plaintext_token)`. O(1) lookup at webhook time.
- **Subprocess access**: the master key is inherited by action subprocesses via env, by design. The `credential_refresh` subprocess uses it via `virtues-auth` helpers. **This is not a security boundary** — single-user personal AI; the user owns the box, the env, and the master key.

---

## The runner

```
Trigger fires
  │
  ├── Resolve action by id
  ├── Validate trigger ∈ action.triggers
  ├── Concurrency gate (skip if previous run still active)
  ├── Evaluate condition (SQL expression) — falsy → 'skipped', stop
  ├── Create app_action_runs row with status='running'
  │
  ├── Load credential (if credential_id set)
  │     decrypt secrets_ciphertext → plaintext JSON
  │     ensure_fresh(credential) — if expires_at near, refresh inline; persist new tokens
  │
  ├── Subprocess phase (if function_name set)
  │     spawn actions/src/bin/{function_name} with stdin = { config, credentials, payload }
  │     read stdout = { result, config }
  │     non-zero exit → 'error', stop
  │     save returned config back to app_actions.config
  │
  ├── Agent phase (if agent set)
  │     LLM loop with subprocess result_summary as context
  │
  └── Mark 'success' with result_summary
```

**Two planes**:
- **Control plane**: stdin/stdout JSON. Small, structured, one-shot.
- **Data plane**: the action talks to the database directly via `DATABASE_URL`. Bulk records flow through SQL, not the pipe.

---

## Webhook routing

Two webhook surfaces, both in core (they dispatch *before* any action subprocess):

### Per-action ingest (self-issued bearer)

`POST /webhook/{action_id}` with `Authorization: Bearer <device_token>`.

```
1. lookup_hash = HMAC(pepper, bearer)
2. credential = SELECT * FROM credentials WHERE secret_lookup_hash = lookup_hash AND status = 'active'
3. action = SELECT * FROM app_actions WHERE id = path.action_id
4. assert action.credential_id == credential.id   ← anti-spoofing
5. run_action(action, "webhook", payload)
```

O(1) lookup. Used by iOS today; will be used by Mac, custom paired devices.

### Provider invalidation (forwarded by proxy)

Plaid's `ITEM_LOGIN_REQUIRED`, Google's token revocation, etc. all hit the **proxy** (it owns the webhook URLs registered with each provider). The proxy verifies the provider's signature, then forwards a normalized event to the user's instance:

```
POST https://my-virtues/webhooks/proxy
  X-Proxy-Signature: <HMAC of body using shared proxy secret>
  Body: { source_id: "plaid", credential_match: { metadata_path: "item_id", value: "..." },
          status: "reauth_required", reason: "item_login_required" }
```

Core verifies the proxy's HMAC, looks up the credential by the indicated metadata path, transitions status. **No per-provider webhook handler.** One generic handler covers every provider.

---

## What an action looks like

### Webhook ingest

```rust
// actions/src/bin/ios_healthkit/main.rs
use virtues_action_helpers::{read_input, output, db};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let input = read_input()?;
    let pool = db::connect_from_env().await?;

    let records = input.payload.as_ref()
        .and_then(|p| p["records"].as_array())
        .ok_or_else(|| anyhow::anyhow!("missing payload.records"))?;

    let count = transform::write_records(&pool, records).await?;
    output(&format!("ingested {count} records"), &input.config)
}
```

### OAuth-backed cron sync

```rust
// actions/src/bin/google_calendar_sync/main.rs
use virtues_action_helpers::{read_input, output, db, http};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut input = read_input()?;
    let pool = db::connect_from_env().await?;

    let token = input.credentials.as_ref()
        .and_then(|c| c["secrets"]["access_token"].as_str())
        .ok_or_else(|| anyhow::anyhow!("missing access_token"))?;

    let sync_token = input.config["sync_token"].as_str();
    let resp: serde_json::Value = http::client()
        .get("https://www.googleapis.com/calendar/v3/calendars/primary/events")
        .bearer_auth(token)
        .query(&[("syncToken", sync_token)])
        .send().await?.error_for_status()?.json().await?;

    let count = transform::write_events(&pool, &resp).await?;
    input.config["sync_token"] = resp["nextSyncToken"].clone();
    output(&format!("synced {count} events"), &input.config)
}
```

A typical sync binary is **~150–300 lines**.

---

## Action templates

Templates declare what `app_actions` rows should exist. Live in `actions/templates.toml` alongside `[[source]]` entries.

```toml
[[action]]
id_prefix     = "embedding_index"
name          = "Embedding Indexer"
owner         = "system"
function_name = "embedding_index"
triggers      = ["cron"]
default_cron  = "0 */15 * * * *"

[[action]]
id_prefix      = "ios_healthkit"
name           = "iOS HealthKit"
owner          = "system"
function_name  = "ios_healthkit"
triggers       = ["webhook"]
per_credential = true
source         = { id = "ios" }

[[action]]
id_prefix      = "google_calendar_sync"
name           = "Google Calendar"
owner          = "system"
function_name  = "google_calendar_sync"
triggers       = ["cron"]
default_cron   = "0 */15 * * * *"
per_credential = true
source         = { id = "google" }

[[action]]
id_prefix     = "credential_refresh"
name          = "Credential Refresh"
owner         = "system"
function_name = "credential_refresh"
triggers      = ["cron", "manual"]
default_cron  = "0 */15 * * * *"
```

**Reconciliation** (on startup, after credential changes, and when an auth handler signals it):
- Templates without `per_credential` → 1 row in `app_actions`. User-editable fields preserved.
- Templates with `per_credential = true` → 1 row per active credential of `source.id`. Rows for inactive credentials are pruned.

User-managed fields (`cron_schedule`, `config`, `enabled`, `memory`) survive reconciliation. Code-managed fields (`function_name`, `triggers`, `agent`, `condition`, `owner`, `name`) are overwritten on every startup so they always match the template.

---

## Helpers

Two crates. `virtues-action-helpers` is for action subprocesses (binaries in `actions/src/bin/`). `virtues-auth` is for auth flows (called by core handlers and by `credential_refresh`).

```
actions/helpers/                          # virtues-action-helpers
  src/
    lib.rs            — read_input, output, ActionInput, ActionOutput
    db.rs             — connect_from_env, batch_upsert, write_records
    http.rs           — retried HTTP client, common headers
    proxy.rs          — POST proxy refresh endpoint, ensure_fresh()
    entity.rs         — resolve_people, cluster_places

crates/virtues-auth/                      # the 12 auth helpers
  src/
    lib.rs            — public API (functions below)
    state.rs          — sign_oauth_state / verify_oauth_state (wraps virtues-crypto)
    proxy.rs          — proxy_exchange / proxy_refresh (HTTP to apps/oauth-proxy)
    vault.rs          — mint_pending_credential, finalize_credential, finalize_self_issued_bearer,
                        finalize_apikey_credential, mark_credential_status, fanout_action_ids
    catalog.rs        — lookup_source / list_sources_sorted / source_auth_kind
    error.rs          — AuthError enum with http_status() method

crates/virtues-crypto/                    # primitives only — no sqlx, no DB
  src/
    lib.rs            — TokenEncryptor (AES-256-GCM + HMAC), OauthStateClaims
                        derive_pepper, sign/verify_oauth_state, lookup_hash
```

If a pattern repeats across two binaries, **lift it to helpers**. No DSL, no manifest, no primitive registry.

---

## Folder structure

```
crates/                                    # Workspace shared crates
  virtues-crypto/                          # AES-GCM, HMAC, OAuth state primitives
  virtues-action-contract/                 # Shared ActionInput / ActionOutput types
  virtues-auth/                            # The 12 auth helpers (called by core + credential_refresh)

core/                                      # Must-exist-before-actions
  src/
    action_runner/                         # spawn subprocess, manage runs, decrypt creds
    action_templates/                      # parse templates.toml [[source]] + [[action]], reconcile rows
    server/
      webhook.rs                           # HMAC bearer router → credential → action_id → spawn
      api.rs                               # /api/sources, /api/credentials, /api/actions/:id/run, /webhooks/proxy
      credentials_api.rs                   # validate_device_token, list/rename/revoke credentials
    api/
      auth.rs                              # 5 thin handlers: pair_initiate, pair_complete,
                                           #                  oauth_start, oauth_callback, apikey_complete
    scheduler/                             # cron tick → trigger fires
    agent/                                 # LLM agent loop (used in agent-phase)
    credentials/                           # Vault types + post-migration hook
    tools/
      code_interpreter/                    # untrusted Python in Docker (NOT an action)

actions/                                   # All action implementations
  templates.toml                           # [[source]] + [[action]] catalog
  helpers/                                 # virtues-action-helpers crate
  src/
    bin/
      ios_healthkit/                       # webhook ingest
      ios_location/
      …
      google_calendar_sync/                # cron, per_credential
      google_mail_sync/
      plaid_transactions_sync/
      …
      credential_refresh/                  # cron sweeper for via_proxy credentials
      day_summary_eod/                     # function + agent
      embedding_index/                     # function only

apps/
  oauth-proxy/                             # Node/TS proxy holding third-party OAuth secrets
                                           # (we run it; not part of self-hosted Virtues)
```

---

## User-created actions

This is the whole point. Two on-ramps:

1. **Agent-only (no code)**: create an `app_actions` row with `agent` and `cron_schedule` via the UI. The LLM runs on schedule. Use `condition` to gate expensive runs.

2. **Compiled Rust binary**: write a new directory under `actions/src/bin/{name}/`, depend on `virtues-action-helpers`, add a `templates.toml` entry, rebuild. **Same shape as system actions.** A custom IoT device, a quirky API, a personal automation — all the same path.

For now, all action binaries ship in the cargo workspace. A `~/.virtues/actions/` overlay (drop a binary at runtime, no rebuild) is a clear future direction; not v1.

---

## Vocabulary

One word per concept. Used identically in UI and code:

| Term | Definition |
|---|---|
| **Source** | A provider you can connect to. A `[[source]]` row in `templates.toml`. UI calls it "Source." Code field is `source_id`. |
| **Password** | A user's stored credential — what the UI shows. Internally a row in the `credentials` table. We say "password" in copy because it's the familiar word and it opens the door to user-stored secrets (1Password-shaped) in the same table. |
| **Action** | A runnable behavior — Rust binary in `actions/src/bin/`, `app_actions` row, fires on a trigger. |
| **Trigger** | When an action fires. `cron`, `webhook`, `manual`, `tool`. |

The user sees: "I added a Google source. I have a Google password. It connects my Google Calendar action."
The code sees: `source_id = "google"`, `credentials.id = "abc-123"`, `app_actions.credential_id = "abc-123"`.

Same words, both layers. No translation tax.

---

## MCP servers, LLM keys, "bare" credentials

The Vault holds more than provider credentials.

### MCP servers

`app_mcp_servers.credential_id → credentials(id)`. Each MCP server with auth has a credential row with `source_id = "mcp:{server_name}"` and `auth.kind = "api_key"`. The MCP client reads the credential at connect time via the same Vault path actions use.

### LLM provider keys

Out of scope for the Vault by default. Routed through the universal Virtues gateway. If a user opts to BYO their own Anthropic/OpenAI/Gemini key, that's a future entry with `source_id = "user:llm_anthropic"` and `auth.kind = "api_key"`. Same table.

### User-stored passwords (future, 1Password-shaped)

The Vault is structurally ready. A user-stored bank login or wifi password lands as a credential with `source_id = "user:..."` and `auth.kind = "user_password"` (a fourth variant added when the feature ships). Same encryption, same status state machine, no second secret store.

---

## Health and visibility

A user mostly never sees "actions." They see their data appearing — calendar events, sleep, transactions, day summaries. The interaction surfaces:

- **Sources tab**: catalog + your passwords (active connections), with each one's fan-out streams expanded.
- **Actions tab**: power-user view of system actions, runs, templates.

Health is computed from `app_action_runs`:

| Status | Meaning |
|---|---|
| **healthy** | Last N runs succeeded with non-trivial results |
| **degraded** | Runs succeed but return empty for longer than typical |
| **failing** | Last run errored |
| **stale** | Enabled but hasn't run in 2× its cron interval |

Drives proactive alerts: "Your Google Calendar sync hasn't pulled new events in 3 days."

---

## Anti-patterns (don't do these)

- **Don't add a new "action type" enum.** The shape is determined by which fields are populated (`function_name`, `agent`, both). Adding a type discriminator is the path to special cases.
- **Don't write per-provider auth Rust.** All auth flows go through the 5 handlers in `core/src/api/auth.rs`, which call `virtues-auth` helpers and branch on `source.auth.kind`. New providers add a `[[source]]` row; the handlers don't change.
- **Don't introduce a second action runtime.** No Python action subprocesses, no JS workers. The LLM code interpreter is its own thing (untrusted, sandboxed, called as a tool).
- **Don't create a declarative DSL for actions.** Code is the implementation. If a pattern repeats, add it to helpers.
- **Don't conflate `function_name` with a path.** It's a logical name; the runner resolves it to `actions/src/bin/{function_name}`.
- **Don't store secrets in `app_actions.config`.** All secrets go through the Vault. Config carries non-secret settings + code-managed state (cursors).
- **Don't put third-party OAuth client_secret on the user's box.** It belongs in the proxy. Self-hosted Virtues stores only access/refresh tokens that the proxy issued.
- **Don't fork the Vault for "special" secret types.** MCP tokens, LLM keys, user-stored passwords all go in `credentials` with a `source_id` namespace prefix. One table, one encryption boundary.
- **Don't `println!` from a binary.** It corrupts the stdout JSON contract. Use `tracing` to stderr; the runner captures stderr to `app_action_runs.error` on failure.
- **Don't `match source_id` in `core/src/api/`.** Provider-specific quirks live in proxy routes (for `via_proxy`) or in the source-tagged sync binary. Core stays catalog-driven.
- **Don't implement HMAC primitives outside `crates/virtues-crypto/`.** One HMAC home; CI greps `Hmac::<Sha256>` outside it as a fence.
- **Don't add a fourth `auth.kind` casually.** The three we have cover every redirect-based, paired, or paste-a-string pattern. New variants require a real provider that doesn't fit, a generic helper implementation, and a charter event.

---

## Out of scope (deliberately)

- **`VIRTUES_OAUTH_PROXY_URL` env var** to self-host the proxy. <0.1% of users will care.
- **User-stored passwords** (1Password-shaped). The schema is ready; the feature isn't.
- **LLM provider keys** in the Vault (today). Routed through the gateway.
- **Multi-tenancy.** Single user per instance. No `owner_user_id`.
- **Master-key rotation.** `VIRTUES_ENCRYPTION_KEY` is stable; no `key_version` column.
- **WebAuthn / passkey login.** That authenticates the user *to* Virtues (lives next to `app_auth_session`), not Virtues *to* a third party.
- **Per-action DB role isolation.** Today every action gets the same `DATABASE_URL`.
- **Community-installed actions at `~/.virtues/actions/`.** Today, all binaries ship in the cargo workspace.

---

## Strategic invariants

1. **Adding a new redirect-based provider is one `[[source]]` row + one proxy route + one or more sync binaries.** No core auth changes. No SDK changes. No new handlers.
2. **Self-hosted Virtues never holds a third-party OAuth client_secret.** The proxy is the only place those live.
3. **The Vault is the only secret store.** Every encrypted secret in the system is in `credentials`. No second table, no env-var sneaking, no plaintext columns.
4. **Status drives the runner.** Only `active` credentials are usable. Status transitions are explicit and surfaced in the UI.
5. **Three auth kinds, period** (until a real provider proves they're insufficient).
6. **The catalog is data, not code.** Adding a source is editing a TOML row. Reading it is parsing TOML.
7. **Subprocess is the user-shippability boundary.** Anything in core is something a user must PR upstream to change. Keep that surface as small as the design allows.

The forcing function: **the next 10 redirect-based providers (Notion, Spotify, Strava, GitHub, Microsoft, Slack, Stripe Connect, Discord, Linear, LinkedIn) should look identical to Google.** Each = one `[[source]]` row + one proxy route + one or two sync binaries. If any of them requires more, the architecture has drifted — fix the drift before shipping the connector.
