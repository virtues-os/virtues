# Database Schema Reference

> **Auto-generated from migrations.** This is the current truth of every table.
> To regenerate: apply all migrations to a fresh DB, then `sqlite3 db ".schema"`.
>
> Last updated: 2026-03-31

---

## App: User & Auth

### app_user_profile (singleton)
```sql
id                   TEXT PK DEFAULT '00000000-...-000000000001'
full_name            TEXT
preferred_name       TEXT
birth_date           TEXT
height_cm            REAL
weight_kg            REAL
ethnicity            TEXT
occupation           TEXT
employer             TEXT
onboarding_status    TEXT NOT NULL DEFAULT 'welcome'   CHECK IN ('welcome','profile','places','tools','complete')
home_place_id        TEXT                              -- FK wiki_places
theme                TEXT DEFAULT 'light'
update_check_hour    INTEGER DEFAULT 8                 CHECK 0-23
crux                 TEXT                              -- legacy (unused)
technology_vision    TEXT                              -- legacy (unused)
pain_point_primary   TEXT                              -- legacy (unused)
pain_point_secondary TEXT                              -- legacy (unused)
excited_features     TEXT                              -- legacy (unused), JSON array
owner_email          TEXT
server_status        TEXT NOT NULL DEFAULT 'provisioning' CHECK IN ('provisioning','migrating','ready')
timezone             TEXT                              -- added 014
created_at           TEXT
updated_at           TEXT
```

### app_assistant_profile (singleton)
```sql
id                   TEXT PK DEFAULT '00000000-...-000000000001'
assistant_name       TEXT DEFAULT 'Ari'
default_agent_id     TEXT DEFAULT 'agent'
default_model_id     TEXT DEFAULT 'anthropic/claude-sonnet-4-20250514'
background_model_id  TEXT DEFAULT 'cerebras/llama-3.3-70b'
enabled_tools        TEXT                              -- JSON
ui_preferences       TEXT                              -- JSON
embedding_model_id   TEXT DEFAULT 'nomic-embed-text'
ollama_endpoint      TEXT DEFAULT 'http://localhost:11434'
persona              TEXT DEFAULT 'standard'
chat_model_id        TEXT
lite_model_id        TEXT
reasoning_model_id   TEXT
coding_model_id      TEXT
personas             TEXT                              -- JSON, nullable
memory               TEXT                              -- AI persistent memory, added 031
created_at           TEXT
updated_at           TEXT
```

### app_auth_user
```sql
id                   TEXT PK
email                TEXT UNIQUE NOT NULL
email_verified       TEXT
created_at           TEXT
updated_at           TEXT
```

### app_auth_session
```sql
id                   TEXT PK
session_token        TEXT UNIQUE NOT NULL
user_id              TEXT NOT NULL                     -- FK app_auth_user
expires              TEXT NOT NULL
created_at           TEXT
```

### app_auth_verification_token
```sql
identifier           TEXT NOT NULL
token                TEXT NOT NULL
expires              TEXT NOT NULL
PRIMARY KEY (identifier, token)
```

---

## App: Chat

### app_chats
```sql
id                   TEXT PK
title                TEXT NOT NULL
message_count        INTEGER DEFAULT 0
trace                TEXT
conversation_summary TEXT
summary_up_to_index  INTEGER DEFAULT 0
summary_version      INTEGER DEFAULT 0
last_compacted_at    TEXT
icon                 TEXT
action_instruction   TEXT                              -- action instruction (scheduled/triggered)
created_at           TEXT
updated_at           TEXT
```

### app_chat_messages
```sql
id                   TEXT PK
chat_id              TEXT NOT NULL                     -- FK app_chats
role                 TEXT NOT NULL                     CHECK IN ('user','assistant','system','checkpoint')
content              TEXT NOT NULL
model                TEXT
provider             TEXT
agent_id             TEXT
reasoning            TEXT
tool_calls           TEXT                              -- JSON array
intent               TEXT                              -- JSON
subject              TEXT
sequence_num         INTEGER NOT NULL
thought_signature    TEXT
parts                TEXT                              -- JSON, structured message parts
created_at           TEXT
UNIQUE(chat_id, sequence_num)
```

