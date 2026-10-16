-- Empty a demo box of its life before (re)installing one. Demo boxes only:
-- install.sh refuses to run this unless the box is marked as a demo box.
--
-- Deletes every ingested and derived row (data_*, wiki_*), the search index
-- built on them, and the product content a life brings (chats, pages,
-- projects, memories, applets it seeded). Keeps what belongs to the box rather
-- than the life: auth, paired devices, settings, the system applets, the
-- getting-started chat, and the real weather the box fetches for itself.

DO $$
DECLARE t text;
BEGIN
  FOR t IN SELECT table_name FROM information_schema.tables
            WHERE table_schema = 'public' AND table_type = 'BASE TABLE'
              AND (table_name LIKE 'data\_%' OR table_name LIKE 'wiki\_%' OR table_name LIKE 'search\_%')
  LOOP
    IF t = 'data_environment_weather' THEN
      DELETE FROM data_environment_weather WHERE source_provider IS DISTINCT FROM 'open-meteo';
    ELSE
      EXECUTE format('DELETE FROM %I', t);
    END IF;
  END LOOP;
END $$;

DELETE FROM app_chat_messages WHERE chat_id NOT IN ('chat_getting_started', 'chat_narrative_interview');
DELETE FROM app_chat_usage    WHERE chat_id NOT IN ('chat_getting_started', 'chat_narrative_interview');
DELETE FROM app_chats         WHERE id      NOT IN ('chat_getting_started', 'chat_narrative_interview');
DELETE FROM app_page_versions;
DELETE FROM app_pages;
DELETE FROM app_project_items;
DELETE FROM app_projects;
DELETE FROM app_assistant_memories;
DELETE FROM app_marginalia;
DELETE FROM app_pins;
DELETE FROM app_drive_files;
-- The life's own applets (a history of runs for the Applets room). The box's
-- shipped applets are owner 'user' as well, so match the life's ids, not owner.
DELETE FROM app_applet_runs WHERE applet_id LIKE 'p3y\_%';
DELETE FROM app_applets     WHERE id LIKE 'p3y\_%';

DROP SCHEMA IF EXISTS demo CASCADE;
DROP SCHEMA IF EXISTS demo_week CASCADE;
