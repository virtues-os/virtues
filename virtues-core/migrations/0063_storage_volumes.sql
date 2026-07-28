-- Registry of places the box may write backups.
--
-- The box holds exactly one copy of the owner's archive today. Nothing
-- schedules a backup, and a dead NVMe, a theft, or a fire is total loss. This
-- table is where a second copy gets its address.
--
-- Deliberately NOT a general storage abstraction. `roles` exists so a volume
-- declares what it may hold (Proxmox's `content` field is the same idea), and
-- in this version the only legal value is 'backup'. Live storage stays on
-- STORAGE_PATH, resolved by storage::lake::lake_root, because a lake on a
-- removable volume turns "someone unplugged the drive" into an outage — while
-- an absent BACKUP destination is only ever a skipped run and a warning.
--
-- See docs/backup-plan.md for why tiering/mover/object-storage are excluded.

CREATE TABLE storage_volume (
    id              TEXT PRIMARY KEY,
    -- What the owner calls it. Shown in Settings; never used to find anything.
    name            TEXT NOT NULL,

    kind            TEXT NOT NULL
                    CHECK (kind IN ('internal', 'removable', 'network')),

    -- What this volume is allowed to hold. Enforced in code, not just here,
    -- because the check constraint cannot express "only these values".
    roles           TEXT[] NOT NULL DEFAULT ARRAY['backup']::TEXT[]
                    CHECK (roles <@ ARRAY['backup']::TEXT[] AND array_length(roles, 1) >= 1),

    -- IDENTITY. Filesystem UUID, resolved via /dev/disk/by-uuid at use time.
    -- Mount points move between boots and between drives; a row keyed on
    -- /mnt/backup would silently start writing to whatever got mounted there.
    fs_uuid         TEXT NOT NULL UNIQUE,

    -- Where it was last seen mounted. A cached observation, never the identity,
    -- and meaningless when state <> 'present'.
    mount_path      TEXT,

    -- Subdirectory the box owns inside the volume. Everything outside it
    -- belongs to the owner and is never touched, so one drive can hold a
    -- backup and the owner's own files, or serve two boxes.
    prefix          TEXT NOT NULL,

    state           TEXT NOT NULL DEFAULT 'absent'
                    CHECK (state IN ('present', 'absent', 'degraded')),
    last_seen_at    TIMESTAMPTZ,

    -- From the last probe. Advisory: retention decides against a live statvfs,
    -- not against these.
    capacity_bytes  BIGINT,
    free_bytes      BIGINT,
    probed_at       TIMESTAMPTZ,

    -- Outcome of the last backup targeting this volume. `last_ok_at` is the
    -- number that matters to a human: how old the newest good copy is.
    last_ok_at      TIMESTAMPTZ,
    last_error      TEXT,
    last_error_at   TIMESTAMPTZ,

    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Volume lookup is always "is this drive one of ours", by UUID, at probe time.
CREATE INDEX idx_storage_volume_state ON storage_volume (state);

COMMENT ON TABLE storage_volume IS
    'Backup destinations. Identity is fs_uuid; mount_path is a cached observation.';
COMMENT ON COLUMN storage_volume.roles IS
    'What this volume may hold. Only ''backup'' in this version — live storage stays on STORAGE_PATH.';
COMMENT ON COLUMN storage_volume.prefix IS
    'Box-owned subdirectory. Nothing outside it is ever read, written, or pruned.';