### app_chat_edit_permissions
```sql
id                   TEXT PK
chat_id              TEXT NOT NULL                     -- FK app_chats
entity_id            TEXT NOT NULL
entity_type          TEXT NOT NULL                     -- 'page', 'folder', etc.
entity_title         TEXT
granted_at           TEXT
UNIQUE(chat_id, entity_id)
```

### app_chat_usage
```sql
id                   TEXT PK
chat_id              TEXT NOT NULL                     -- FK app_chats
model                TEXT NOT NULL
input_tokens         INTEGER DEFAULT 0
output_tokens        INTEGER DEFAULT 0
reasoning_tokens     INTEGER DEFAULT 0
cache_read_tokens    INTEGER DEFAULT 0
cache_write_tokens   INTEGER DEFAULT 0
estimated_cost_usd   REAL DEFAULT 0
created_at           TEXT
updated_at           TEXT
UNIQUE(chat_id, model)
```

---

## App: Spaces, Views, Pages

### app_spaces
```sql
id                   TEXT PK
name                 TEXT NOT NULL
icon                 TEXT
is_system            BOOLEAN DEFAULT FALSE
sort_order           INTEGER DEFAULT 0
theme_id             TEXT NOT NULL DEFAULT 'tatooine'
accent_color         TEXT
active_tab_state_json TEXT
vectorize            BOOLEAN DEFAULT FALSE
created_at           TEXT
updated_at           TEXT
```

### app_views
```sql
id                   TEXT PK
space_id             TEXT NOT NULL                     -- FK app_spaces
parent_view_id       TEXT                              -- FK app_views (depth=1)
name                 TEXT NOT NULL
icon                 TEXT
sort_order           INTEGER DEFAULT 0
view_type            TEXT NOT NULL                     CHECK IN ('manual','smart')
query_config         TEXT                              -- JSON (smart views)
is_system            BOOLEAN DEFAULT FALSE
created_at           TEXT
updated_at           TEXT
```

### app_space_items
```sql
id                   INTEGER PK AUTOINCREMENT
view_id              TEXT                              -- FK app_views (XOR with space_id)
space_id             TEXT                              -- FK app_spaces (XOR with view_id)
url                  TEXT NOT NULL
sort_order           INTEGER DEFAULT 0
created_at           TEXT
```

### app_pages
```sql
id                   TEXT PK                           -- e.g. 'page_a1b2c3d4e5f6g7h8'
title                TEXT NOT NULL
content              TEXT NOT NULL DEFAULT ''
icon                 TEXT
cover_url            TEXT
tags                 TEXT                              -- JSON array
yjs_state            BLOB                              -- Yjs document state
created_at           TEXT
updated_at           TEXT
```

### app_page_versions
```sql
id                   TEXT PK
page_id              TEXT NOT NULL                     -- FK app_pages
version_number       INTEGER NOT NULL
yjs_snapshot         BLOB
content_preview      TEXT
created_at           TEXT
created_by           TEXT DEFAULT 'user'
description          TEXT
UNIQUE(page_id, version_number)
```

### app_page_shares
```sql
id                   TEXT PK
page_id              TEXT NOT NULL UNIQUE               -- FK app_pages
token                TEXT UNIQUE NOT NULL
created_at           TEXT
```

### app_namespaces
```sql
name                 TEXT PK                           -- 'person', 'drive', 'virtues'
backend              TEXT NOT NULL                     -- 'sqlite', 'filesystem', 's3', 'none'
backend_config       TEXT                              -- JSON
is_entity            BOOLEAN DEFAULT FALSE
is_system            BOOLEAN DEFAULT FALSE
icon                 TEXT
label                TEXT
created_at           TEXT
```

---

## App: MCP

### app_mcp_servers
```sql
id                   TEXT PK
name                 TEXT NOT NULL
url                  TEXT NOT NULL
description          TEXT
auth_token           TEXT
enabled              INTEGER DEFAULT 1
status               TEXT DEFAULT 'disconnected'       CHECK IN ('disconnected','connecting','connected','error')
last_error           TEXT
last_connected_at    TEXT
created_at           TEXT
updated_at           TEXT
```

### app_mcp_tools
```sql
id                   TEXT PK
server_id            TEXT NOT NULL                     -- FK app_mcp_servers
server_name          TEXT NOT NULL
tool_name            TEXT NOT NULL
description          TEXT
input_schema         TEXT
enabled              INTEGER DEFAULT 1
created_at           TEXT
```

