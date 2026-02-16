-- Per-event entropy: embedding novelty + Shannon W6H
-- Adds embedding storage and computed entropy scores to wiki_events

ALTER TABLE wiki_events ADD COLUMN embedding BLOB;
ALTER TABLE wiki_events ADD COLUMN entropy REAL;
ALTER TABLE wiki_events ADD COLUMN w6h_entropy REAL;
