-- 0040_add_chat_message_span
--
-- How long the turn that wrote an assistant row took: from the box
-- receiving the request to the row being written. The thinking block shows
-- it as "Worked for".
--
-- The client used to measure it itself, and after a reload fell back to the
-- gap between the question's row and the answer's. That gap is wrong for a
-- regenerated answer (the question's row is not rewritten, so an answer
-- regenerated an hour later read "Worked for 60m") and for any second line
-- after one question.
--
-- Null on user rows, and on assistant rows from before this column where the
-- span cannot be recovered. Existing assistant rows are backfilled from the
-- question directly before them when that gap is under thirty minutes; a
-- longer gap is a regenerate or a room speaking later, not a turn's length.
ALTER TABLE app_chat_messages
    ADD COLUMN IF NOT EXISTS started_at timestamptz,
    ADD COLUMN IF NOT EXISTS ended_at timestamptz;

UPDATE app_chat_messages m
SET started_at = p.prev_created_at,
    ended_at = m.created_at
FROM (
    SELECT id,
           LAG(role) OVER w AS prev_role,
           LAG(created_at) OVER w AS prev_created_at
    FROM app_chat_messages
    WINDOW w AS (PARTITION BY chat_id ORDER BY sequence_num)
) p
WHERE m.id = p.id
  AND m.role = 'assistant'
  AND p.prev_role = 'user'
  AND m.created_at >= p.prev_created_at
  AND m.created_at - p.prev_created_at < interval '30 minutes';