---

## App: Usage & Limits

### app_api_usage
```sql
id                   TEXT PK
endpoint             TEXT NOT NULL
day_bucket           TEXT NOT NULL
request_count        INTEGER DEFAULT 0
token_count          INTEGER DEFAULT 0
input_tokens         INTEGER DEFAULT 0
output_tokens        INTEGER DEFAULT 0
estimated_cost_usd   REAL DEFAULT 0
created_at           TEXT
updated_at           TEXT
UNIQUE(endpoint, day_bucket)
```

### app_usage_limits
```sql
service              TEXT PK
monthly_limit        INTEGER NOT NULL
unit                 TEXT DEFAULT 'requests'
limit_type           TEXT DEFAULT 'hard'               CHECK IN ('hard','soft')
enabled              INTEGER DEFAULT 1
created_at           TEXT
updated_at           TEXT
```

---

## ELT: Sources & Streams

### elt_source_connections
```sql
id                   TEXT PK
source               TEXT NOT NULL
name                 TEXT NOT NULL UNIQUE
access_token         TEXT
refresh_token        TEXT
token_expires_at     TEXT
auth_type            TEXT DEFAULT 'oauth2'             CHECK IN ('oauth2','device','api_key','none','plaid')
device_id            TEXT
device_info          TEXT                              -- JSON
device_token         TEXT
pairing_status       TEXT                              CHECK IN ('pending','active','revoked')
last_seen_at         TEXT
is_active            INTEGER DEFAULT 1
is_internal          INTEGER DEFAULT 0
error_message        TEXT
error_at             TEXT
metadata             TEXT                              -- JSON
sync_strategy        TEXT DEFAULT 'ongoing'            CHECK IN ('migration','ongoing','hybrid')
tier                 TEXT DEFAULT 'free'
connection_policy    TEXT DEFAULT 'multi_instance'
created_at           TEXT
updated_at           TEXT
```

### elt_stream_connections
```sql
id                   TEXT PK
source_connection_id TEXT NOT NULL                     -- FK elt_source_connections
stream_name          TEXT NOT NULL
table_name           TEXT NOT NULL
is_enabled           INTEGER DEFAULT 1
cron_schedule        TEXT
config               TEXT DEFAULT '{}'                 -- JSON
last_sync_token      TEXT
last_sync_at         TEXT
earliest_record_at   TEXT
latest_record_at     TEXT
sync_status          TEXT DEFAULT 'pending'            CHECK IN ('pending','initial','incremental','backfilling','failed')
created_at           TEXT
updated_at           TEXT
UNIQUE(source_connection_id, stream_name)
```

### elt_stream_objects
```sql
id                   TEXT PK
source_connection_id TEXT NOT NULL                     -- FK elt_source_connections
stream_name          TEXT NOT NULL
storage_key          TEXT NOT NULL UNIQUE
record_count         INTEGER NOT NULL                  CHECK > 0
size_bytes           INTEGER NOT NULL                  CHECK > 0
min_timestamp        TEXT
max_timestamp        TEXT
created_at           TEXT
updated_at           TEXT
```

### elt_stream_checkpoints
```sql
id                   TEXT PK
source_id            TEXT NOT NULL
stream_name          TEXT NOT NULL
checkpoint_key       TEXT NOT NULL
last_processed_at    TEXT NOT NULL
created_at           TEXT
updated_at           TEXT
UNIQUE(source_id, stream_name, checkpoint_key)
```

---

## Drive

### app_drive_files
```sql
id                   TEXT PK
path                 TEXT NOT NULL UNIQUE
filename             TEXT NOT NULL
mime_type            TEXT
size_bytes           INTEGER NOT NULL                  CHECK >= 0
parent_id            TEXT                              -- FK app_drive_files (self-ref)
is_folder            INTEGER DEFAULT 0
sha256_hash          TEXT
deleted_at           TEXT                              -- soft delete
created_at           TEXT
updated_at           TEXT
```

