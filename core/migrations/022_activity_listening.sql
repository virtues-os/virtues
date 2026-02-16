-- Spotify listening history ontology table
-- Stores individual track plays from Spotify's Recently Played API

CREATE TABLE IF NOT EXISTS data_activity_listening (
    id TEXT PRIMARY KEY,
    source_connection_id TEXT REFERENCES elt_source_connections(id),

    track_name TEXT NOT NULL,
    artist_name TEXT,
    album_name TEXT,
    duration_ms INTEGER,
    played_at TEXT NOT NULL,
    spotify_track_id TEXT,
    spotify_uri TEXT,
    context_type TEXT,          -- playlist, album, artist, collection
    context_name TEXT,
    context_uri TEXT,

    source_stream_id TEXT NOT NULL UNIQUE,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    deleted_at_source TEXT,
    is_archived INTEGER DEFAULT 0,
    metadata TEXT DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_activity_listening_played_at ON data_activity_listening(played_at);
CREATE INDEX idx_activity_listening_source ON data_activity_listening(source_connection_id);
