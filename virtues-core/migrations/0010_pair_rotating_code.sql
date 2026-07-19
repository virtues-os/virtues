-- Universal rotating pairing code.
--
-- Two kinds of pair token now coexist in app_pair_token:
--   • 'oneoff'   — the existing single-use device-add token (web "+ Add Device",
--                  CLI recovery). Consumed atomically on first redeem. Default,
--                  so every existing row keeps today's behavior.
--   • 'standing' — the box's universal rotating code. MULTI-use within its
--                  validity window (it pairs many devices over its life), rotated
--                  on a timer by the pair_rotator task with an overlap window so
--                  a code read mid-rotation never dies under the user. Never
--                  marked 'consumed'; it expires by time.
--
-- The raw code is normally never stored (only SHA-256(token) in token_hash).
-- But a standing code must be DISPLAYED on physical surfaces (the 8" panel,
-- `virtues pair` in the box's terminal), so we keep it — encrypted at rest with
-- the vault key (TokenEncryptor). It is never served over the LAN; only
-- box-local processes (the server rendering /panel, the CLI) decrypt it.
-- Proximity = authority.

ALTER TABLE app_pair_token
    ADD COLUMN kind TEXT NOT NULL DEFAULT 'oneoff'
        CHECK (kind IN ('oneoff', 'standing'));

ALTER TABLE app_pair_token
    ADD COLUMN display_secret TEXT;   -- encrypted raw code; standing codes only

-- Fetch the freshest valid standing code (panel/CLI display) and let consume
-- validate any of the currently-valid standing codes (the overlap window).
CREATE INDEX idx_app_pair_token_standing
    ON app_pair_token(expires_at DESC)
    WHERE kind = 'standing' AND status = 'authorized';
