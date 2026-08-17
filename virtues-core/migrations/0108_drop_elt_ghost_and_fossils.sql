-- Drop the ELT ghost, and rename the constraints eight table renames left behind.
--
-- Written immediately before the migration squash, which is the only reason it
-- is affordable. Everything here was known and deliberately deferred — see
-- 0104's header, which did this exact analysis and stopped: "the cleanup is
-- real -- 21 dead columns and 21 dead constraints -- but it is a schema change
-- across a third of the data model." With zero users and zero rows to migrate,
-- that reservation dissolves.
--
-- ## 1. The ELT ghost
--
-- `elt_source_connections` is a table with ONE column and no rows. Nothing has
-- ever written to it. Yet TWENTY-ONE of the schema's fifty-two foreign keys
-- point at it -- forty percent of the database's referential integrity spent
-- enforcing a relationship to an empty stub -- because every `data_*` table
-- carries a `source_connection_id` that no code has ever read or written.
--
-- Provenance is not lost with it. It was modeled twice and the wrong copy got
-- the foreign keys: the live one is the `(source_stream_id UNIQUE, source_table,
-- source_provider)` triple on every ingested row, and `source_stream_id` is the
-- idempotency key the whole ingest layer is built on.
--
-- ## 2. The `app_annotations` compatibility view
--
-- Its own comment says "Drop one release after 0082 ships." 0082 shipped
-- twenty-six migrations ago, and pre-launch there are no older binaries to be
-- compatible with.
--
-- ## 3. Indexes that duplicate a constraint
--
-- Only this class -- an index that is an exact copy of, or a strict column
-- prefix of, a UNIQUE or PRIMARY KEY index. Postgres will use the constraint's
-- index for anything these could serve, so no query plan changes. Indexes that
-- merely appear to have no caller are NOT dropped here: the AI SQL agent issues
-- ad-hoc read-only queries, so "no Rust reader" does not mean "unreachable".
--
-- Two are worth naming. `search_embeddings` carried SEVEN indexes, two of them
-- prefixes of its own unique constraint, on the table the indexer rewrites by
-- delete-and-reinsert on every re-embed -- so each was a wasted write and WAL
-- record per row touched, on an SBC with one NVMe. And two are partial indexes
-- on their own table's PRIMARY KEY column, which can never be the best plan.
--
-- ## 4. 81 constraints named after tables that no longer exist
--
-- `ALTER TABLE ... RENAME` does not rename constraints, so eight renames left
-- their names behind: app_actions -> app_applets, app_action_runs ->
-- app_applet_runs, app_annotations -> app_marginalia, app_spaces/wiki_stories
-- -> app_notebooks, app_space_items -> app_notebook_items, wiki_marginalia ->
-- wiki_notes, wiki_entity_refs -> wiki_refs, wiki_standing_order -> wiki_rules,
-- data_activity_app_usage -> data_activity_app_session. The best specimen is
-- `app_notebooks.auto_add_materials` carrying a NOT NULL constraint named
-- `wiki_stories_auto_add_materials_not_null` -- a column on notebooks named
-- after the stories table, which was itself dropped.
--
-- Renamed here rather than left for the squash to fix, because the squash is
-- derived from `pg_dump`, which preserves every fossil name faithfully. Left
-- alone, they would be baked into 0001_initial.sql permanently, and the next
-- person to read this schema would have to learn a decade of table renames to
-- understand a constraint name.
--
-- The three `*_singleton` CHECK constraints are NOT renamed: those are
-- deliberate descriptive names, not fossils.

