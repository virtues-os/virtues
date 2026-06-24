-- 0005 — Chats and pages.
--
-- Conversational interface (chats + messages + per-chat usage + per-entity
-- edit permissions) and the wiki-style pages surface (pages + Yjs version
-- history + public share tokens).

-- ---------------------------------------------------------------------------
-- Chats
-- ---------------------------------------------------------------------------
CREATE TABLE app_chats (
    id                    TEXT PRIMARY KEY,
    title                 TEXT NOT NULL,
    message_count         BIGINT NOT NULL DEFAULT 0,
    trace                 TEXT,
    conversation_summary  TEXT,
    summary_up_to_index   BIGINT NOT NULL DEFAULT 0,
    summary_version       BIGINT NOT NULL DEFAULT 0,
    last_compacted_at     TIMESTAMPTZ,
    icon                  TEXT,
    action_instruction    TEXT,
    -- The Space (room) this chat lives in. At most one; cleared, not deleted,
    -- if the Space is removed. See app_spaces in 0003.
    space_id              TEXT REFERENCES app_spaces(id) ON DELETE SET NULL,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_chats_updated ON app_chats(updated_at DESC);
CREATE INDEX idx_chats_space   ON app_chats(space_id) WHERE space_id IS NOT NULL;
CREATE TRIGGER set_updated_at BEFORE UPDATE ON app_chats
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

CREATE TABLE app_chat_messages (
    id                 TEXT PRIMARY KEY,
    chat_id            TEXT NOT NULL REFERENCES app_chats(id) ON DELETE CASCADE,
    role               TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system', 'checkpoint')),
    content            TEXT NOT NULL,
    model              TEXT,
    provider           TEXT,
    agent_id           TEXT,
    reasoning          TEXT,
    tool_calls         JSONB,
    intent             JSONB,
    subject            TEXT,
    thought_signature  TEXT,
    parts              JSONB,
    sequence_num       BIGINT NOT NULL,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(chat_id, sequence_num)
);
CREATE INDEX idx_chat_messages_chat    ON app_chat_messages(chat_id, sequence_num);
CREATE INDEX idx_chat_messages_role    ON app_chat_messages(chat_id, role);
CREATE INDEX idx_chat_messages_created ON app_chat_messages(created_at DESC);

CREATE TABLE app_chat_usage (
    id                  TEXT PRIMARY KEY,
    chat_id             TEXT NOT NULL REFERENCES app_chats(id) ON DELETE CASCADE,
    model               TEXT NOT NULL,
    input_tokens        BIGINT NOT NULL DEFAULT 0,
    output_tokens       BIGINT NOT NULL DEFAULT 0,
    reasoning_tokens    BIGINT NOT NULL DEFAULT 0,
    cache_read_tokens   BIGINT NOT NULL DEFAULT 0,
    cache_write_tokens  BIGINT NOT NULL DEFAULT 0,
    estimated_cost_usd  DOUBLE PRECISION NOT NULL DEFAULT 0,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(chat_id, model)
);
CREATE INDEX idx_chat_usage_chat ON app_chat_usage(chat_id);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON app_chat_usage
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

CREATE TABLE app_chat_edit_permissions (
    id            TEXT PRIMARY KEY,
    chat_id       TEXT NOT NULL REFERENCES app_chats(id) ON DELETE CASCADE,
    entity_id     TEXT NOT NULL,
    entity_type   TEXT NOT NULL,
    entity_title  TEXT,
    granted_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(chat_id, entity_id)
);
CREATE INDEX idx_chat_edit_permissions_chat   ON app_chat_edit_permissions(chat_id);
CREATE INDEX idx_chat_edit_permissions_entity ON app_chat_edit_permissions(entity_id);

-- ---------------------------------------------------------------------------
-- Pages (markdown + Yjs CRDT)
-- ---------------------------------------------------------------------------
CREATE TABLE app_pages (
    id          TEXT PRIMARY KEY,
    title       TEXT NOT NULL,
    content     TEXT NOT NULL DEFAULT '',
    icon        TEXT,
    cover_url   TEXT,
    tags        JSONB NOT NULL DEFAULT '[]'::jsonb,
    yjs_state   BYTEA,
    date        DATE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_pages_updated ON app_pages(updated_at DESC);
CREATE INDEX idx_pages_title   ON app_pages(title);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON app_pages
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

CREATE TABLE app_page_versions (
    id               TEXT PRIMARY KEY,
    page_id          TEXT NOT NULL REFERENCES app_pages(id) ON DELETE CASCADE,
    version_number   BIGINT NOT NULL,
    yjs_snapshot     BYTEA,
    content_preview  TEXT,
    description      TEXT,
    created_by       TEXT NOT NULL DEFAULT 'user',
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(page_id, version_number)
);
CREATE INDEX idx_page_versions_page ON app_page_versions(page_id, version_number DESC);

CREATE TABLE app_page_shares (
    id          TEXT PRIMARY KEY,
    page_id     TEXT NOT NULL UNIQUE REFERENCES app_pages(id) ON DELETE CASCADE,
    token       TEXT NOT NULL UNIQUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_page_shares_token ON app_page_shares(token);
