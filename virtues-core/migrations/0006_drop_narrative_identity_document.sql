-- The narrative-identity document moves out of this table and into the wiki:
-- a real article (subject_type 'narrative_identity', singleton-indexed since
-- 0001) whose prose lives in its page — editor, history and marginalia
-- included. `document` was write-only here (drafted, never read back; the UI
-- showed the draft from the POST response), and `active` was read by nothing.
-- What remains is the apparatus half: `content`, the distilled core carried
-- into every chat, plus `drafted_at`.
--
-- No data migration: no fielded box has rows in this table (the core has been
-- the empty string in every prompt since it shipped — see chat.rs), and dev
-- drafts regenerate from the saved interview answers.
ALTER TABLE wiki_narrative_identity
    DROP COLUMN IF EXISTS document,
    DROP COLUMN IF EXISTS active;
