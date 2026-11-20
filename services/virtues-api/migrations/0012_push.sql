-- Change signals for boxes (routes/push.rs).
--
-- Google Calendar calls one public address when a watched calendar changes.
-- Each box gets a random push id, which it hands Google as the channel
-- token, so the call says which box to tell and nothing else. Google's call
-- carries no event, no address and no account name; this table keeps only
-- the box-chosen channel id until the box collects it.
--
-- `account_id` is atlas's account id, with no foreign key, like
-- `device_keys` since 0011: calendar sync works without a paid wallet, so its
-- change signals must not depend on one.
CREATE TABLE push_ids (
    account_id TEXT PRIMARY KEY,
    push_id    TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE push_pending (
    push_id    TEXT NOT NULL REFERENCES push_ids (push_id) ON DELETE CASCADE,
    channel_id TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (push_id, channel_id)
);
