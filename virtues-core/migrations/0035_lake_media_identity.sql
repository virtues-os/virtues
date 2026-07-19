-- ---------------------------------------------------------------------------
-- Lake: media is identified by its key, not by its bytes (docs/lake-plan.md)
--
-- 0034 made sha256 globally UNIQUE, which is right for raw_stream objects —
-- content-dedupe is what stops a failing batch, retried every 5 minutes for a
-- week, from archiving thousands of identical copies of itself.
--
-- It is WRONG for media. A blob's identity is the recording it belongs to, not
-- its bytes. Two genuinely different recordings that happen to encode
-- identically — a pair of silent chunks, which is a real case here: the
-- transcription drainer exists partly to handle ~28-byte empty AAC containers —
-- would collide, and one recording would silently inherit the other's file.
-- Nothing would surface it; the audio would simply be wrong.
--
-- So: content-dedupe raw_stream, key-dedupe media. `storage_key` is already
-- UNIQUE and is derived from the recording's stream id, so media identity is
-- already well-defined without the digest. sha256 stays on media rows as an
-- integrity/accounting field, just not as a constraint.
--
-- (Checked before relying on it: the 885 legacy recordings on the box hash to
-- 885 distinct digests — no collisions today. This closes the door before one
-- happens, not after.)
-- ---------------------------------------------------------------------------

ALTER TABLE lake_objects DROP CONSTRAINT IF EXISTS lake_objects_sha256_key;

CREATE UNIQUE INDEX idx_lake_objects_raw_sha
    ON lake_objects (sha256)
    WHERE kind = 'raw_stream';