### app_drive_usage (singleton)
```sql
id                   TEXT PK DEFAULT '00000000-...-000000000001'
drive_bytes          INTEGER DEFAULT 0
data_lake_bytes      INTEGER DEFAULT 0
total_bytes          INTEGER DEFAULT 0
file_count           INTEGER DEFAULT 0
folder_count         INTEGER DEFAULT 0
quota_bytes          INTEGER DEFAULT 107374182400      -- 100 GB
warning_80_sent      INTEGER DEFAULT 0
warning_90_sent      INTEGER DEFAULT 0
warning_100_sent     INTEGER DEFAULT 0
last_scan_at         TEXT
last_scan_bytes      INTEGER
trash_bytes          INTEGER DEFAULT 0
trash_count          INTEGER DEFAULT 0
created_at           TEXT
updated_at           TEXT
```

---

## Data Ontology Tables

All data tables share these common columns:
```sql
id                   TEXT PK
source_connection_id TEXT                              -- FK elt_source_connections
source_stream_id     TEXT NOT NULL UNIQUE
source_table         TEXT NOT NULL
source_provider      TEXT NOT NULL
deleted_at_source    TEXT
is_archived          INTEGER DEFAULT 0
metadata             TEXT DEFAULT '{}'                 -- JSON
created_at           TEXT
updated_at           TEXT
```

### data_health_workout
`workout_type, start_time, end_time, duration_minutes, calories_burned, distance_km, avg_heart_rate, max_heart_rate, route_geometry`

### data_health_sleep
`sleep_stages (JSON), start_time, end_time, duration_minutes, sleep_quality_score`

### data_health_steps
`step_count, timestamp`

### data_health_heart_rate
`bpm, timestamp`

### data_health_hrv
`hrv_ms, timestamp`

### data_location_visit
`place_name, latitude, longitude, arrival_time, departure_time, duration_minutes`

### data_location_point
`latitude, longitude, altitude, horizontal_accuracy, vertical_accuracy, timestamp`

### data_calendar_event
`title, description, calendar_name, event_type, status, response_status, organizer_identifier, attendee_identifiers, location_name, conference_url, conference_platform, start_time, end_time, is_all_day, timezone, recurrence_rule, block_type, is_sacred, external_id, external_url`

### data_communication_email
`message_id, thread_id, subject, body, body_preview, from_email, from_name, to_emails, to_names, cc_emails, bcc_emails, direction, is_read, is_starred, has_attachments, labels, timestamp`

### data_communication_message
`message_id, thread_id, channel, body, from_identifier, from_name, to_identifiers, is_read, is_group_message, reply_to_message_id, has_attachments, timestamp`

### data_communication_transcription
`audio_url, text, language, duration_seconds, start_time, end_time, speaker_count, speaker_segments, title, summary, confidence, tags, entities`

### data_financial_account
`account_name, account_type, institution_name, institution_id, mask, currency, current_balance, available_balance, credit_limit, is_active`

### data_financial_transaction
`account_id, transaction_id, amount (cents), currency, merchant_name, merchant_category, description, category, is_pending, transaction_type, payment_channel, timestamp, authorized_timestamp`

### data_financial_asset
`account_id, asset_type, symbol, name, quantity, cost_basis (cents), current_value (cents), currency, timestamp`

### data_financial_liability
`account_id, liability_type, principal (cents), interest_rate, minimum_payment (cents), next_payment_due_date, origination_date, maturity_date, currency, timestamp`

### data_activity_app_usage
`app_name, app_bundle_id, app_category, start_time, end_time, window_title, document_path, url`

### data_activity_web_browsing
`url, domain, page_title, visit_duration_seconds, scroll_depth_percent, timestamp`

### data_activity_listening
`track_name, artist_name, album_name, duration_ms, played_at, spotify_track_id, spotify_uri, context_type, context_name, context_uri`

### data_content_bookmark
`url, title, description, source_platform, bookmark_type, content_type, author, tags, thumbnail_url, timestamp`

### data_content_conversation
`conversation_id, message_id, role, content, model, provider, tags, timestamp`

### data_content_document
`title, content, content_summary, document_type, external_id, external_url, tags, is_authored, created_time, last_modified_time`

---

## Wiki

