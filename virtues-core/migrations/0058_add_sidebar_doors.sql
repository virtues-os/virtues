-- 0058_add_sidebar_doors
--
-- The owner's arrangement of the sidebar's doors (New chat, Pages, Tasks,
-- Projects, Applets, Wiki): the order they put them in and the ones they
-- hid, as {"order": [...], "hidden": [...]} of door ids. NULL is the
-- shipped arrangement. A door id this box no longer knows is ignored when
-- read, and a door it has never seen takes its shipped place, so the column
-- never needs migrating when the set of doors changes.

ALTER TABLE app_user_profile ADD COLUMN sidebar_doors jsonb;
