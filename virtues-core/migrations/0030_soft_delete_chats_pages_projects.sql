-- 0030_soft_delete_chats_pages_projects
--
-- Delete stops being permanent. A chat, a page or a project the owner deletes
-- is stamped `deleted_at` and hidden; it sits in Recently deleted for 30 days
-- (`api::trash::TRASH_RETENTION_DAYS`) with a restore, and the sweeper purges
-- it after that. The same shape `app_drive_files.deleted_at` has had since the
-- squash — this extends it to the three things a person makes in the app.
--
-- Why: this is a record of a life, and the worst failure it can have is the one
-- it already had — a real interview gone on one misclick of a hard delete that
-- said "cannot be undone". Nothing the owner made should be one click from
-- unrecoverable.
--
-- `app_projects.archived_at` stays. It is a different state — "finished, kept,
-- out of the working view" — and the archive slice is what will start writing
-- it. A trashed project is one the owner removed; an archived one is one they
-- closed. Two columns because they are two answers.
--
-- The filter is `deleted_at IS NULL`, spelled out in every listing, fetch and
-- search query. `sqlx::query` is untyped, so a listing that forgets the
-- predicate shows deleted rows silently — the sweep that landed with this
-- migration is in `api::trash`'s module doc.

ALTER TABLE app_chats    ADD COLUMN deleted_at TIMESTAMPTZ;
ALTER TABLE app_pages    ADD COLUMN deleted_at TIMESTAMPTZ;
ALTER TABLE app_projects ADD COLUMN deleted_at TIMESTAMPTZ;

-- Partial: the trash listing and the purge read only the stamped rows, and
-- those are a handful. The live-row filter is served by the tables' existing
-- indexes plus a cheap NULL test.
CREATE INDEX idx_app_chats_deleted_at    ON app_chats    (deleted_at) WHERE deleted_at IS NOT NULL;
CREATE INDEX idx_app_pages_deleted_at    ON app_pages    (deleted_at) WHERE deleted_at IS NOT NULL;
CREATE INDEX idx_app_projects_deleted_at ON app_projects (deleted_at) WHERE deleted_at IS NOT NULL;
