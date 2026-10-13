-- 0048_document_contract
--
-- A page's text is markdown in a Y.Text ('markdown') or a Yjs XML tree under
-- the document contract ('tree', crates/virtues-document). Every reader and
-- writer of page text routes on `format`. `content` holds markdown either
-- way: for a tree page it is the export, written with every save, so every
-- reader of `content` keeps working.

ALTER TABLE app_pages
    ADD COLUMN format text NOT NULL DEFAULT 'markdown';

ALTER TABLE app_pages
    ADD CONSTRAINT app_pages_format_check
        CHECK (format IN ('markdown', 'tree')),
    -- A tree page's document is the page. Rebuilding it from `content`, the
    -- export, would drop what markdown cannot hold, so the row cannot be
    -- without it.
    ADD CONSTRAINT app_pages_tree_has_document
        CHECK (format = 'markdown' OR yjs_state IS NOT NULL),
    -- Articles are written by machines that write markdown text; they stay
    -- markdown until those writers move.
    ADD CONSTRAINT app_pages_tree_is_a_user_page
        CHECK (format = 'markdown' OR kind = 'page');

-- What the model has seen of a tree page: the merge base for its next edit,
-- kept here because a base must outlive the doc cache and a restart. `base`
-- hashes the seen tree with its block ids, so deleting a block changes it.
-- `tree_json` is that tree's JSON as bytes: jsonb refuses the NUL page text
-- may hold. Pruned by api::page_reads; deleted with the page.
CREATE TABLE app_page_read_bases (
    page_id    text        NOT NULL REFERENCES app_pages (id) ON DELETE CASCADE,
    base       text        NOT NULL,
    tree_json  bytea       NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (page_id, base)
);
CREATE INDEX idx_app_page_read_bases_page_recent
    ON app_page_read_bases (page_id, updated_at DESC);
CREATE INDEX idx_app_page_read_bases_updated_at
    ON app_page_read_bases (updated_at);
