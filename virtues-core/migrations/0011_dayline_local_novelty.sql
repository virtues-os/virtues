-- Local (within-kind) novelty for dayline events.
--
-- `novelty_z` (0006) is GLOBAL novelty — distance from a kernel-weighted
-- centroid of all recent events ("rare in your life at all"). It cannot tell
-- "off-pattern for its kind" from "rare type" because it normalizes against
-- one global reference.
--
-- These columns add the LOCAL channel: a density-relative (LOF) score that
-- answers "unusual compared to events LIKE it" — e.g. first cardio when you
-- always lift. The two are orthogonal and intentionally NOT blended; any
-- single salience number is derived on read (magnitude/max), never stored.
--
--   lof_raw          — raw Local Outlier Factor (ratio, ~1.0 = typical,
--                      >1.5 ≈ outlier). Kept underneath so the interpretable
--                      absolute threshold + cold-start-safe binary survive.
--   local_novelty_z  — clamp(±3, robust_standardize(ln(lof_raw))): LOF mapped
--                      onto the same σ axis as novelty_z for the Dayline.

ALTER TABLE wiki_events ADD COLUMN IF NOT EXISTS lof_raw          DOUBLE PRECISION;
ALTER TABLE wiki_events ADD COLUMN IF NOT EXISTS local_novelty_z  DOUBLE PRECISION;
