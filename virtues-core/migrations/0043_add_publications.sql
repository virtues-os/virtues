-- 0043_add_publications
--
-- What the owner has shared. A publication is a page frozen into a bundle
-- that the door serves to anyone holding its token
-- (agents/plan/publishing-plan.md). The bytes live in the bundle directory,
-- not here: the door reads that directory and has no database access, so
-- this table is the owner's record of what is out there, and revoking is
-- deleting the bundle plus stamping `revoked_at`.
--
-- `app_page_shares` predates this and is page-only, with no expiry or
-- revocation record. It stays until pages publish through here.

CREATE TABLE app_publications (
    id              TEXT        PRIMARY KEY,
    -- 128 random bits, base64url: the secret part of the link.
    token           TEXT        NOT NULL UNIQUE,
    producer_kind   TEXT        NOT NULL CHECK (producer_kind IN ('applet')),
    producer_id     TEXT        NOT NULL,
    title           TEXT        NOT NULL,
    -- sha256 of the frozen bundle, so "Update" can tell nothing changed.
    content_hash    TEXT        NOT NULL,
    size_bytes      BIGINT      NOT NULL,
    -- The link-preview card the owner opted into; NULL is no card.
    card_title      TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at      TIMESTAMPTZ,
    revoked_at      TIMESTAMPTZ,
    open_count      BIGINT      NOT NULL DEFAULT 0,
    last_opened_at  TIMESTAMPTZ
);

-- The Shared list, and "is this applet already shared".
CREATE INDEX idx_app_publications_producer ON app_publications (producer_kind, producer_id);

-- A live page's approved queries: the exact statements the owner saw, with
-- the rows they returned, on the Share sheet. The door may ask the core to
-- run these and only these, by `(publication_id, query_key)`.
CREATE TABLE app_publication_queries (
    publication_id  TEXT        NOT NULL REFERENCES app_publications (id) ON DELETE CASCADE,
    -- The key the page uses, e.g. the sha256 of its SQL text.
    query_key       TEXT        NOT NULL,
    sql             TEXT        NOT NULL,
    approved_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (publication_id, query_key)
);
