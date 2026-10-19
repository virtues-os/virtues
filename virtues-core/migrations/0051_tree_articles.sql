-- 0051_tree_articles
--
-- Articles (kind = 'article': wiki articles and day pages) can be block
-- pages. Their writers still send markdown; the server converts it and
-- writes the difference as block edits. An article nobody has opened keeps
-- the markdown it was written in, and becomes blocks when first opened.

ALTER TABLE app_pages DROP CONSTRAINT app_pages_tree_is_a_user_page;

-- Moving a page between formats is not an edit, so it keeps its place in
-- every list ordered by updated_at. Turning the trigger off for the move
-- held every other write to the table while it ran.
CREATE FUNCTION tg_app_pages_set_updated_at() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF NEW.format IS DISTINCT FROM OLD.format THEN
        NEW.updated_at = OLD.updated_at;
    ELSE
        NEW.updated_at = now();
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER set_updated_at ON app_pages;
CREATE TRIGGER set_updated_at BEFORE UPDATE ON app_pages
    FOR EACH ROW EXECUTE FUNCTION tg_app_pages_set_updated_at();

-- Which copy of a block page's document this is. A device keeps its copy
-- of the page under it, so a copy of an earlier document (the page put
-- back from a backup and moved to blocks again) is never merged into this
-- one. NULL for a markdown page, and for a block page from before this.
ALTER TABLE app_pages ADD COLUMN tree_lineage text;
