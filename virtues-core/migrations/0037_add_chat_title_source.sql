-- 0037_add_chat_title_source
--
-- Who wrote a chat's title, so the generator knows when to leave it alone.
--
--   seed       the first user message, cut to fifty characters
--   generated  the model's first title, after the first exchange
--   settled    the model's one re-title, once the chat has run about six
--              turns; no more automatic titles after this
--   user       renamed by hand; the generator never touches it again
--
-- Without it the client asked for a title on the first send after every
-- open, and the server wrote whatever came back, so a hand-picked name
-- lasted until the chat was next used.
--
-- Existing chats are backfilled as settled: a hand rename and a generated
-- title look the same in the old rows, and re-titling a chat the person
-- named is the bug this column fixes. The backfill is the ADD COLUMN
-- default, not an UPDATE: app_chats has an updated_at trigger, and an
-- UPDATE would stamp every chat as moved just now and put all of them
-- under Today.
ALTER TABLE app_chats ADD COLUMN IF NOT EXISTS title_source text NOT NULL DEFAULT 'settled'
    CHECK (title_source IN ('seed', 'generated', 'settled', 'user'));

ALTER TABLE app_chats ALTER COLUMN title_source SET DEFAULT 'seed';
