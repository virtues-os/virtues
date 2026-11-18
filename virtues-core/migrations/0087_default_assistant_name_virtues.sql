-- The assistant is called Virtues until the owner names it
-- (`assistant_profile::DEFAULT_ASSISTANT_NAME`).
--
-- 0001 inserts the one assistant profile row with the old default, so a new
-- server already holds 'Ari' when this runs. A server with no chats yet has
-- never talked to its assistant, so it takes the new name; a server whose
-- owner already talks to Ari keeps that name, since renaming someone you
-- already talk to would be a surprise.

ALTER TABLE app_assistant_profile ALTER COLUMN assistant_name SET DEFAULT 'Virtues';

UPDATE app_assistant_profile
SET assistant_name = 'Virtues'
WHERE assistant_name = 'Ari'
  AND NOT EXISTS (SELECT 1 FROM app_chats);
