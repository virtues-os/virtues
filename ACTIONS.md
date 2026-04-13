# Actions

> An action is how data enters, transforms, and surfaces in your personal OS.
> Some actions pull from APIs. Some receive pushes from your phone.
> Some are pure LLM reasoning over what's already there.
> All of them read config and write to the database.

---

## The Spectrum

Every action sits somewhere on this spectrum:

| Level | function_name | agent | Example |
|-------|:---:|:---:|---------|
| **Function-only** | yes | — | HealthKit ingest, embedding index, trash purge |
| **Function + agent** | yes | yes | Dayline EOD: resolve sleep (function), then write autobiography (LLM) |
| **Agent-only** | — | yes | "Summarize yesterday's spending by category" |

One schema, three power levels. A function-only action can gain LLM augmentation by adding an `agent` field. No schema change, no new action type.

---

## Schema

```sql
app_actions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    enabled BOOLEAN DEFAULT true,

    function_name TEXT,              -- subprocess name (resolved to a binary or script)
    agent TEXT,                      -- LLM prompt (runs agent loop)
    agent_max_retries INT DEFAULT 3, -- runner retries on transient agent errors (timeouts, 5xx)
    condition TEXT,                  -- SQL expression: skip run if falsy

    cron_schedule TEXT,              -- "*/15 * * * *"
    config JSONB DEFAULT '{}',       -- settings + code-managed state (cursors, checkpoints)
    credential_id TEXT REFERENCES action_credentials(id),

    created_at TIMESTAMPTZ DEFAULT now(),
    updated_at TIMESTAMPTZ DEFAULT now()
)
```

**config** holds both user settings and code-managed state in one JSONB blob:

```json
{ "calendar_ids": ["primary", "work"], "sync_token": "abc123" }
```

User-facing keys (`calendar_ids`) are shown in the UI. Code-managed keys (`sync_token`) are updated by the action and saved by the runner after success.

```sql
action_credentials (
    id TEXT PRIMARY KEY,
    provider TEXT NOT NULL,
    name TEXT NOT NULL,
    auth_type TEXT NOT NULL,         -- 'oauth2', 'api_key', 'device', 'plaid'
    access_token TEXT,               -- encrypted at rest
    refresh_token TEXT,
    token_expires_at TIMESTAMPTZ,
    device_id TEXT,
    device_token TEXT,
    device_info JSONB,
    last_seen_at TIMESTAMPTZ,
    is_active BOOLEAN DEFAULT true,
    error_message TEXT,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT now(),
    updated_at TIMESTAMPTZ DEFAULT now()
)

app_action_runs (
    id TEXT PRIMARY KEY,
    action_id TEXT REFERENCES app_actions(id),
    trigger TEXT NOT NULL,           -- 'cron', 'push', 'manual', 'api', 'tool'
    status TEXT NOT NULL DEFAULT 'running',
    result_summary TEXT,
    error TEXT,
    started_at TIMESTAMPTZ DEFAULT now(),
    finished_at TIMESTAMPTZ
)

data_archives (
    id TEXT PRIMARY KEY,
    action_id TEXT REFERENCES app_actions(id),
    storage_key TEXT UNIQUE,         -- S3/MinIO path
    record_count INTEGER,
    size_bytes BIGINT,
    min_timestamp TIMESTAMPTZ,
    max_timestamp TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT now()
)
```

---

## Two Planes

**Control plane: stdin/stdout.** The runner pipes `{config, credentials, payload}` to the subprocess via stdin. The subprocess returns `{result, config}` via stdout. Exit 0 = success, exit 1 = failure.

**Data plane: the database.** The subprocess connects directly to PostgreSQL and reads/writes ontology tables. This is by design — serializing entire datasets through a pipe would be impractical. The DB is shared mutable state.

Two connection strings ship by default:
- `VIRTUES_DATABASE_URL` — full access, passed to system actions
- `PERSONAL_DATABASE_URL` — restricted PG role, can only CRUD `personal.*`, passed to user actions

The "Publish to Virtues Data" toggle grants the user role `INSERT/UPDATE` on specific `virtues.data_*` tables. Enforcement is at the PG level, not application level.

### stdin

```json
{
  "config": { "calendar_ids": ["primary"], "sync_token": "abc123" },
  "credentials": { "access_token": "ya29...", "refresh_token": "1//..." },
  "payload": null
}
```

- **config**: from the action row. Settings + state.
- **credentials**: decrypted by the runner from `action_credentials`. Null if no `credential_id`.
- **payload**: ingest records for push actions. Null for cron/manual.

### stdout

```json
{ "result": "synced 42 events", "config": { "calendar_ids": ["primary"], "sync_token": "def456" } }
```

Runner saves returned `config` back to the action row. That's how cursors persist.

---

## Runner

