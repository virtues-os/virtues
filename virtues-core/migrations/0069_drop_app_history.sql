-- Drop app_history.
--
-- Recents is gone from the sidebar, and it was the table's only reader. What
-- that leaves, if the table stays, is the worst of both: a complete,
-- permanent, deliberately-unpruned log of every page the owner has looked at,
-- still being written on every navigation, with nothing that ever reads it.
--
-- On an appliance whose entire premise is that the owner's life stays theirs,
-- a recorder running for no consumer is not a neutral leftover. Keeping it
-- "in case history comes back" would mean choosing to retain that record on
-- the strength of a feature nobody has committed to building. If a history
-- feature returns it can return deliberately, with its own retention and
-- clear-history story decided at that point — which is a better conversation
-- to have with an empty table than with two years of accumulated browsing.
--
-- Recents lasted two days (0067 introduced it, 0068 removed its pruning). It
-- was removed because it filled with duplicates of the sidebar's own nav rows:
-- clicking "Pages" recorded a visit to /pages, so the sidebar was the largest
-- contributor to its own history list. That is a design problem, not a schema
-- problem, but it is the reason there is no reader left.
--
-- 0068's function drop is included for boxes that never applied it.

DROP FUNCTION IF EXISTS prune_app_history();

DROP TABLE IF EXISTS app_history;
