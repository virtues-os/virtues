-- 0101_add_profile_start_with
--
-- What excites the owner most about Virtues. They choose one of
-- five short answers in Setup's Names step and can change or clear it in
-- Settings · Profile. NULL means they have not said.
--
-- It is not in the assistant's prompt. While the server is linked to an
-- account, the same one answer is sent to the account service
-- (agents/build/privacy-boundary.md).

ALTER TABLE app_user_profile
    ADD COLUMN start_with text
        CHECK (start_with IN ('days', 'patterns', 'memory', 'habit', 'ownership'));