```
Trigger fires (cron | push | manual | api | tool call)
  │
  ├── Skip if already running (atomic: INSERT WHERE NOT EXISTS)
  │
  ├── Evaluate condition (if set)
  │     SELECT ({condition})  →  falsy? mark 'skipped', stop
  │
  ├── Spawn function_name subprocess (if set)
  │     Pipe {config, credentials, payload} to stdin
  │     Read {result, config} from stdout
  │     Exit 1? → mark 'failed' with stderr, stop
  │     Exit 0? → save returned config to app_actions.config
  │
  ├── Run agent (if set)
  │     LLM agent loop with function result as context
  │     On transient error → retry up to agent_max_retries (fixed 5s delay)
  │     On final error → mark 'failed'
  │
  └── Mark 'success' with result_summary
```

The condition gates everything — runs before subprocess or LLM. Primarily useful for agent-only actions (saves LLM cost). Function-only actions can gate themselves internally in their first few lines.

---

## Triggers

| Trigger | How it fires |
|---------|-------------|
| `cron` | Scheduler checks `cron_schedule` every tick |
| `push` | Device POSTs to `/ingest`, mapped to action by credential + stream |
| `manual` | User clicks "Run" in UI |
| `api` | `POST /api/actions/:id/run` |
| `tool` | LLM agent calls `run_action` tool (MCP-compatible) |

Push actions receive batched data (e.g., iOS HealthKit sends every 5 minutes). One subprocess spawn per batch, not per record.

---

## Actions Are Modules

An action's `function_name` resolves to a subprocess — a compiled Rust binary, a Python script, or anything that reads stdin JSON and writes stdout JSON.

### Source action (folder with multiple files)

```rust
// actions/src/bin/google_calendar/main.rs

mod sync;
mod transform;

#[tokio::main]
async fn main() -> Result<()> {
    let db = helpers::connect_from_env().await?;
    let input: ActionInput = serde_json::from_reader(std::io::stdin())?;
    let mut config = input.config.clone();

    let creds = input.credentials.ok_or("missing credentials")?;
    let token = helpers::oauth::refresh_if_expired(&db, &creds).await?;

    let sync_token = config["sync_token"].as_str();
    let records = sync::fetch(&token, sync_token).await?;
    if records.is_empty() {
        return helpers::output("no new events", &config);
    }

    let count = transform::write(&db, &config, &records).await?;
    transform::resolve_attendees(&db, &config).await?;

    config["sync_token"] = json!(records.next_sync_token);
    helpers::output(&format!("synced {} events", count), &config)
}
```

### Push action (receives ingest payload)

```rust
// actions/src/bin/ios_healthkit/main.rs

mod transform;

#[tokio::main]
async fn main() -> Result<()> {
    let db = helpers::connect_from_env().await?;
    let input: ActionInput = serde_json::from_reader(std::io::stdin())?;

    let records = input.payload.as_ref().and_then(|p| p.as_array())
        .ok_or("push action requires payload")?;

    let mut results = Vec::new();
    for record in records {
        let r = match record["metric_type"].as_str() {
            Some("heart_rate") => transform::write_heart_rate(&db, record).await,
            Some("steps") => transform::write_steps(&db, record).await,
            Some("sleep") => transform::write_sleep(&db, record).await,
            _ => continue,
        };
        match r {
            Ok(n) => results.push(format!("{}: {}", record["metric_type"].as_str().unwrap_or("?"), n)),
            Err(e) => results.push(format!("{}: ERR {}", record["metric_type"].as_str().unwrap_or("?"), e)),
        }
    }
    helpers::output(&results.join(", "), &input.config)
}
```

---

## Templates

Templates define available actions and their defaults. Shipped as `actions/templates.toml`.

```toml
# actions/templates.toml (representative entries)

[[templates]]
id = "google_calendar"
name = "Google Calendar Sync"
function_name = "google_calendar"
requires_credential = "google"
default_cron = "*/15 * * * *"
[templates.default_config]
calendar_ids = ["primary"]

[[templates]]
id = "ios_healthkit"
name = "iOS HealthKit"
function_name = "ios_healthkit"
requires_credential = "ios"
# no default_cron = push-triggered

[[templates]]
id = "oauth_token_refresh"
name = "OAuth Token Refresh"
function_name = "oauth_token_refresh"
default_cron = "*/30 * * * *"
# no requires_credential — refreshes all OAuth credentials

# ... dayline_hourly, dayline_eod, entity_resolution, embedding_index, etc.
```

**On install/update**: templates without `requires_credential` get `app_actions` rows (system actions, always enabled). Templates with `requires_credential` get rows when the user connects that source.

**On Virtues update**: new binaries replace old binaries (same name). Existing action rows are NOT touched — user customizations (cron, config) are preserved. If a new binary needs a new config key, it uses `serde(default)`. Breaking changes are handled via database migrations, same as any schema update.

**Source connected** → create credential → find templates with matching `requires_credential` → create action rows → syncs start.

**Source disconnected** → disable all actions referencing that credential → revoke tokens.

---

## Ingest Flow

