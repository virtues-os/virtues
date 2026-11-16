-- 0079_index_chat_list
--
-- The chat list sorts every chat by its newest message before it takes the
-- first 25. Without an index on (chat_id, created_at), each chat's
-- MAX(created_at) visited every row of that chat, so the list slowed with the
-- total number of messages. It runs on launch, on focus and after every turn.
--
-- The list also leaves out chats that belong to a task, which looked up
-- app_tasks by chat_id with no index.
CREATE INDEX IF NOT EXISTS idx_chat_messages_chat_created
    ON app_chat_messages (chat_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_app_tasks_chat
    ON app_tasks (chat_id)
    WHERE chat_id IS NOT NULL;
