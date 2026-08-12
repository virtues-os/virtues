-- The setup phrase: the box's one secret that proves ownership.
--
-- Four words. It is the Bluetooth setup key AND the recovery key, because those
-- were never two things. See docs/onboarding-paradigm.md §1.
--
-- The whole security argument is *where it is readable and when*:
--
--   * While the box is UNCLAIMED the phrase is on its panel, and it ROTATES
--     there (15 min + 5 min grace, mirroring the standing pair code). Showing it
--     costs nothing when the box is empty, and reading it requires seeing the
--     box — radio range passes through walls, line of sight does not. Rotation
--     is what stops a box left unclaimed for a week from being a permanent key
--     on display for every houseguest with a camera.
--
--   * On first claim the phrase FREEZES and leaves the screen forever. The box
--     now holds a life, so the phrase exists only where the owner saved it.
--     Because it freezes rather than being replaced, what they saved is exactly
--     what they typed — there is no second secret to explain.
--
-- That asymmetry is what makes the reset button safe: anyone who opens the case
-- can reset a box (a nuisance — the data survives), but only someone with the
-- phrase can CLAIM it, and a screwdriver does not provide one.
--
-- `phrase_hash` is what verification uses; the plaintext is kept encrypted in
-- `display_secret` for the panel alone, exactly as the standing pair code does,
-- and is never served over the LAN. Once frozen, `display_secret` is CLEARED —
-- a frozen phrase must not be recoverable from the box, only verifiable.
CREATE TABLE app_setup_phrase (
    id              text PRIMARY KEY,
    -- SHA-256 of the normalized phrase (lowercase, single hyphens).
    phrase_hash     text        NOT NULL,
    -- Encrypted plaintext, for the panel while unclaimed. NULL once frozen.
    display_secret  text,
    -- NULL while rotating; set when the box is first claimed. A frozen row is
    -- the box's permanent credential and is never superseded.
    frozen_at       timestamptz,
    -- Rotation horizon. Ignored once frozen — a frozen phrase never expires.
    expires_at      timestamptz NOT NULL,
    created_at      timestamptz NOT NULL DEFAULT now()
);

-- At most one frozen phrase, ever. The partial index makes that an invariant
-- rather than a convention: a second freeze is a bug that should fail loudly,
-- not a quiet second credential.
CREATE UNIQUE INDEX app_setup_phrase_one_frozen
    ON app_setup_phrase ((frozen_at IS NOT NULL))
    WHERE frozen_at IS NOT NULL;

CREATE INDEX app_setup_phrase_expires_idx ON app_setup_phrase (expires_at);
