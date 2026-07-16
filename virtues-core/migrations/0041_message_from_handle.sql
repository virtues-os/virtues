-- ─────────────────────────────────────────────────────────────────────────────
-- The normal form of a message's sender.
-- ─────────────────────────────────────────────────────────────────────────────
--
-- `from_identifier` is whatever chat.db said: "+15125550142", "nick@me.com",
-- "22395", "me". Migration 0040 gave `wiki_people` a `handles` column holding the
-- normal form of everyone we know. This is the other half of that join — the same
-- normal form, on the message — and without it resolution could not be a join at
-- all. It was an N+1: normalize one identifier in Rust, SELECT one person, repeat
-- once per message. Nothing indexable to join on meant nothing to drive the work
-- off except a time window, and a time window cannot see a backfill.
--
-- Tri-state, and every state is load-bearing:
--
--   NULL    not normalized yet        → work for the resolver's drain
--   ''      normalized to nothing     → a short code, or 'me'. Never a person.
--   '+1…'   a handle                  → joins against wiki_people.handles
--
-- The '' state is the one that is easy to miss. A work-driven resolver asks "what
-- has no result yet?" — and a bank's 2FA short code will never have one. Without a
-- way to record "attempted, and the answer is nobody", the resolver would re-attempt
-- every robot in your history every fifteen minutes, forever.

ALTER TABLE data_communication_message ADD COLUMN from_handle TEXT;

-- The join: only rows that can actually match a person.
CREATE INDEX idx_message_from_handle
    ON data_communication_message (from_handle)
    WHERE from_handle IS NOT NULL AND from_handle <> '';

-- The drain queue. Empties to nothing once the backlog is normalized, at which
-- point it costs a few pages and keeps the resolver's "any new work?" probe an
-- index scan instead of a sequential scan of every message you have ever sent.
CREATE INDEX idx_message_handle_pending
    ON data_communication_message (id)
    WHERE from_handle IS NULL;

-- The resolver's anti-join looks refs up by source, which `idx_entity_refs_source`
-- (source_table, source_id) already serves. No new index needed here.
