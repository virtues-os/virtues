-- The two absences the ontology review named (2026-09-01): bonds have no
-- carrier, and nobody in the record can die.
--
-- BOND — the authored line about what a person MEANS, distinct from every
-- observed statistic about them. The doctrine (agents/build/
-- narrative-identity.md) is that recency says who is AROUND; only the person
-- can say who MATTERS — "my brother", "my oldest friend", "we don't speak
-- anymore". One sentence, theirs verbatim, never inferred from message
-- volume, and rendered above the observed stats wherever the person's page
-- shows them. NULL is the normal state: an unwritten bond is an absence, not
-- a zero.
ALTER TABLE wiki_people ADD COLUMN bond text;

-- DEATH — the record will outlive people in it, and a schema that can only
-- record a birthday treats that as unsayable. Same precision idiom as
-- wiki_chapters: "sometime in 2019" is a real answer and forcing a full date
-- would record a lie.
ALTER TABLE wiki_people ADD COLUMN died_on date;
ALTER TABLE wiki_people ADD COLUMN died_precision text
    CONSTRAINT wiki_people_died_precision_check
    CHECK (died_precision IS NULL OR died_precision IN ('year', 'month', 'day'));

-- A death without a date is still a death the person may want recorded; a
-- precision without a date is meaningless. Only the second is refused.
ALTER TABLE wiki_people ADD CONSTRAINT wiki_people_death_shape_check
    CHECK (died_precision IS NULL OR died_on IS NOT NULL);
