-- 020: Per-event W6H activation
--
-- Stores the 7-dimension W6H activation vector on each timeline event.
-- Computed after LLM event identification by checking which ontologies
-- have data in the event's [start_time, end_time] range.
-- Stored as JSON array: "[0.8, 0.0, 0.3, 0.0, 0.9, 0.0, 0.4]"

ALTER TABLE wiki_events ADD COLUMN w6h_activation TEXT;
