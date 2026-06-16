-- 0003 — App shell.
--
-- Singletons (user profile, assistant profile, drive usage), workspace shape
-- (spaces, views, sidebar items, pins), drive file tree, API usage counters.

-- ---------------------------------------------------------------------------
-- User profile (singleton — one owner per appliance)
-- ---------------------------------------------------------------------------
CREATE TABLE app_user_profile (
    id                    TEXT PRIMARY KEY DEFAULT '00000000-0000-0000-0000-000000000001',
    full_name             TEXT,
    preferred_name        TEXT,
    birth_date            DATE,
    height_cm             DOUBLE PRECISION,
    weight_kg             DOUBLE PRECISION,
    ethnicity             TEXT,
    occupation            TEXT,
    employer              TEXT,
    home_place_id         TEXT,  -- soft reference into wiki_places (no FK)
    theme                 TEXT DEFAULT 'light',
    crux                  TEXT,
    technology_vision     TEXT,
    pain_point_primary    TEXT,
    pain_point_secondary  TEXT,
    excited_features      JSONB NOT NULL DEFAULT '[]'::jsonb,
    -- Money flow flags. Auto-top-up is on by default per the locked v3
    -- economic model. `auto_topup_failures_24h` is the circuit-breaker
    -- counter — when it hits 3, the box stops attempting auto-top-up and
    -- surfaces a "your card keeps failing" notice. The counter is rolled
    -- by the renew cron daily.
    auto_topup_enabled        BOOLEAN NOT NULL DEFAULT TRUE,
    auto_topup_failures_24h   INTEGER NOT NULL DEFAULT 0,
    auto_topup_disabled_at    TIMESTAMPTZ,                       -- set when breaker trips; cleared when user re-enables
    server_status         TEXT NOT NULL DEFAULT 'provisioning'
                              CHECK (server_status IN ('provisioning', 'migrating', 'ready')),
    timezone              TEXT,
    -- Onboarding lifecycle: new (just claimed) → onboarding (in the relational
    -- first-chat / next-wins flow) → active (using the box day-to-day) →
    -- complete (checklist fully done). Written by chat.rs (first-message flip)
    -- and tools/executor.rs (name captured → active).
    onboarding_status     TEXT NOT NULL DEFAULT 'new'
                              CHECK (onboarding_status IN ('new', 'onboarding', 'active', 'complete')),
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT user_profile_singleton CHECK (id = '00000000-0000-0000-0000-000000000001')
);
CREATE INDEX idx_app_user_profile_server_status ON app_user_profile(server_status);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON app_user_profile
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

-- ---------------------------------------------------------------------------
-- Assistant profile (singleton — one assistant config per appliance)
-- ---------------------------------------------------------------------------
CREATE TABLE app_assistant_profile (
    id                    TEXT PRIMARY KEY DEFAULT '00000000-0000-0000-0000-000000000001',
    assistant_name        TEXT DEFAULT 'Ari',
    default_agent_id      TEXT DEFAULT 'agent',
    default_model_id      TEXT DEFAULT 'anthropic/claude-sonnet-4-20250514',
    background_model_id   TEXT DEFAULT 'cerebras/llama-3.3-70b',
    chat_model_id         TEXT,
    lite_model_id         TEXT,
    reasoning_model_id    TEXT,
    coding_model_id       TEXT,
    embedding_model_id    TEXT DEFAULT 'bge-m3',
    enabled_tools         JSONB NOT NULL DEFAULT
        '{"web_search": true, "virtues_query_ontology": true, "virtues_semantic_search": true}'::jsonb,
    ui_preferences        JSONB NOT NULL DEFAULT
        '{"contextIndicator": {"alwaysVisible": false, "showThreshold": 70}}'::jsonb,
    persona               TEXT DEFAULT 'standard',
    personas              JSONB,
    memory                JSONB,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT assistant_profile_singleton CHECK (id = '00000000-0000-0000-0000-000000000001')
);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON app_assistant_profile
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

