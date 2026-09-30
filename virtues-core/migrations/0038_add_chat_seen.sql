-- 0038_add_chat_seen
--
-- When the person last had each chat on screen, so a reply that finished
-- while they were elsewhere reads as unread in the sidebar. A chat with an
-- assistant message newer than its seen_at is unread; a chat with no row
-- here has never been opened, so any reply in it is unread (a chat an
-- applet started, say).
--
-- Its own table, not a column on app_chats: that table's updated_at trigger
-- would stamp every chat as touched each time it was opened.
--
-- Existing chats are marked seen now, so an upgrade does not light up every
-- chat the box has ever held.
CREATE TABLE IF NOT EXISTS app_chat_seen (
    chat_id text PRIMARY KEY REFERENCES app_chats(id) ON DELETE CASCADE,
    seen_at timestamptz NOT NULL DEFAULT now()
);

INSERT INTO app_chat_seen (chat_id, seen_at)
SELECT id, now() FROM app_chats
ON CONFLICT (chat_id) DO NOTHING;
