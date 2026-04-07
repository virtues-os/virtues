-- Add versioning to narrative identity.
-- active=1 is the live version (editable). active=0 rows are daily snapshots.
ALTER TABLE wiki_narrative_identity ADD COLUMN active INTEGER NOT NULL DEFAULT 0;

-- Mark the existing singleton row as active
UPDATE wiki_narrative_identity SET active = 1 WHERE id = 'nar_identity_001';
