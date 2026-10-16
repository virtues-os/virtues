-- Nightly: return a demo box to the demo life. Demo boxes only.
--
-- A demo box is shared, one visitor after another, so whatever anyone writes
-- on it is otherwise visible to the next. Everything the life brought is
-- listed in demo.rows; this deletes the product state that is NOT in that list
-- (a visitor's chats, pages, projects, pins, the people their chat taught the
-- wiki) and leaves everything that is. Auth, paired devices, settings and the
-- review pair code are never named, so a reviewer's pairing survives the night.
-- Run it in one transaction (`psql -1`): the unit and install.sh both do.

CREATE TEMP TABLE _keep_chats ON COMMIT DROP AS
  SELECT id FROM demo.rows WHERE tbl = 'app_chats'
  UNION SELECT unnest(ARRAY['chat_getting_started', 'chat_narrative_interview']);

DELETE FROM app_chat_messages WHERE chat_id NOT IN (SELECT id FROM _keep_chats);
DELETE FROM app_chat_usage    WHERE chat_id NOT IN (SELECT id FROM _keep_chats);
DELETE FROM app_chats         WHERE id      NOT IN (SELECT id FROM _keep_chats);

DELETE FROM app_page_versions
 WHERE page_id NOT IN (SELECT id FROM demo.rows WHERE tbl = 'app_pages');
DELETE FROM app_pages WHERE id NOT IN (SELECT id FROM demo.rows WHERE tbl = 'app_pages');

DELETE FROM app_project_items
 WHERE project_id NOT IN (SELECT id FROM demo.rows WHERE tbl = 'app_projects')
    OR id::text NOT IN (SELECT id FROM demo.rows WHERE tbl = 'app_project_items');
DELETE FROM app_projects WHERE id NOT IN (SELECT id FROM demo.rows WHERE tbl = 'app_projects');

DELETE FROM app_assistant_memories
 WHERE id::text NOT IN (SELECT id FROM demo.rows WHERE tbl = 'app_assistant_memories');

-- People, places and organisations a visitor's chat added to the wiki.
DELETE FROM wiki_people WHERE id NOT IN (SELECT id FROM demo.rows WHERE tbl = 'wiki_people');
DELETE FROM wiki_places WHERE id NOT IN (SELECT id FROM demo.rows WHERE tbl = 'wiki_places');
DELETE FROM wiki_orgs   WHERE id NOT IN (SELECT id FROM demo.rows WHERE tbl = 'wiki_orgs');

-- Nothing in the life lives in these, so anything here arrived from a visitor.
DELETE FROM app_marginalia;
DELETE FROM app_pins;
DELETE FROM app_drive_files;

-- The owner is the persona. A visitor who walks through setup renames the owner
-- and moves the home timezone, which moves every day boundary in the record.
INSERT INTO app_user_profile (full_name, preferred_name, occupation, home_place_id, home_timezone,
                              home_city, self_person_id, onboarding_status)
VALUES ('Jules Arden', 'Jules', 'Product designer', 'p3y_pl_home', 'America/Chicago', 'Austin', 'wk_self',
        'active')
ON CONFLICT (id) DO UPDATE
   SET full_name = EXCLUDED.full_name, preferred_name = EXCLUDED.preferred_name,
       occupation = EXCLUDED.occupation, employer = NULL, home_place_id = EXCLUDED.home_place_id,
       home_timezone = EXCLUDED.home_timezone, home_city = EXCLUDED.home_city,
       self_person_id = EXCLUDED.self_person_id, onboarding_status = 'active';
