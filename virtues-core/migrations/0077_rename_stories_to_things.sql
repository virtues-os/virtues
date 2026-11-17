-- 0077_rename_stories_to_things
--
-- A story becomes a thing, and `wiki_stories` is renamed `wiki_things`.
--
-- A thing is something in the person's life that goes on and cannot be
-- finished, and that is not a person, a place or an organization: the garden,
-- the bike, guitar, sleep. The person names it and it gets an article. A story
-- row already was a subject the person names (a title, their one sentence,
-- optional vague dates), so the table is renamed and keeps its rows. Projects
-- and tasks can sit under a thing (0078).
--
-- Three renames happen together: the table and its constraints, the subject
-- kind in the three tables that name one, and the ids, whose prefix is the
-- kind (`api/subjects.rs` finds a subject from the prefix of its id).
--
-- A release from before this one queries `wiki_stories`. After `virtues
-- rollback` to one of those releases, its Stories list fails until the box
-- upgrades again. Nothing else in such a release reads the table.

ALTER TABLE wiki_stories RENAME TO wiki_things;

-- Every constraint that carried the old table name, the NOT NULL ones
-- included. Postgres 18 keeps a catalog row for a named NOT NULL and older
-- servers do not, so the names are read from the catalog here. Renaming the
-- primary key renames its index.
DO $$
DECLARE
    c record;
BEGIN
    FOR c IN
        SELECT conname FROM pg_constraint
         WHERE conrelid = 'wiki_things'::regclass
           AND conname LIKE 'wiki\_stories\_%'
    LOOP
        EXECUTE format(
            'ALTER TABLE wiki_things RENAME CONSTRAINT %I TO %I',
            c.conname,
            'wiki_things_' || substr(c.conname, length('wiki_stories_') + 1)
        );
    END LOOP;
END $$;

-- Ids. No foreign key points at this table. An article, a note and a rule
-- name their subject by kind and id, and those rows move below.
UPDATE wiki_things SET id = regexp_replace(id, '^story_', 'thing_');

-- The subject kind. Each CHECK is dropped before its rows move and added back
-- with `thing` in place of `story`, so a box that has a story row cannot fail
-- the new CHECK and refuse to boot.
ALTER TABLE wiki_articles DROP CONSTRAINT wiki_articles_subject_type_check;
UPDATE wiki_articles
   SET subject_type = 'thing',
       subject_id = regexp_replace(subject_id, '^story_', 'thing_')
 WHERE subject_type = 'story';
ALTER TABLE wiki_articles ADD CONSTRAINT wiki_articles_subject_type_check
    CHECK (subject_type = ANY (ARRAY[
        'person'::text, 'place'::text, 'organization'::text, 'day'::text,
        'thing'::text, 'narrative_identity'::text, 'chapter'::text, 'year'::text
    ]));

ALTER TABLE wiki_notes DROP CONSTRAINT wiki_notes_subject_type_check;
UPDATE wiki_notes
   SET subject_type = 'thing',
       subject_id = regexp_replace(subject_id, '^story_', 'thing_')
 WHERE subject_type = 'story';
ALTER TABLE wiki_notes ADD CONSTRAINT wiki_notes_subject_type_check
    CHECK (subject_type = ANY (ARRAY[
        'event'::text, 'day'::text, 'thing'::text, 'person'::text,
        'place'::text, 'organization'::text, 'chat'::text, 'page'::text,
        'narrative_identity'::text, 'chapter'::text, 'year'::text
    ]));

ALTER TABLE wiki_rules DROP CONSTRAINT wiki_rules_subject_type_check;
UPDATE wiki_rules
   SET subject_type = 'thing',
       subject_id = regexp_replace(subject_id, '^story_', 'thing_')
 WHERE subject_type = 'story';
ALTER TABLE wiki_rules ADD CONSTRAINT wiki_rules_subject_type_check
    CHECK (subject_type IS NULL OR subject_type = ANY (ARRAY[
        'person'::text, 'place'::text, 'organization'::text, 'day'::text,
        'thing'::text, 'narrative_identity'::text, 'chapter'::text, 'year'::text
    ]));

-- A pin to the Stories room opens the Things room. An app screen's pin shows
-- the label it was pinned with, so that moves too.
UPDATE app_pins
   SET url = '/wiki/things',
       label = CASE WHEN label = 'Stories' THEN 'Things' ELSE label END
 WHERE url = '/wiki/stories';