-- ── 1. The ELT ghost ────────────────────────────────────────────────────────
-- The 21 FK constraints and `idx_activity_listening_source` go with the columns.
ALTER TABLE data_activity_app_session DROP COLUMN source_connection_id;
ALTER TABLE data_activity_listening DROP COLUMN source_connection_id;
ALTER TABLE data_activity_web_browsing DROP COLUMN source_connection_id;
ALTER TABLE data_calendar_event DROP COLUMN source_connection_id;
ALTER TABLE data_communication_email DROP COLUMN source_connection_id;
ALTER TABLE data_communication_message DROP COLUMN source_connection_id;
ALTER TABLE data_communication_transcription DROP COLUMN source_connection_id;
ALTER TABLE data_content_bookmark DROP COLUMN source_connection_id;
ALTER TABLE data_content_conversation DROP COLUMN source_connection_id;
ALTER TABLE data_content_document DROP COLUMN source_connection_id;
ALTER TABLE data_financial_account DROP COLUMN source_connection_id;
ALTER TABLE data_financial_asset DROP COLUMN source_connection_id;
ALTER TABLE data_financial_liability DROP COLUMN source_connection_id;
ALTER TABLE data_financial_transaction DROP COLUMN source_connection_id;
ALTER TABLE data_health_heart_rate DROP COLUMN source_connection_id;
ALTER TABLE data_health_hrv DROP COLUMN source_connection_id;
ALTER TABLE data_health_sleep DROP COLUMN source_connection_id;
ALTER TABLE data_health_steps DROP COLUMN source_connection_id;
ALTER TABLE data_health_workout DROP COLUMN source_connection_id;
ALTER TABLE data_location_point DROP COLUMN source_connection_id;
ALTER TABLE data_location_visit DROP COLUMN source_connection_id;
DROP TABLE elt_source_connections;

-- ── 2. The expired compatibility view ───────────────────────────────────────
DROP VIEW IF EXISTS app_annotations;

-- ── 3. Indexes that duplicate a constraint ──────────────────────────────────
DROP INDEX IF EXISTS idx_api_usage_endpoint_day;          -- = app_api_usage_endpoint_day_bucket_key
DROP INDEX IF EXISTS idx_chat_messages_chat;              -- = app_chat_messages_chat_id_sequence_num_key
DROP INDEX IF EXISTS idx_app_drive_files_path;            -- = app_drive_files_path_key
DROP INDEX IF EXISTS idx_page_shares_token;               -- = app_page_shares_token_key
DROP INDEX IF EXISTS idx_page_versions_page;              -- = app_page_versions_..._key (btree scans backwards)
DROP INDEX IF EXISTS idx_wiki_days_date;                  -- = wiki_days_date_key
DROP INDEX IF EXISTS idx_wiki_years_year;                 -- = wiki_years_year_key
DROP INDEX IF EXISTS idx_chat_edit_permissions_chat;      -- prefix of app_chat_edit_permissions_..._key
DROP INDEX IF EXISTS idx_chat_usage_chat;                 -- prefix of app_chat_usage_chat_id_model_key
DROP INDEX IF EXISTS idx_extracted_document_chunks_file;  -- prefix of extracted_document_chunks_..._key
DROP INDEX IF EXISTS idx_search_embeddings_ontology;      -- prefix of search_embeddings_..._key
DROP INDEX IF EXISTS idx_search_embeddings_record;        -- prefix of search_embeddings_..._key
DROP INDEX IF EXISTS idx_wiki_refs_source;                -- prefix of idx_wiki_refs_source_subject_type
DROP INDEX IF EXISTS idx_usage_limits_enabled;            -- partial on app_usage_limits PRIMARY KEY (service)
DROP INDEX IF EXISTS idx_financial_account_active;        -- partial on data_financial_account PRIMARY KEY (id)

