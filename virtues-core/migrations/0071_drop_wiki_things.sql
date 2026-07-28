-- ---------------------------------------------------------------------------
-- Drop wiki_things (docs/stories-plan.md §8, decided 2026-07-13).
--
-- Deprecated in 0033_stories_foundations.sql (comment-only at the time,
-- "drop in a later cleanup migration"): topics are universals, things were
-- particulars — particulars now accumulate as floating mentions instead of
-- their own entity table. The create/update API paths were removed in the
-- same change that added this migration; no code writes to this table.
--
-- No FK constraints reference wiki_things, so the drop itself needs no CASCADE.
-- But two tables store routes as free text and can hold `/thing/<id>`:
--
--   app_pins.url            sidebar pins
--   app_notebook_items.url  notebook membership
--
-- Those rows have to go in the same migration. Before this change a stale
-- `/thing/` row still resolved as a route *type* — the UI could label it and
-- give it an icon even if the row behind it was gone. This change removes the
-- `/thing` prefix from the frontend router as well, so such a row would render
-- as an untyped, unlabelled entry that nothing can open. Deleting the rows is
-- the honest end of the deprecation; leaving them would be a visible break.
-- ---------------------------------------------------------------------------

DELETE FROM app_pins            WHERE url LIKE '/thing/%';
DELETE FROM app_notebook_items  WHERE url LIKE '/thing/%';

DROP TABLE IF EXISTS wiki_things;
