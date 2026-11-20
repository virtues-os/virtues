-- A deletion the owner never allowed goes with its chat.
--
-- 0093 made a deletion outlive the chat it was asked in, because one that
-- ran keeps tombstones that only its page in Settings can lift. A proposal
-- that was never allowed has no tombstones, and its stored filter can name a
-- person or a place, so nothing needs it once its chat is gone. This removes
-- it when the chat row is deleted (emptied from Recently deleted, or deleted
-- for good), before the foreign key clears the chat on the deletions that
-- remain.
CREATE FUNCTION app_drop_unallowed_deletions() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    DELETE FROM app_deletions WHERE chat_id = OLD.id AND approved_at IS NULL;
    RETURN OLD;
END;
$$;

CREATE TRIGGER drop_unallowed_deletions
    BEFORE DELETE ON app_chats
    FOR EACH ROW EXECUTE FUNCTION app_drop_unallowed_deletions();
