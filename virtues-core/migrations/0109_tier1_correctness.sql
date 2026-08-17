-- Tier 1: the correctness and durability fixes, before the squash.
--
-- ## 1. Deleting a user would have deleted their authored applets
--
-- `app_device.user_id -> app_auth_user` was ON DELETE CASCADE, and
-- `app_applets.device_id -> app_device` was ON DELETE CASCADE too. So one
-- `DELETE FROM app_auth_user` would take every device, and with them every
-- chat-authored applet and its schema migrations. An applet is the owner's own
-- automation; it is not re-derivable from anything.
--
-- Unfired today only because nothing hard-deletes a device — revocation is soft
-- (`revoked_at`). Which is exactly why it is worth fixing now: the CASCADE buys
-- nothing and is waiting for the first "forget this device" feature.
--
-- RESTRICT rather than SET NULL: a future delete should fail loudly and make
-- whoever wrote it decide, not silently orphan rows or silently eat work.
-- `app_sudo_request` keeps its CASCADE — those rows are ephemeral by design.
--
-- ## 2. The hottest question in entity resolution was a sequential scan
--
-- `people.rs` asks "who owns this address" with `emails @> to_jsonb($1::text)`
-- for every unresolved sender. `wiki_people` had GIN indexes on `aliases` and
-- `handles` — and none on `emails`.
--
-- ## 3. Autovacuum on the tables that churn
--
-- The search tables are not append-only: re-embedding a record DELETEs its
-- chunks and postings and re-INSERTs them. At the default 20% scale factor a
-- large postings table waits for millions of dead tuples before autovacuum
-- wakes. This box runs for years with nobody to notice, on one NVMe, so the
-- thresholds are set where the churn actually is.
--
-- ## 4. One vector geometry
--
-- `search_vectors.embedding` was `vector(256)`, `search_topic_cache.embedding`
-- `vector(256)`, `app_notebooks.centroid` `halfvec(384)` — three columns, two
-- type families, two widths, for one embedding space. `app_notebooks.centroid`'s
-- own comment claimed "Same space as search_vectors — halfvec(256)" while the
-- column said otherwise, and 0060's header records this exact drift happening
-- once already: "Every centroid write failed the dimension check."
--
-- Startup resizes all three together, so a fresh box converges — but it should
-- not have to start crooked. Declared here at one width and one family.

-- ── 1. Stop the cascade that eats authored applets ──────────────────────────
ALTER TABLE app_applets DROP CONSTRAINT app_applets_device_id_fkey;
ALTER TABLE app_applets ADD CONSTRAINT app_applets_device_id_fkey
    FOREIGN KEY (device_id) REFERENCES app_device(id) ON DELETE RESTRICT;

ALTER TABLE app_device DROP CONSTRAINT app_device_user_id_fkey;
ALTER TABLE app_device ADD CONSTRAINT app_device_user_id_fkey
    FOREIGN KEY (user_id) REFERENCES app_auth_user(id) ON DELETE RESTRICT;

-- ── 2. The missing index ────────────────────────────────────────────────────
CREATE INDEX IF NOT EXISTS idx_wiki_people_emails ON wiki_people USING gin (emails);

-- ── 3. Autovacuum where the churn is ────────────────────────────────────────
ALTER TABLE search_bm25_postings SET (
    autovacuum_vacuum_scale_factor = 0.02,
    autovacuum_vacuum_threshold    = 5000,
    autovacuum_vacuum_cost_delay   = 2
);
ALTER TABLE search_embeddings SET (autovacuum_vacuum_scale_factor = 0.05);
ALTER TABLE search_vectors    SET (autovacuum_vacuum_scale_factor = 0.05);

-- ── 4. One vector width and one type family ─────────────────────────────────
-- Empty on any box that has not indexed yet; on one that has, startup's resize
-- has already converged them and these are no-ops.
ALTER TABLE app_notebooks ALTER COLUMN centroid TYPE halfvec(256)
    USING centroid::halfvec(256);
COMMENT ON COLUMN app_notebooks.centroid IS
    'Mean of the notebook members'' embeddings. Same space as search_vectors — halfvec(256). Re-derivable: NULL it and the next magnet run rebuilds it.';

-- ── 5. One write-only table ─────────────────────────────────────────────────
-- Three tables have exactly one writer and no reader anywhere — no SELECT in
-- any .rs, no compiled query in .sqlx, no API, no UI. Only one of them is
-- actually dead, and the difference is worth writing down, because "nothing
-- reads it" is a weaker fact than it looks.
--
-- `search_embedding_progress` IS dead. Its whole purpose was resumable
-- indexing, and the resume cursor `last_processed_id` is bound to the literal
-- `''` on every write — so the feature it exists for was never built, and the
-- row carries a counter nobody has ever looked at. Truncated wholesale by
-- reindex. Nothing is lost by removing it.
--
-- `app_applet_package` is NOT dead — it is accumulating. It records where an
-- imported applet came from and at which commit, and its own comment says that
-- is "most of why packages are worth having as a unit at all". The reader
-- ("is there a newer version") is unbuilt, but the provenance it is quietly
-- collecting is exactly what that reader will need, and it can only be
-- collected at import time. Dropping it would throw away the history of every
-- applet imported before the feature ships.
--
-- `app_auth_event_archive` is NOT dead either — it is a security audit trail.
-- The sweeper moves auth events here after 90 days so the live table stays
-- small. Dropping it would turn that move into a permanent delete, which is a
-- reduction in what the box can account for, decided on the grounds that we
-- have not written the viewer yet. Wrong trade for an audit log.
DROP TABLE IF EXISTS search_embedding_progress;
