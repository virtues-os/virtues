-- Drop semantic entity resolution.
--
-- The graph is deterministic + user-authored. Semantic ER (LLM/NER extraction
-- of names out of prose into `er_mentions`, then fuzzy linking) is gone; the
-- deterministic resolvers — handle matching, merchant resolution, place
-- clustering — stay and are untouched by this migration.
--
-- The numbers from a real box, which decided it:
--
--   resolved_by  source                       refs
--   system       data_communication_message   128,900   deterministic (handles)
--   alias        data_communication_message       687   user-authored
--   system       data_financial_transaction       553   deterministic (merchant)
--   system       data_location_visit              448   deterministic (clustering)
--   alias        data_communication_transcription 185   ← semantic
--   alias        app_chats                          4   ← semantic
--
-- Semantic ER produced 189 of 130,777 refs — 0.14% — and even those linked only
-- via a human-written alias, never by the machine. Against that it accrued
-- 11,113 permanently-floating mentions (a review queue never once cleared),
-- 172k extraction-log rows, ~46MB, and an LLM call on every sweep. It was
-- still growing ~600 mentions/day at the time of removal.
--
-- Existing `wiki_entity_refs` rows are deliberately NOT deleted. The 876
-- alias-linked ones record real human decisions; that is authored history and
-- it stays. This only stops new semantic rows being produced and reclaims the
-- evidence tables that fed them.

DROP TABLE IF EXISTS er_mentions;
DROP TABLE IF EXISTS er_extraction_log;
