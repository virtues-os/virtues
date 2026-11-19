-- 0082_threads_cleanup_and_conversations
--
-- Two names change so that "thread" means one thing.
--
-- A message or email conversation is a conversation. "Thread" now names the
-- unit the person follows (0080), so the chat.db or Gmail conversation id on
-- messages, emails and the outbox is `conversation_id`:
--
--   data_communication_message.thread_id -> conversation_id
--   data_communication_email.thread_id   -> conversation_id
--   app_message_outbox.thread_id         -> conversation_id
--
-- A thread's adding setting is `is_adding_automatic`: false (the default)
-- means only what the owner adds becomes part of the thread, true means the
-- server also adds what it is sure belongs. The column existed as
-- `auto_add_materials` with the same default and nothing read it.
--
-- Stored references to a thread use `/thread/{id}`, and a visit to one is of
-- kind `thread`. Pins and visits written as `/project/…`, `/notebook/…` or
-- `project` are rewritten so they keep resolving.
--
-- A release from before this one queries the old column names. After
-- `virtues rollback` to one of those releases, message search, the outbox
-- and thread settings fail until the box upgrades again.

ALTER TABLE data_communication_message RENAME COLUMN thread_id TO conversation_id;
ALTER TABLE data_communication_email RENAME COLUMN thread_id TO conversation_id;
ALTER TABLE app_message_outbox RENAME COLUMN thread_id TO conversation_id;

ALTER INDEX idx_social_message_thread RENAME TO idx_social_message_conversation;
ALTER INDEX idx_social_email_thread RENAME TO idx_social_email_conversation;

ALTER TABLE app_threads RENAME COLUMN auto_add_materials TO is_adding_automatic;

UPDATE app_pins SET url = '/thread/' || substr(url, length('/project/') + 1)
 WHERE url LIKE '/project/%';
UPDATE app_pins SET url = '/thread/' || substr(url, length('/notebook/') + 1)
 WHERE url LIKE '/notebook/%';
UPDATE app_thread_items SET url = '/thread/' || substr(url, length('/project/') + 1)
 WHERE url LIKE '/project/%';
UPDATE app_visits SET kind = 'thread' WHERE kind = 'project';
