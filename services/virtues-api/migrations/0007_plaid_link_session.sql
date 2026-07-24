-- Ephemeral Plaid Hosted Link session state.
--
-- Hosted Link hands the public_token back out-of-band: Plaid redirects the
-- browser to `hosted_link.completion_redirect_uri` carrying NOTHING, and the
-- integration then polls `/link/token/get` with the *link_token* to learn what
-- happened. So the callback needs the link_token that `/plaid/start` created —
-- and the `state` blob it was going to bounce back to the box.
--
-- None of that can ride in the completion URI: Plaid requires that URI to be
-- registered in the dashboard and matched exactly, so it cannot carry a
-- per-session query param. The session id therefore travels in a first-party
-- `Set-Cookie` on auth.virtues.com (SameSite=Lax survives Plaid's top-level
-- redirect back), and the rest of the session lives here.
--
-- Rows are single-use (deleted on read) and expire with the Hosted Link URL.
-- `put_link_session` opportunistically sweeps expired rows, so no cron owns
-- this table.
CREATE TABLE plaid_link_session (
    session_id  TEXT PRIMARY KEY,
    link_token  TEXT        NOT NULL,
    return_url  TEXT        NOT NULL,
    rust_state  TEXT        NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at  TIMESTAMPTZ NOT NULL
);

CREATE INDEX plaid_link_session_expires_at_idx ON plaid_link_session (expires_at);
