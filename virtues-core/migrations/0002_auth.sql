-- 0002 — Authentication.
--
-- Pair-only auth model. No passwords, no email, no magic links.
--
-- Canonical entity is `app_device` — every connecting client (browser tab,
-- iOS app, Mac collector, sensor) is a paired device. Both the browser
-- cookie path (`app_auth_session`) and the bearer-token path (`credentials`)
-- carry a `device_id` FK so the unified Devices page can list and revoke
-- across credential types in a single transaction. Revocation cascades:
-- soft-revoke the device → middleware refuses the cookie AND the WG daemon
-- evicts the peer.
--
-- Bootstrap primitive: `app_pair_token`. A 24-byte random token with an
-- RFC 8628-shaped state machine (pending → authorized → consumed). The
-- raw token is never stored; only SHA-256(token). Tokens grant only the
-- right to enroll — devices generate their own long-lived secrets and
-- submit pubkeys, never receive long-lived secrets via the bootstrap.
--
-- Multi-user seam: every device FKs to `app_auth_user`. v1 ships with a
-- single seeded user row; multi-user UI is v1.1+. Don't add WHERE user_id
-- IS NULL anywhere — write WHERE user_id = $X.

-- ---------------------------------------------------------------------------
-- User (single-tenant in v1; table exists to give devices a stable FK)
-- ---------------------------------------------------------------------------
CREATE TABLE app_auth_user (
    id          TEXT PRIMARY KEY,
    label       TEXT,                                              -- free-form display name; never used for auth
    is_owner    BOOLEAN NOT NULL DEFAULT TRUE,                     -- v1 always true; multi-user seam
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON app_auth_user
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

-- Seed the singleton owner row at the same id used by app_user_profile.
INSERT INTO app_auth_user (id) VALUES ('00000000-0000-0000-0000-000000000001')
    ON CONFLICT (id) DO NOTHING;

-- ---------------------------------------------------------------------------
-- Device — the canonical record for everything that talks to the box.
--
-- `kind` discriminates browser cookies from app bearers from sensors. The
-- credential payload itself lives in `app_auth_session` (cookies) or
-- `credentials` (bearers + WG peers). One device may have at most one row in
-- each of those tables; revoke flows the other direction.
-- ---------------------------------------------------------------------------
CREATE TABLE app_device (
    id              TEXT PRIMARY KEY,
    user_id         TEXT NOT NULL REFERENCES app_auth_user(id) ON DELETE CASCADE,
    kind            TEXT NOT NULL
                        CHECK (kind IN ('browser', 'mobile_app', 'desktop_app', 'sensor', 'cli')),
    label           TEXT NOT NULL,                                 -- "MacBook · Chrome", "iPhone 15 Pro", "garage ESP32"
    device_info     JSONB NOT NULL DEFAULT '{}'::jsonb,            -- model, os, app_version, user_agent, etc.
    -- Onboarding "doorplate": set when the owner deliberately renames this
    -- device (vs the auto-generated `label`). NULL = still auto-labeled.
    -- Drives the Tier -1 "named" onboarding step (see api/box_status.rs).
    named_at        TIMESTAMPTZ,
    -- Initial backfill timing for collector/source devices (Tier 0/1). Set on
    -- the first action run for this device's credential, and on its first
    -- success. Drives the "device_collecting" onboarding step.
    init_sync_started_at   TIMESTAMPTZ,
    init_sync_completed_at TIMESTAMPTZ,
    paired_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    paired_from_ip  TEXT,                                          -- audit-only
    last_seen_at    TIMESTAMPTZ,
    revoked_at      TIMESTAMPTZ,                                   -- soft-delete; NULL = active
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_app_device_user_active ON app_device(user_id) WHERE revoked_at IS NULL;
CREATE INDEX idx_app_device_last_seen   ON app_device(last_seen_at DESC) WHERE revoked_at IS NULL;
CREATE TRIGGER set_updated_at BEFORE UPDATE ON app_device
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

-- ---------------------------------------------------------------------------
-- Browser session cookies.
--
-- `last_used_at` is touched on every authenticated request; the middleware
-- enforces an 8h idle ceiling (re-pair required after) in addition to the
-- 30d hard expiry. Revocation is soft via `app_device.revoked_at` — the
-- middleware joins both rows and refuses if either fails.
-- ---------------------------------------------------------------------------
CREATE TABLE app_auth_session (
    id              TEXT PRIMARY KEY,
    session_token   TEXT NOT NULL UNIQUE,                          -- opaque 32-byte base64url
    device_id       TEXT NOT NULL REFERENCES app_device(id) ON DELETE CASCADE,
    expires_at      TIMESTAMPTZ NOT NULL,                          -- hard ceiling (30d from creation)
    last_used_at    TIMESTAMPTZ NOT NULL DEFAULT now(),            -- updated each request; idle-timeout source
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_app_auth_session_device     ON app_auth_session(device_id);
CREATE INDEX idx_app_auth_session_expires_at ON app_auth_session(expires_at);

-- ---------------------------------------------------------------------------
-- Pair token — the bootstrap right-to-enroll.
--
-- 24 random bytes → SHA-256 stored here; raw goes into the QR / `virtues link`
-- URL output and is never persisted. State machine:
--
--   pending    — minted from a paired browser; awaiting minting-device confirm
--   authorized — confirmed (or CLI-minted; physical access implies confirm)
--   consumed   — exchanged for a device; tombstoned 60s for "network died
--                mid-consume, user retries" recovery, then deleted by sweeper
--   expired    — TTL hit before consume (or before confirm in `pending`)
--   denied     — minting device explicitly denied
--
-- TTLs:
--   pending      → expires_at = now() + 10 min  (confirmation window)
--   authorized   → expires_at = now() + 5 min   (consumption window after confirm)
--   cli-minted   → expires_at = now() + 15 min  (longer; you literally typed it
--                                                 from the box, no shoulder-surf risk)
-- ---------------------------------------------------------------------------
CREATE TABLE app_pair_token (
    id                  TEXT PRIMARY KEY,
    token_hash          TEXT NOT NULL UNIQUE,                      -- SHA-256(raw_token); raw never stored
    minted_by_device    TEXT REFERENCES app_device(id) ON DELETE SET NULL,  -- NULL when CLI-minted
    minted_via          TEXT NOT NULL CHECK (minted_via IN ('cli', 'web')),
    intended_kind       TEXT
                            CHECK (intended_kind IN ('browser', 'mobile_app', 'desktop_app', 'sensor', 'cli')),
    status              TEXT NOT NULL DEFAULT 'pending'
                            CHECK (status IN ('pending', 'authorized', 'consumed', 'expired', 'denied')),
    consumed_by_device  TEXT REFERENCES app_device(id) ON DELETE SET NULL,
    authorized_at       TIMESTAMPTZ,
    consumed_at         TIMESTAMPTZ,
    expires_at          TIMESTAMPTZ NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_app_pair_token_status_active ON app_pair_token(status, expires_at)
    WHERE status IN ('pending', 'authorized');
CREATE INDEX idx_app_pair_token_expires_at    ON app_pair_token(expires_at);

-- ---------------------------------------------------------------------------
-- Sudo request — confirmation gate for high-sensitivity actions.
--
-- v1 confirms via `virtues sudo` on the box CLI (proof of physical access).
-- v1.1 replaces the proof channel with push-confirm to a paired phone; the
-- state machine and request shape are unchanged.
--
-- Gated actions (the only ones that mint a sudo request):
--   export_data            — dump all data to a zip
--   change_byo_key         — rotate the AI provider key
--   wipe_box               — factory reset / delete account
--   revoke_last_device     — deleting the only other paired device
-- ---------------------------------------------------------------------------
CREATE TABLE app_sudo_request (
    id              TEXT PRIMARY KEY,
    requested_by    TEXT NOT NULL REFERENCES app_device(id) ON DELETE CASCADE,
    action          TEXT NOT NULL,
    action_payload  JSONB NOT NULL DEFAULT '{}'::jsonb,
    -- State machine:
    --   pending  → awaiting CLI approval
    --   approved → confirmed by `virtues sudo`, awaiting use by the gated handler
    --   consumed → approval was used; the action ran (audit-trail terminal)
    --   denied   → explicitly rejected at the CLI
    --   expired  → TTL elapsed before approve or before consume
    status          TEXT NOT NULL DEFAULT 'pending'
                        CHECK (status IN ('pending', 'approved', 'consumed', 'denied', 'expired')),
    approved_at     TIMESTAMPTZ,
    approved_by     TEXT,                                          -- 'cli' (v1); 'device:<id>' (v1.1 push-confirm)
    consumed_at     TIMESTAMPTZ,                                   -- set when verify_and_consume succeeds
    requested_ip    TEXT,
    expires_at      TIMESTAMPTZ NOT NULL,                          -- 5 min from request
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_app_sudo_request_pending ON app_sudo_request(status, expires_at)
    WHERE status = 'pending';

-- ---------------------------------------------------------------------------
-- Auth event log — the audit trail surfaced at /virtues/activity.
--
-- Append-only. Records pair/revoke/login/sudo events with IP + UA. Needed
-- for incident response ("did I just get paired from an unfamiliar IP?").
-- ---------------------------------------------------------------------------
CREATE TABLE app_auth_event (
    id          BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    user_id     TEXT REFERENCES app_auth_user(id) ON DELETE SET NULL,
    device_id   TEXT REFERENCES app_device(id)    ON DELETE SET NULL,
    event_type  TEXT NOT NULL,                                     -- 'paired', 'revoked', 'session_started', 'session_ended',
                                                                   -- 'sudo_requested', 'sudo_approved', 'sudo_denied',
                                                                   -- 'idle_logout', 'pair_token_minted', 'pair_token_denied'
    detail      JSONB NOT NULL DEFAULT '{}'::jsonb,
    ip          TEXT,
    user_agent  TEXT,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_app_auth_event_user_recent   ON app_auth_event(user_id, occurred_at DESC)
    WHERE user_id IS NOT NULL;
CREATE INDEX idx_app_auth_event_device_recent ON app_auth_event(device_id, occurred_at DESC)
    WHERE device_id IS NOT NULL;
CREATE INDEX idx_app_auth_event_recent        ON app_auth_event(occurred_at DESC);

-- Archive table for `app_auth_event` rows older than 90 days. The
-- maintenance sweeper (`virtues-core/src/maintenance/sweeper.rs`) moves
-- rows here in batches so the live table stays small for incident-response
-- queries while preserving the full history for forensics. Same column
-- shape; no FKs to avoid blocking the archive move when a referenced
-- device or user is later hard-deleted.
CREATE TABLE app_auth_event_archive (
    id          BIGINT PRIMARY KEY,
    user_id     TEXT,
    device_id   TEXT,
    event_type  TEXT NOT NULL,
    detail      JSONB NOT NULL DEFAULT '{}'::jsonb,
    ip          TEXT,
    user_agent  TEXT,
    occurred_at TIMESTAMPTZ NOT NULL,
    archived_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_app_auth_event_archive_occurred ON app_auth_event_archive(occurred_at DESC);
