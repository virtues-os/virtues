-- 036: Add entity references and per-topic/entity novelty scores to wiki_events
--
-- entities: JSON array of wiki entity IDs (from ER pipeline)
-- topic_novelty: JSON object mapping topic string → z-score
-- entity_novelty: JSON object mapping entity ID → z-score

ALTER TABLE wiki_events ADD COLUMN entities TEXT DEFAULT '[]';
ALTER TABLE wiki_events ADD COLUMN topic_novelty TEXT;
ALTER TABLE wiki_events ADD COLUMN entity_novelty TEXT;
