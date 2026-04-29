-- Narrative identity: the user's present-orientation self-portrait.
-- A short, user-authored text (~800 chars) describing who they are now
-- and what direction they're facing. Injected into every system prompt.
-- Singleton row (one per user).

CREATE TABLE IF NOT EXISTS wiki_narrative_identity (
    id TEXT PRIMARY KEY DEFAULT 'nar_identity_001',
    content TEXT NOT NULL DEFAULT '',
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Seed empty singleton row
INSERT OR IGNORE INTO wiki_narrative_identity (id) VALUES ('nar_identity_001');

-- Auto-update timestamp
CREATE TRIGGER IF NOT EXISTS trg_wiki_narrative_identity_updated
AFTER UPDATE ON wiki_narrative_identity
FOR EACH ROW
BEGIN
    UPDATE wiki_narrative_identity SET updated_at = datetime('now') WHERE id = NEW.id;
END;
