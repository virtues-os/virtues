-- ---------------------------------------------------------------------------
-- Entity aliases (V2 W2a — docs/stories-plan.md §8)
--
-- The only thing standing between a floating mention and a linked entity.
--
-- Nothing in the ER pipeline links a prose mention to a person by guessing.
-- A mention resolves iff its normalized surface matches EXACTLY ONE entity, by
-- canonical name, nickname, or an alias a human put here. Anything else stays
-- floating — dust: searchable, never linked, never lying.
--
-- So an alias is not a hint. It is the *record of a human decision*, and it is
-- what makes the review queue converge: linking "Sarah" once writes the alias,
-- backfills every past mention of that surface, and resolves every future one
-- without ever asking again. One decision per name, once — not per mention.
--
-- A JSONB array on the entity, not a table: `wiki_people` already carries
-- `emails` and `phones` this way, and the uniqueness a table would have bought
-- us is already an emergent property of the resolver's rule. If two people
-- both answer to "Sarah", the lookup returns two candidates and the resolver
-- declines to link — which is precisely the behavior a UNIQUE constraint would
-- have forced. The constraint was ceremony.
--
-- Aliases are stored lowercased; the resolver lowercases the surface before
-- the containment check (`aliases ? 'sarah'`), so matching is case-insensitive
-- without a functional index.
-- ---------------------------------------------------------------------------

ALTER TABLE wiki_people ADD COLUMN aliases JSONB NOT NULL DEFAULT '[]'::jsonb;
ALTER TABLE wiki_places ADD COLUMN aliases JSONB NOT NULL DEFAULT '[]'::jsonb;
ALTER TABLE wiki_orgs   ADD COLUMN aliases JSONB NOT NULL DEFAULT '[]'::jsonb;

-- `?` (key/element exists) is the resolver's hot path, once per distinct
-- surface per hour. GIN with jsonb_ops supports it directly.
CREATE INDEX idx_wiki_people_aliases ON wiki_people USING GIN (aliases);
CREATE INDEX idx_wiki_places_aliases ON wiki_places USING GIN (aliases);
CREATE INDEX idx_wiki_orgs_aliases   ON wiki_orgs   USING GIN (aliases);

COMMENT ON COLUMN wiki_people.aliases IS
    'Lowercased surfaces that resolve to this person ("mom", "sarah smith"). Written by a human via the mention review queue — never inferred. A surface shared by two entities simply never resolves (stays floating as dust); that is the precision-over-recall contract, not a gap.';
COMMENT ON COLUMN wiki_places.aliases IS
    'Lowercased surfaces that resolve to this place. See wiki_people.aliases.';
COMMENT ON COLUMN wiki_orgs.aliases IS
    'Lowercased surfaces that resolve to this org. See wiki_people.aliases.';

-- ---------------------------------------------------------------------------
-- er_mentions.snippet — the quotation that makes a mention decidable
--
-- A bare surface is not reviewable. "Sarah" tells a human nothing; they cannot
-- link what they cannot recognize. "had a great time with Sarah Smith last
-- night" they can answer in a second.
--
-- This is the difference between NER and evidence, and it costs nothing: the
-- extractor is already reading the sentence when it finds the name.
-- ---------------------------------------------------------------------------

ALTER TABLE er_mentions ADD COLUMN snippet TEXT;

COMMENT ON COLUMN er_mentions.snippet IS
    'The sentence the surface was found in. What the review queue shows a human so the decision is answerable at a glance; also what becomes a wiki_marginalia note on the entity once linked.';
