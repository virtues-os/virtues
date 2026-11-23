-- What excites the owner most about Virtues.
--
-- A server asks this once in Setup, as one of five short answers, and the
-- owner can change or clear it in the app's Settings. While that server is
-- linked to an account it sends the answer here (`PUT /account/start-with`),
-- so we can read which kind of person keeps paying. It is the only thing
-- this table knows about why someone came. NULL means they have not said,
-- or they cleared it.
ALTER TABLE accounts
    ADD COLUMN IF NOT EXISTS start_with text
        CHECK (start_with IN ('days', 'patterns', 'memory', 'habit', 'ownership'));
