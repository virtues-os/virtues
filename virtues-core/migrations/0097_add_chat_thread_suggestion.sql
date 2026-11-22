-- 0097_add_chat_thread_suggestion
--
-- Your server can suggest a thread for a chat that is in none. The title
-- call picks it, so a suggestion costs no model call of its own. It shows
-- as one line under the chat's name until you file the chat or say no.
--
-- suggested_thread_id is the open suggestion. declined_thread_ids are the
-- threads you said no to for this chat, so none of them is suggested again.

ALTER TABLE app_chats
    ADD COLUMN suggested_thread_id text REFERENCES app_threads(id) ON DELETE SET NULL,
    ADD COLUMN declined_thread_ids text[] NOT NULL DEFAULT '{}';