-- ---------------------------------------------------------------------------
-- Namespaces (backend routing for entity types)
-- ---------------------------------------------------------------------------
CREATE TABLE app_namespaces (
    name            TEXT PRIMARY KEY,            -- 'person', 'drive', 'virtues'
    backend         TEXT NOT NULL,               -- 'postgres', 'filesystem', 's3', 'none'
    backend_config  JSONB,
    is_entity       BOOLEAN NOT NULL DEFAULT FALSE,
    is_system       BOOLEAN NOT NULL DEFAULT FALSE,
    icon            TEXT,
    label           TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ---------------------------------------------------------------------------
-- Spaces and views (workspace shell)
-- ---------------------------------------------------------------------------
CREATE TABLE app_spaces (
    id                     TEXT PRIMARY KEY,
    name                   TEXT NOT NULL,
    icon                   TEXT,
    is_system              BOOLEAN NOT NULL DEFAULT FALSE,
    sort_order             INTEGER NOT NULL DEFAULT 0,
    theme_id               TEXT NOT NULL DEFAULT 'tatooine',
    accent_color           TEXT,
    vectorize              BOOLEAN NOT NULL DEFAULT FALSE,
    active_tab_state_json  JSONB,
    created_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at             TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON app_spaces
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

CREATE TABLE app_views (
    id              TEXT PRIMARY KEY,
    space_id        TEXT NOT NULL REFERENCES app_spaces(id) ON DELETE CASCADE,
    parent_view_id  TEXT REFERENCES app_views(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    icon            TEXT,
    sort_order      INTEGER NOT NULL DEFAULT 0,
    view_type       TEXT NOT NULL CHECK (view_type IN ('manual', 'smart')),
    query_config    JSONB,
    is_system       BOOLEAN NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_views_space  ON app_views(space_id, sort_order);
CREATE INDEX idx_views_parent ON app_views(parent_view_id);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON app_views
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

CREATE TABLE app_space_items (
    id          BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    view_id     TEXT REFERENCES app_views(id)  ON DELETE CASCADE,
    space_id    TEXT REFERENCES app_spaces(id) ON DELETE CASCADE,
    url         TEXT NOT NULL,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (
        (view_id IS NOT NULL AND space_id IS NULL) OR
        (view_id IS NULL AND space_id IS NOT NULL)
    )
);
CREATE INDEX idx_space_items_view  ON app_space_items(view_id,  sort_order);
CREATE INDEX idx_space_items_space ON app_space_items(space_id, sort_order);
CREATE INDEX idx_space_items_url   ON app_space_items(url);
CREATE UNIQUE INDEX idx_space_items_view_url  ON app_space_items(view_id, url)  WHERE view_id  IS NOT NULL;
CREATE UNIQUE INDEX idx_space_items_space_url ON app_space_items(space_id, url) WHERE space_id IS NOT NULL;

-- ---------------------------------------------------------------------------
-- Pins (sidebar/launcher quick access)
-- ---------------------------------------------------------------------------
CREATE TABLE app_pins (
    id          TEXT PRIMARY KEY,
    url         TEXT NOT NULL UNIQUE,
    label       TEXT,
    icon        TEXT,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    pinned_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_app_pins_sort ON app_pins(sort_order, pinned_at DESC);

-- ---------------------------------------------------------------------------
-- Drive file tree
-- ---------------------------------------------------------------------------
CREATE TABLE app_drive_files (
    id          TEXT PRIMARY KEY,
    path        TEXT NOT NULL UNIQUE,
    filename    TEXT NOT NULL,
    mime_type   TEXT,
    size_bytes  BIGINT NOT NULL CHECK (size_bytes >= 0),
    parent_id   TEXT REFERENCES app_drive_files(id) ON DELETE CASCADE,
    is_folder   BOOLEAN NOT NULL DEFAULT FALSE,
    sha256_hash TEXT,
    deleted_at  TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_app_drive_files_path        ON app_drive_files(path);
CREATE INDEX idx_app_drive_files_parent      ON app_drive_files(parent_id) WHERE parent_id IS NOT NULL;
CREATE INDEX idx_app_drive_files_folder      ON app_drive_files(parent_id, is_folder);
CREATE INDEX idx_app_drive_files_deleted_at  ON app_drive_files(deleted_at);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON app_drive_files
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

CREATE TABLE app_drive_usage (
    id                TEXT PRIMARY KEY DEFAULT '00000000-0000-0000-0000-000000000001',
    drive_bytes       BIGINT NOT NULL DEFAULT 0 CHECK (drive_bytes >= 0),
    data_lake_bytes   BIGINT NOT NULL DEFAULT 0 CHECK (data_lake_bytes >= 0),
    total_bytes       BIGINT NOT NULL DEFAULT 0 CHECK (total_bytes >= 0),
    file_count        BIGINT NOT NULL DEFAULT 0 CHECK (file_count >= 0),
    folder_count      BIGINT NOT NULL DEFAULT 0 CHECK (folder_count >= 0),
    quota_bytes       BIGINT NOT NULL DEFAULT 107374182400,  -- 100 GiB
    warning_80_sent   BOOLEAN NOT NULL DEFAULT FALSE,
    warning_90_sent   BOOLEAN NOT NULL DEFAULT FALSE,
    warning_100_sent  BOOLEAN NOT NULL DEFAULT FALSE,
    last_scan_at      TIMESTAMPTZ,
    last_scan_bytes   BIGINT,
    trash_bytes       BIGINT NOT NULL DEFAULT 0,
    trash_count       BIGINT NOT NULL DEFAULT 0,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT drive_usage_singleton CHECK (id = '00000000-0000-0000-0000-000000000001')
);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON app_drive_usage
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

-- ---------------------------------------------------------------------------
-- API usage / rate-limit counters
-- ---------------------------------------------------------------------------
CREATE TABLE app_api_usage (
    id                  TEXT PRIMARY KEY,
    endpoint            TEXT NOT NULL,
    day_bucket          DATE NOT NULL,
    request_count       BIGINT NOT NULL DEFAULT 0 CHECK (request_count >= 0),
    token_count         BIGINT NOT NULL DEFAULT 0 CHECK (token_count   >= 0),
    input_tokens        BIGINT NOT NULL DEFAULT 0 CHECK (input_tokens  >= 0),
    output_tokens       BIGINT NOT NULL DEFAULT 0 CHECK (output_tokens >= 0),
    estimated_cost_usd  DOUBLE PRECISION NOT NULL DEFAULT 0 CHECK (estimated_cost_usd >= 0),
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(endpoint, day_bucket)
);
CREATE INDEX idx_api_usage_day          ON app_api_usage(day_bucket DESC);
CREATE INDEX idx_api_usage_endpoint_day ON app_api_usage(endpoint, day_bucket);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON app_api_usage
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

CREATE TABLE app_usage_limits (
    service        TEXT PRIMARY KEY,
    monthly_limit  BIGINT NOT NULL,
    unit           TEXT NOT NULL DEFAULT 'requests',
    limit_type     TEXT NOT NULL DEFAULT 'hard' CHECK (limit_type IN ('hard', 'soft')),
    enabled        BOOLEAN NOT NULL DEFAULT TRUE,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_usage_limits_enabled ON app_usage_limits(service) WHERE enabled;
CREATE TRIGGER set_updated_at BEFORE UPDATE ON app_usage_limits
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

-- ---------------------------------------------------------------------------
-- Singleton seed rows.
--
-- The three singleton tables above each carry a CHECK forcing
-- id = '00000000-0000-0000-0000-000000000001'. Several handlers do
-- `.fetch_one(...)` on them at boot (profile, assistant profile, drive
-- usage, internal status checks). Insert the rows here so the migration
-- itself bootstraps a usable database — no post-migration Rust hook
-- needed. ON CONFLICT DO NOTHING keeps it idempotent.
-- ---------------------------------------------------------------------------

INSERT INTO app_user_profile (id) VALUES ('00000000-0000-0000-0000-000000000001')
    ON CONFLICT (id) DO NOTHING;

INSERT INTO app_assistant_profile (id) VALUES ('00000000-0000-0000-0000-000000000001')
    ON CONFLICT (id) DO NOTHING;

INSERT INTO app_drive_usage (id) VALUES ('00000000-0000-0000-0000-000000000001')
    ON CONFLICT (id) DO NOTHING;