-- ── 4. 81 fossil constraint names ──────────────────────────────────────────
ALTER TABLE app_applet_runs RENAME CONSTRAINT "app_action_runs_created_at_not_null" TO "app_applet_runs_created_at_not_null";
ALTER TABLE app_applet_runs RENAME CONSTRAINT "app_action_runs_id_not_null" TO "app_applet_runs_id_not_null";
ALTER TABLE app_applet_runs RENAME CONSTRAINT "app_action_runs_parent_run_id_fkey" TO "app_applet_runs_parent_run_id_fkey";
ALTER TABLE app_applet_runs RENAME CONSTRAINT "app_action_runs_records_processed_not_null" TO "app_applet_runs_records_processed_not_null";
ALTER TABLE app_applet_runs RENAME CONSTRAINT "app_action_runs_started_at_not_null" TO "app_applet_runs_started_at_not_null";
ALTER TABLE app_applet_runs RENAME CONSTRAINT "app_action_runs_status_not_null" TO "app_applet_runs_status_not_null";
ALTER TABLE app_applet_runs RENAME CONSTRAINT "app_action_runs_trigger_not_null" TO "app_applet_runs_trigger_not_null";
ALTER TABLE app_applets RENAME CONSTRAINT "app_actions_config_not_null" TO "app_applets_config_not_null";
ALTER TABLE app_applets RENAME CONSTRAINT "app_actions_created_at_not_null" TO "app_applets_created_at_not_null";
ALTER TABLE app_applets RENAME CONSTRAINT "app_actions_credential_id_fkey" TO "app_applets_credential_id_fkey";
ALTER TABLE app_applets RENAME CONSTRAINT "app_actions_device_id_fkey" TO "app_applets_device_id_fkey";
ALTER TABLE app_applets RENAME CONSTRAINT "app_actions_enabled_not_null" TO "app_applets_enabled_not_null";
ALTER TABLE app_applets RENAME CONSTRAINT "app_actions_id_not_null" TO "app_applets_id_not_null";
ALTER TABLE app_applets RENAME CONSTRAINT "app_actions_name_not_null" TO "app_applets_name_not_null";
ALTER TABLE app_applets RENAME CONSTRAINT "app_actions_owner_not_null" TO "app_applets_owner_not_null";
ALTER TABLE app_applets RENAME CONSTRAINT "app_actions_triggers_not_null" TO "app_applets_triggers_not_null";
ALTER TABLE app_applets RENAME CONSTRAINT "app_actions_updated_at_not_null" TO "app_applets_updated_at_not_null";
ALTER TABLE app_marginalia RENAME CONSTRAINT "app_annotations_color_not_null" TO "app_marginalia_color_not_null";
ALTER TABLE app_marginalia RENAME CONSTRAINT "app_annotations_created_at_not_null" TO "app_marginalia_created_at_not_null";
ALTER TABLE app_marginalia RENAME CONSTRAINT "app_annotations_file_id_fkey" TO "app_marginalia_file_id_fkey";
ALTER TABLE app_marginalia RENAME CONSTRAINT "app_annotations_file_id_not_null" TO "app_marginalia_file_id_not_null";
ALTER TABLE app_marginalia RENAME CONSTRAINT "app_annotations_id_not_null" TO "app_marginalia_id_not_null";
ALTER TABLE app_marginalia RENAME CONSTRAINT "app_annotations_note_md_not_null" TO "app_marginalia_note_md_not_null";
ALTER TABLE app_marginalia RENAME CONSTRAINT "app_annotations_quote_prefix_not_null" TO "app_marginalia_quote_prefix_not_null";
ALTER TABLE app_marginalia RENAME CONSTRAINT "app_annotations_quote_suffix_not_null" TO "app_marginalia_quote_suffix_not_null";
ALTER TABLE app_marginalia RENAME CONSTRAINT "app_annotations_quote_text_not_null" TO "app_marginalia_quote_text_not_null";
ALTER TABLE app_marginalia RENAME CONSTRAINT "app_annotations_rects_not_null" TO "app_marginalia_rects_not_null";
ALTER TABLE app_marginalia RENAME CONSTRAINT "app_annotations_updated_at_not_null" TO "app_marginalia_updated_at_not_null";
ALTER TABLE app_notebook_items RENAME CONSTRAINT "app_space_items_added_at_not_null" TO "app_notebook_items_added_at_not_null";
ALTER TABLE app_notebook_items RENAME CONSTRAINT "app_space_items_id_not_null" TO "app_notebook_items_id_not_null";
ALTER TABLE app_notebook_items RENAME CONSTRAINT "app_space_items_pkey" TO "app_notebook_items_pkey";
ALTER TABLE app_notebook_items RENAME CONSTRAINT "app_space_items_sort_order_not_null" TO "app_notebook_items_sort_order_not_null";
ALTER TABLE app_notebook_items RENAME CONSTRAINT "app_space_items_space_id_fkey" TO "app_notebook_items_space_id_fkey";
ALTER TABLE app_notebook_items RENAME CONSTRAINT "app_space_items_space_id_not_null" TO "app_notebook_items_space_id_not_null";
ALTER TABLE app_notebook_items RENAME CONSTRAINT "app_space_items_space_id_url_key" TO "app_notebook_items_space_id_url_key";
ALTER TABLE app_notebook_items RENAME CONSTRAINT "app_space_items_url_not_null" TO "app_notebook_items_url_not_null";
ALTER TABLE app_notebooks RENAME CONSTRAINT "app_spaces_created_at_not_null" TO "app_notebooks_created_at_not_null";
ALTER TABLE app_notebooks RENAME CONSTRAINT "app_spaces_id_not_null" TO "app_notebooks_id_not_null";
ALTER TABLE app_notebooks RENAME CONSTRAINT "app_spaces_name_not_null" TO "app_notebooks_name_not_null";
ALTER TABLE app_notebooks RENAME CONSTRAINT "app_spaces_pkey" TO "app_notebooks_pkey";
ALTER TABLE app_notebooks RENAME CONSTRAINT "app_spaces_sort_order_not_null" TO "app_notebooks_sort_order_not_null";
ALTER TABLE app_notebooks RENAME CONSTRAINT "app_spaces_updated_at_not_null" TO "app_notebooks_updated_at_not_null";
ALTER TABLE app_notebooks RENAME CONSTRAINT "wiki_stories_auto_add_materials_not_null" TO "app_notebooks_auto_add_materials_not_null";
ALTER TABLE data_activity_app_session RENAME CONSTRAINT "data_activity_app_usage_app_name_not_null" TO "data_activity_app_session_app_name_not_null";
ALTER TABLE data_activity_app_session RENAME CONSTRAINT "data_activity_app_usage_created_at_not_null" TO "data_activity_app_session_created_at_not_null";
ALTER TABLE data_activity_app_session RENAME CONSTRAINT "data_activity_app_usage_end_time_not_null" TO "data_activity_app_session_end_time_not_null";
ALTER TABLE data_activity_app_session RENAME CONSTRAINT "data_activity_app_usage_id_not_null" TO "data_activity_app_session_id_not_null";
ALTER TABLE data_activity_app_session RENAME CONSTRAINT "data_activity_app_usage_is_archived_not_null" TO "data_activity_app_session_is_archived_not_null";
ALTER TABLE data_activity_app_session RENAME CONSTRAINT "data_activity_app_usage_metadata_not_null" TO "data_activity_app_session_metadata_not_null";
ALTER TABLE data_activity_app_session RENAME CONSTRAINT "data_activity_app_usage_pkey" TO "data_activity_app_session_pkey";
ALTER TABLE data_activity_app_session RENAME CONSTRAINT "data_activity_app_usage_source_provider_not_null" TO "data_activity_app_session_source_provider_not_null";
ALTER TABLE data_activity_app_session RENAME CONSTRAINT "data_activity_app_usage_source_stream_id_key" TO "data_activity_app_session_source_stream_id_key";
ALTER TABLE data_activity_app_session RENAME CONSTRAINT "data_activity_app_usage_source_stream_id_not_null" TO "data_activity_app_session_source_stream_id_not_null";
ALTER TABLE data_activity_app_session RENAME CONSTRAINT "data_activity_app_usage_source_table_not_null" TO "data_activity_app_session_source_table_not_null";
ALTER TABLE data_activity_app_session RENAME CONSTRAINT "data_activity_app_usage_start_time_not_null" TO "data_activity_app_session_start_time_not_null";
ALTER TABLE data_activity_app_session RENAME CONSTRAINT "data_activity_app_usage_updated_at_not_null" TO "data_activity_app_session_updated_at_not_null";
ALTER TABLE wiki_notes RENAME CONSTRAINT "wiki_marginalia_author_not_null" TO "wiki_notes_author_not_null";
ALTER TABLE wiki_notes RENAME CONSTRAINT "wiki_marginalia_body_not_null" TO "wiki_notes_body_not_null";
ALTER TABLE wiki_notes RENAME CONSTRAINT "wiki_marginalia_created_at_not_null" TO "wiki_notes_created_at_not_null";
ALTER TABLE wiki_notes RENAME CONSTRAINT "wiki_marginalia_id_not_null" TO "wiki_notes_id_not_null";
ALTER TABLE wiki_notes RENAME CONSTRAINT "wiki_marginalia_kind_not_null" TO "wiki_notes_kind_not_null";
ALTER TABLE wiki_notes RENAME CONSTRAINT "wiki_marginalia_subject_id_not_null" TO "wiki_notes_subject_id_not_null";
ALTER TABLE wiki_notes RENAME CONSTRAINT "wiki_marginalia_subject_type_not_null" TO "wiki_notes_subject_type_not_null";
ALTER TABLE wiki_refs RENAME CONSTRAINT "wiki_entity_refs_confidence_not_null" TO "wiki_refs_confidence_not_null";
ALTER TABLE wiki_refs RENAME CONSTRAINT "wiki_entity_refs_created_at_not_null" TO "wiki_refs_created_at_not_null";
ALTER TABLE wiki_refs RENAME CONSTRAINT "wiki_entity_refs_entity_id_not_null" TO "wiki_refs_entity_id_not_null";
ALTER TABLE wiki_refs RENAME CONSTRAINT "wiki_entity_refs_entity_type_not_null" TO "wiki_refs_entity_type_not_null";
ALTER TABLE wiki_refs RENAME CONSTRAINT "wiki_entity_refs_id_not_null" TO "wiki_refs_id_not_null";
ALTER TABLE wiki_refs RENAME CONSTRAINT "wiki_entity_refs_metadata_not_null" TO "wiki_refs_metadata_not_null";
ALTER TABLE wiki_refs RENAME CONSTRAINT "wiki_entity_refs_pkey" TO "wiki_refs_pkey";
ALTER TABLE wiki_refs RENAME CONSTRAINT "wiki_entity_refs_resolved_by_not_null" TO "wiki_refs_resolved_by_not_null";
ALTER TABLE wiki_refs RENAME CONSTRAINT "wiki_entity_refs_source_id_not_null" TO "wiki_refs_source_id_not_null";
ALTER TABLE wiki_refs RENAME CONSTRAINT "wiki_entity_refs_source_table_not_null" TO "wiki_refs_source_table_not_null";
ALTER TABLE wiki_rules RENAME CONSTRAINT "wiki_standing_order_active_not_null" TO "wiki_rules_active_not_null";
ALTER TABLE wiki_rules RENAME CONSTRAINT "wiki_standing_order_created_at_not_null" TO "wiki_rules_created_at_not_null";
ALTER TABLE wiki_rules RENAME CONSTRAINT "wiki_standing_order_id_not_null" TO "wiki_rules_id_not_null";
ALTER TABLE wiki_rules RENAME CONSTRAINT "wiki_standing_order_kind_check" TO "wiki_rules_kind_check";
ALTER TABLE wiki_rules RENAME CONSTRAINT "wiki_standing_order_kind_not_null" TO "wiki_rules_kind_not_null";
ALTER TABLE wiki_rules RENAME CONSTRAINT "wiki_standing_order_pkey" TO "wiki_rules_pkey";
ALTER TABLE wiki_rules RENAME CONSTRAINT "wiki_standing_order_rule_not_null" TO "wiki_rules_rule_not_null";
ALTER TABLE wiki_rules RENAME CONSTRAINT "wiki_standing_order_updated_at_not_null" TO "wiki_rules_updated_at_not_null";