```
Device POSTs to /ingest
  ├── { source: "ios", stream: "healthkit", records: [...] }
  ├── Validate device token → get credential_id
  ├── Look up action: function_name = '{source}_{stream}' AND credential_id = ?
  ├── Spawn subprocess with { config, credentials, payload: records } on stdin
  └── Return { accepted, rejected }
```

One endpoint for all devices. Stream name routes to the right action by convention.

---

## User-Created Actions

Three ways, simplest to most powerful:

**1. LLM-only (no code):** Create an action row with `agent` and `cron_schedule`. The LLM runs on schedule. `condition` SQL prevents unnecessary LLM calls.

**2. Script (any language):** Create `actions/user/{name}/main.py` (or `.js`, `.rb`, etc.). Same stdin/stdout JSON contract. Runner detects the entrypoint and spawns the matching interpreter. No compilation, no helpers provided — the author brings their own libraries.

**3. Compiled Rust:** Create `actions/user/{name}/` with `Cargo.toml` and `main.rs`. Compile with `cargo build --release`. Maximum performance and type safety, and access to `virtues-action-helpers`.

All user actions run with `PERSONAL_DATABASE_URL` (restricted PG role, `personal.*` only). The LLM assistant can write files to `actions/user/` but cannot modify system actions in `actions/src/`.

---

## Helpers

Shared Rust crate used by all system actions:

```
actions/helpers/
  Cargo.toml                     ← Rust crate: virtues-action-helpers
  src/
    lib.rs                       — ActionInput, ActionOutput, output(), connect_from_env()
    oauth.rs                     — token refresh per provider
    dedup.rs                     — batch_upsert() with ON CONFLICT
    entity.rs                    — resolve_people(), cluster_places()
```

System actions (and user actions written in Rust) depend on `virtues-action-helpers` via the workspace. User actions in other languages use the stdin/stdout contract directly and bring their own libraries — no equivalent helper is shipped for non-Rust runtimes.

---

## Database Namespaces (PostgreSQL)

```
virtues schema (system-managed, VIRTUES_DATABASE_URL)
  ├── app_actions, app_action_runs, action_credentials
  ├── data_health_*, data_calendar_*, data_financial_*, data_location_*, ...
  ├── wiki_days, wiki_events, wiki_people, wiki_places, ...
  └── data_archives

personal schema (user-managed, PERSONAL_DATABASE_URL)
  ├── weather, mood_log, custom_metrics, ...
  └── created via LLM tool calls or direct SQL
```

System actions write to `virtues.*`. User actions write to `personal.*` by default. The "Publish to Virtues Data" toggle grants `INSERT/UPDATE` on specific `virtues.data_*` tables to the user PG role.

---

## Visibility & Health

A user never sees "actions." They see their data appearing — calendar events, health metrics, location visits. The only interaction with actions is in Settings → Sources:
- Connected sources with status (green/amber/red)
- Last sync time and record counts
- "Run now" button
- Toggle to enable/disable streams

**Health** is computed from `app_action_runs`:

| Status | Meaning |
|--------|---------|
| **healthy** | Last N runs succeeded with non-trivial results |
| **degraded** | Runs succeed but return empty for longer than typical |
| **failing** | Last run errored |
| **stale** | Enabled but hasn't run in 2x its cron interval |

Feeds proactive alerts: "Your Google Calendar sync hasn't pulled new events in 3 days."

---

## Folder Structure

```
core/                              # Infrastructure only
  src/
    scheduler/                     # Cron dispatch, run tracking, subprocess spawning
    api/                           # HTTP endpoints (OAuth, ingest, action CRUD)
    agent/                         # LLM agent loop (agent-only actions)

actions/                           # All action implementations
  Cargo.toml                       # Workspace with [[bin]] per system action
  templates.toml                   # Action template catalog
  helpers/                         # Shared Rust crate (virtues-action-helpers)
  src/bin/                         # System action binaries
    google_calendar/
    google_gmail.rs
    ios_healthkit/
    ios_location.rs
    strava_activities/
    notion_pages/
    oauth_token_refresh.rs
    dayline_hourly.rs
    dayline_eod.rs
    entity_resolution.rs
    embedding_index.rs
    ...
  user/                            # User-created actions (outside workspace)
    weather_ingest/main.py
    custom_sync/Cargo.toml + main.rs
```

---

## What Changes

| Old | New |
|-----|-----|
| `elt_source_connections` | `action_credentials` |
| `elt_stream_connections` | `action.config` (settings + cursors) |
| `elt_stream_objects` | `data_archives` |
| `core/src/sources/` (40+ files) | `actions/src/bin/` (separate subprocesses) |
| StreamFactory, PushStream, PullStream | stdin/stdout JSON |
| `action_type` enum | Inferred from which fields are populated |
| Python activation gates | SQL condition expressions |
| Shared process | Separate subprocess per action |

| Stays | Why |
|-------|-----|
| `app_action_runs` | Run tracking, unchanged |
| All `data_*` ontology tables | The target, unchanged |
| `virtues-registry` ontology definitions | Table metadata |
| Python sandbox | For in-chat code interpreter only |
