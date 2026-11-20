-- 0095_drop_suggested_questions
--
-- Suggested questions are removed: the hourly applet that wrote one question
-- for an empty new chat, the line that offered it, and its Settings switch.
-- These two tables held its runs and candidates and nothing else reads them.
-- The applet's own row in app_applets goes when the server next reconciles
-- its shipped applets, since the applet no longer ships.

DROP TABLE app_suggestions;
DROP TABLE app_suggestion_runs;
