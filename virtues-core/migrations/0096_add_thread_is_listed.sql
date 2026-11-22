-- 0096_add_thread_is_listed
--
-- A thread is listed or unlisted. A listed thread is one you keep in view,
-- with its own row in the sidebar. An unlisted thread holds a lesser concern
-- (the car, a renewal): it has tasks, chats and pages like any thread and
-- stays out of the sidebar until you list it. Every existing thread is
-- listed.

ALTER TABLE app_threads ADD COLUMN is_listed boolean NOT NULL DEFAULT true;
