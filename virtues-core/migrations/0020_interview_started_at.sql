-- 0020_interview_started_at
--
-- The narrative interview happens inside the getting-started room (one
-- thread, 2026-09-14), so the drafter needs to know where in that room's
-- transcript the interview began: everything before this instant is setup
-- talk (introductions, sources) and never material for the document.
-- Set once, by POST /api/getting-started/interview, when the person starts
-- the interview. NULL = not started, and the old standalone room
-- (chat_narrative_interview) stays the source for boxes that used it.
ALTER TABLE app_user_profile
    ADD COLUMN interview_started_at timestamptz;
