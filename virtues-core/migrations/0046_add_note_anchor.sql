-- 0046_add_note_anchor
--
-- A note can sit beside the passage it is about. On a day page the owner
-- writes in the margin beside a sentence, and the page draws the note there,
-- bracketing that sentence. The anchor holds the sentence's own words (which
-- survive a re-render; an index alone would not) and, when the note was
-- written on one sentence of a paragraph, the sentence's position:
--
--   {"quote": "<the sentence as written>", "sentence": 2}
--
-- Nullable: every note written before this, and every note on another kind
-- of subject, has no passage.

ALTER TABLE wiki_notes
    ADD COLUMN anchor jsonb CHECK (anchor IS NULL OR jsonb_typeof(anchor) = 'object');