### wiki_people
```sql
id                   TEXT PK
canonical_name       TEXT NOT NULL
emails               TEXT DEFAULT '[]'                 -- JSON array
phones               TEXT DEFAULT '[]'                 -- JSON array
relationship_category TEXT
nickname             TEXT
notes                TEXT
first_interaction    TEXT
last_interaction     TEXT
interaction_count    INTEGER DEFAULT 0
metadata             TEXT DEFAULT '{}'                 -- JSON
content              TEXT                              -- wiki markdown
picture              TEXT
cover_image          TEXT
birthday             TEXT
instagram            TEXT
facebook             TEXT
linkedin             TEXT
x                    TEXT
created_at           TEXT
updated_at           TEXT
```

### wiki_places
```sql
id                   TEXT PK
name                 TEXT NOT NULL
category             TEXT
address              TEXT
latitude             REAL
longitude            REAL
radius_m             REAL DEFAULT 100.0
google_place_id      TEXT
visit_count          INTEGER DEFAULT 0
first_visit          TEXT
last_visit           TEXT
metadata             TEXT DEFAULT '{}'
content              TEXT
cover_image          TEXT
created_at           TEXT
updated_at           TEXT
```

### wiki_orgs
```sql
id                   TEXT PK
canonical_name       TEXT NOT NULL
organization_type    TEXT
relationship_type    TEXT
role_title           TEXT
start_date           TEXT
end_date             TEXT
interaction_count    INTEGER DEFAULT 0
first_interaction    TEXT
last_interaction     TEXT
metadata             TEXT DEFAULT '{}'
content              TEXT
cover_image          TEXT
created_at           TEXT
updated_at           TEXT
```

### wiki_days
```sql
id                   TEXT PK
date                 TEXT NOT NULL UNIQUE
start_timezone       TEXT
end_timezone         TEXT
autobiography        TEXT
autobiography_sections TEXT                            -- JSON
last_edited_by       TEXT DEFAULT 'ai'                 CHECK IN ('ai','human')
act_id               TEXT                              -- FK wiki_acts
chapter_id           TEXT                              -- FK wiki_chapters
morning_baseline     REAL                              -- Body battery: 0-1 sigmoid of overnight recovery
battery_curve        TEXT                              -- JSON array of hourly battery values
cover_image          TEXT
snapshot             TEXT
created_at           TEXT
updated_at           TEXT
```

### wiki_events
```sql
id                   TEXT PK
day_id               TEXT NOT NULL                     -- FK wiki_days
start_time           TEXT NOT NULL
end_time             TEXT NOT NULL
auto_label           TEXT
auto_location        TEXT
user_label           TEXT
user_location        TEXT
user_notes           TEXT
source_ontologies    TEXT DEFAULT '[]'                 -- JSON array
is_unknown           INTEGER DEFAULT 0
is_transit           INTEGER DEFAULT 0
is_user_added        INTEGER DEFAULT 0
is_user_edited       INTEGER DEFAULT 0
embedding            BLOB                              -- 768-dim nomic-embed (for novelty scoring)
novelty_z            REAL                              -- z-scored novelty vs 12-week baseline
topics               TEXT DEFAULT '[]'                 -- JSON array of activity contexts
event_summary        TEXT                              -- 1-3 sentence factual summary
agent_action         TEXT                              -- NEW/CONTINUE/REVISE/NO_DATA
is_sleep             INTEGER DEFAULT 0
user_hidden          INTEGER DEFAULT 0                 -- soft delete
user_created         INTEGER DEFAULT 0                 -- user-created, never modified by recompute
created_at           TEXT
updated_at           TEXT
```

### wiki_years
```sql
id                   TEXT PK
year                 INTEGER NOT NULL UNIQUE
title                TEXT
description          TEXT
content              TEXT
cover_image          TEXT
created_at           TEXT
updated_at           TEXT
```

### wiki_telos
```sql
id                   TEXT PK
title                TEXT NOT NULL
description          TEXT
is_active            INTEGER DEFAULT 1
content              TEXT
cover_image          TEXT
created_at           TEXT
updated_at           TEXT
```

### wiki_acts
```sql
id                   TEXT PK
title                TEXT NOT NULL
subtitle             TEXT
description          TEXT
start_date           TEXT NOT NULL
end_date             TEXT
sort_order           INTEGER DEFAULT 0
telos_id             TEXT                              -- FK wiki_telos
themes               TEXT                              -- JSON array
metadata             TEXT DEFAULT '{}'
content              TEXT
cover_image          TEXT
location             TEXT
created_at           TEXT
updated_at           TEXT
```

