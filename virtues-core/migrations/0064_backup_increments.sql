-- What has already been shipped to each backup volume.
--
-- The lake is append-only, so a backup only ever needs to send files it has not
-- sent before. Something has to remember which those are, and it cannot be the
-- drive: archives are encrypted to a key the box does not hold, so the box
-- physically cannot read back its own increments to find out what is in them.
-- That is by design (see docs/backup-plan.md), and this table is its cost.
--
-- The drive still holds the authoritative list of which INCREMENTS exist —
-- filenames are plain timestamps and leak nothing. Each run reconciles: any
-- increment this table references that is no longer on the volume has its rows
-- dropped, so the files it held get re-sent rather than silently lost. That
-- makes a wiped or swapped drive self-healing instead of a permanent hole.

CREATE TABLE backup_archived_file (
    volume_id   TEXT NOT NULL REFERENCES storage_volume(id) ON DELETE CASCADE,

    -- Path relative to the lake root, matching the member path inside the
    -- increment. Lake keys are already unique and stable.
    rel_path    TEXT NOT NULL,

    -- Which increment carries these bytes. Each file lives in exactly ONE
    -- increment, which is why increments are never pruned: dropping one would
    -- permanently lose every file it holds, with no other copy anywhere.
    increment   TEXT NOT NULL,

    size_bytes  BIGINT NOT NULL,
    archived_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    PRIMARY KEY (volume_id, rel_path)
);

-- Reconciliation drops whole increments at a time.
CREATE INDEX idx_backup_archived_file_increment
    ON backup_archived_file (volume_id, increment);

COMMENT ON TABLE backup_archived_file IS
    'Which lake files have reached which backup volume. The box cannot read its own encrypted increments, so it tracks them here.';
COMMENT ON COLUMN backup_archived_file.increment IS
    'Increment filename on the volume. Never pruned — each lake file exists in exactly one.';
