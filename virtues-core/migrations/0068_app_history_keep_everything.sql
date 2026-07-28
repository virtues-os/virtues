-- History is kept, not pruned.
--
-- 0067 shipped a `prune_app_history()` that bounded the log at 90 days and 20k
-- rows. It isn't worth the loss: a visit row is ~100 bytes, and someone
-- navigating 500 times a day writes about 18MB a year. Against a box that
-- already holds this person's mail, photos and finances, that is nothing — and
-- a history that quietly forgets last year is worse than one that costs a few
-- megabytes.
--
-- What was actually being protected was read speed, and pruning was the wrong
-- lever for it: the recents query now scans a bounded recent window instead of
-- the whole table (see api/history.rs), so it stays flat no matter how far the
-- archive goes back.

DROP FUNCTION IF EXISTS prune_app_history();
