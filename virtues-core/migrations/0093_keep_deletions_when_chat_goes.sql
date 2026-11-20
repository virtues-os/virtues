-- A deletion outlives the chat it was asked in.
--
-- Settings · Deleted records lists every deletion, and its page is the one
-- place to let a deletion's records be collected again. With the chat as an
-- owner of the row, deleting that chat (or emptying it from Recently deleted
-- after 30 days) removed the deletion, while its tombstones stayed: the
-- records stayed refused, and nothing in the app could name them or lift
-- them. Now the deletion stays and only loses its link to the chat.
ALTER TABLE app_deletions DROP CONSTRAINT app_deletions_chat_id_fkey;
ALTER TABLE app_deletions
    ADD CONSTRAINT app_deletions_chat_id_fkey
    FOREIGN KEY (chat_id) REFERENCES app_chats(id) ON DELETE SET NULL;

COMMENT ON COLUMN app_deletions.chat_id IS
    'The chat that proposed it. NULL for one made in Query, and once that chat is deleted.';