### wiki_chapters
```sql
id                   TEXT PK
act_id               TEXT                              -- FK wiki_acts
title                TEXT NOT NULL
subtitle             TEXT
description          TEXT
start_date           TEXT NOT NULL
end_date             TEXT
sort_order           INTEGER DEFAULT 0
themes               TEXT                              -- JSON array
metadata             TEXT DEFAULT '{}'
content              TEXT
cover_image          TEXT
created_at           TEXT
updated_at           TEXT
```

### wiki_entity_refs (junction table)
```sql
id                   TEXT PK
entity_type          TEXT NOT NULL                     CHECK IN ('person','place','organization','thing')
entity_id            TEXT NOT NULL                     -- wiki_people.id, wiki_places.id, etc.
source_table         TEXT NOT NULL                     -- 'data_calendar_event', 'data_location_visit', etc.
source_id            TEXT NOT NULL                     -- row ID in source table
role                 TEXT                              -- 'organizer', 'attendee', 'sender', 'location', 'merchant'
confidence           REAL NOT NULL DEFAULT 1.0
resolved_by          TEXT DEFAULT 'system'             -- 'system', 'user', 'llm'
timestamp            TEXT                              -- when the interaction occurred (denormalized)
metadata             TEXT DEFAULT '{}'
created_at           TEXT
UNIQUE(entity_id, source_table, source_id, role)
-- Indexes: entity_id+timestamp, source_table+source_id, entity_type+timestamp, source_table+source_id+entity_type
```

### wiki_narrative_identity
```sql
id                   TEXT PK DEFAULT 'nar_identity_001'
content              TEXT NOT NULL DEFAULT ''
active               INTEGER NOT NULL DEFAULT 0
created_at           TEXT
updated_at           TEXT
```

### wiki_day_embeddings
```sql
id                   TEXT PK
day_date             TEXT NOT NULL
dimension            TEXT NOT NULL
embedding            BLOB NOT NULL
text_hash            TEXT NOT NULL
model                TEXT NOT NULL
created_at           TEXT
UNIQUE(day_date, dimension)
```

---

## Search

### search_embeddings
```sql
id                   TEXT PK
ontology             TEXT NOT NULL
record_id            TEXT NOT NULL
text_hash            TEXT NOT NULL
model                TEXT NOT NULL
chunk_index          INTEGER DEFAULT 0
title                TEXT
preview              TEXT
author               TEXT
timestamp            TEXT
created_at           TEXT
UNIQUE(ontology, record_id, chunk_index)
```

### search_embedding_progress
```sql
ontology             TEXT PK
last_processed_id    TEXT
last_processed_timestamp TEXT
total_embedded       INTEGER DEFAULT 0
last_run_at          TEXT
```

---

## Actions (Scheduler)

### app_actions
Universal scheduler table. Everything that runs on a schedule (or manually / via endpoint) is an action.
```sql
id                   TEXT PK
action_type          TEXT NOT NULL                     CHECK IN ('sync','agent','system')
name                 TEXT NOT NULL
cron_schedule        TEXT
enabled              INTEGER DEFAULT 1
config               TEXT DEFAULT '{}'                  -- JSON: {chat_id, source_connection_id, function_name, ...}
activation_code      TEXT                              -- Optional Python gate script
created_at           TEXT
updated_at           TEXT
```
**action_type subtypes:**
- `'sync'`   — data pipeline (fetch → transform → write), no LLM. config: `{source_connection_id, stream_name}`
- `'agent'`  — LLM agent loop with chat, instruction, optional activation gate. config: `{chat_id, trigger_token?}`
- `'system'` — hardcoded Rust job dispatched via `config.function_name` (embedding_index, trash_purge)

### app_action_runs
```sql
id                   TEXT PK
action_id            TEXT                              -- FK app_actions
status               TEXT DEFAULT 'running'            CHECK IN ('running','success','error','cancelled','skipped')
started_at           TEXT
completed_at         TEXT
records_processed    INTEGER DEFAULT 0
error                TEXT
trigger              TEXT DEFAULT 'cron'               -- 'cron' | 'manual'
parent_run_id        TEXT                              -- FK app_action_runs (self-ref, for transform chaining)
transform_stage      TEXT
created_at           TEXT
```
