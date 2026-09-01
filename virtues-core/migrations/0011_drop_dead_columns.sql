-- The column tier of the 2026-08-28 schema audit
-- (agents/plan/schema-cleanup-checklist.md R7-R25, adversarially verified;
-- every code sweep landed before this migration, per the ordering rule:
-- SELECT*/FromRow reads of a dropped column fail at runtime, not build time).

-- ── R7: indexes no query uses (four also die with their columns below —
--        IF EXISTS covers the cascade order) ──
DROP INDEX IF EXISTS idx_lake_objects_kind;
DROP INDEX IF EXISTS idx_lake_objects_stream;
DROP INDEX IF EXISTS idx_lake_objects_replay;
DROP INDEX IF EXISTS idx_backup_archived_file_increment;
DROP INDEX IF EXISTS idx_storage_volume_state;
DROP INDEX IF EXISTS idx_app_applet_runs_parent;
DROP INDEX IF EXISTS idx_calendar_block_type;
DROP INDEX IF EXISTS idx_wiki_days_dirty;
DROP INDEX IF EXISTS idx_wiki_events_dirty;

-- ── R8/R9: wiki ghosts. hrv_z is KEPT (autonomic scoring names it as
--           intended-future; Adam's call pending). ──
ALTER TABLE wiki_days
    DROP COLUMN IF EXISTS illustration,
    DROP COLUMN IF EXISTS morning_baseline,
    DROP COLUMN IF EXISTS battery_curve,
    DROP COLUMN IF EXISTS segmented_at,
    DROP COLUMN IF EXISTS dirty_at;
ALTER TABLE wiki_events
    DROP COLUMN IF EXISTS lof_raw,
    DROP COLUMN IF EXISTS user_created,
    DROP COLUMN IF EXISTS dirty_at;

-- ── R10: three never-written columns riding 100k+ rows ──
ALTER TABLE wiki_refs
    DROP COLUMN IF EXISTS confidence,
    DROP COLUMN IF EXISTS resolved_by,
    DROP COLUMN IF EXISTS metadata;

-- ── R11/R12/R16: app-side ghosts (all zero non-null on a fielded box) ──
ALTER TABLE app_chats
    DROP COLUMN IF EXISTS trace,
    DROP COLUMN IF EXISTS action_instruction;
ALTER TABLE app_applet_runs
    DROP COLUMN IF EXISTS parent_run_id,
    DROP COLUMN IF EXISTS transform_stage;
ALTER TABLE app_assistant_profile DROP COLUMN IF EXISTS embedding_model_id;
ALTER TABLE app_auth_user DROP COLUMN IF EXISTS is_owner;
ALTER TABLE app_ai_calls
    DROP COLUMN IF EXISTS chat_id,
    DROP COLUMN IF EXISTS status;
ALTER TABLE app_notebook_items DROP COLUMN IF EXISTS similarity;

-- ── R13: the drive tally graveyard ──
ALTER TABLE app_drive_usage
    DROP COLUMN IF EXISTS quota_bytes,
    DROP COLUMN IF EXISTS data_lake_bytes,
    DROP COLUMN IF EXISTS total_bytes,
    DROP COLUMN IF EXISTS last_scan_at,
    DROP COLUMN IF EXISTS last_scan_bytes,
    DROP COLUMN IF EXISTS trash_bytes,
    DROP COLUMN IF EXISTS trash_count,
    DROP COLUMN IF EXISTS warning_80_sent,
    DROP COLUMN IF EXISTS warning_90_sent,
    DROP COLUMN IF EXISTS warning_100_sent;

-- ── R14: discovery-survey residue ──
ALTER TABLE app_user_profile
    DROP COLUMN IF EXISTS crux,
    DROP COLUMN IF EXISTS technology_vision,
    DROP COLUMN IF EXISTS pain_point_primary,
    DROP COLUMN IF EXISTS pain_point_secondary,
    DROP COLUMN IF EXISTS excited_features;

-- ── R15: the retired reflections feature. On the one fielded box the
--        surviving rows were three empty pages and the single word "dyad";
--        machine-minted, unreachable since the route died, disposable. ──
DELETE FROM app_pages WHERE date IS NOT NULL;
ALTER TABLE app_pages DROP COLUMN IF EXISTS date;

-- ── R17-R20: data_* columns no collector has ever written (the demo seed,
--            their only writer, was swept first) ──
ALTER TABLE data_calendar_event
    DROP COLUMN IF EXISTS event_type,
    DROP COLUMN IF EXISTS conference_url,
    DROP COLUMN IF EXISTS conference_platform,
    DROP COLUMN IF EXISTS recurrence_rule,
    DROP COLUMN IF EXISTS timezone,
    DROP COLUMN IF EXISTS block_type;
ALTER TABLE data_activity_app_session
    DROP COLUMN IF EXISTS url,
    DROP COLUMN IF EXISTS document_path,
    DROP COLUMN IF EXISTS app_category;
ALTER TABLE data_activity_web_browsing
    DROP COLUMN IF EXISTS visit_duration_seconds,
    DROP COLUMN IF EXISTS scroll_depth_percent;
ALTER TABLE data_health_workout DROP COLUMN IF EXISTS route_geometry;
ALTER TABLE data_communication_transcription DROP COLUMN IF EXISTS speaker_segments;
ALTER TABLE data_financial_account DROP COLUMN IF EXISTS institution_id;
ALTER TABLE data_content_bookmark DROP COLUMN IF EXISTS content_type;
ALTER TABLE data_content_conversation
    DROP COLUMN IF EXISTS model,
    DROP COLUMN IF EXISTS tags,
    DROP COLUMN IF EXISTS metadata;

-- ── R21: search hash/meta fossils (doc_hash is the freshness key; the live
--        model fingerprint is env-based, never these) ──
ALTER TABLE search_embeddings DROP COLUMN IF EXISTS text_hash;
ALTER TABLE search_index_meta
    DROP COLUMN IF EXISTS fingerprint,
    DROP COLUMN IF EXISTS built_at;

-- ── R22: infra write-only columns. lake_objects.source_id is KEPT — it
--        carries the writing applet's ACTION, real provenance. ──
ALTER TABLE lake_objects DROP COLUMN IF EXISTS content_encoding;  -- constant 'none'
ALTER TABLE backup_archived_file
    DROP COLUMN IF EXISTS size_bytes,
    DROP COLUMN IF EXISTS archived_at;
ALTER TABLE storage_volume
    DROP COLUMN IF EXISTS capacity_bytes,
    DROP COLUMN IF EXISTS free_bytes,
    DROP COLUMN IF EXISTS probed_at,
    DROP COLUMN IF EXISTS last_error_at,
    DROP COLUMN IF EXISTS roles;  -- CHECK provably admitted exactly {'backup'}
ALTER TABLE credentials DROP COLUMN IF EXISTS scopes;

-- ── R23: narrow aspirational CHECKs to values the code produces.
--        credentials.status keeps 'reauth_required' (PRODUCED on provider-
--        rejected refresh) and 'error' (one line from produced). ──
ALTER TABLE lake_objects DROP CONSTRAINT IF EXISTS lake_objects_kind_check;
ALTER TABLE lake_objects ADD CONSTRAINT lake_objects_kind_check
    CHECK (kind IN ('raw_stream', 'media'));
ALTER TABLE storage_volume DROP CONSTRAINT IF EXISTS storage_volume_kind_check;
ALTER TABLE storage_volume ADD CONSTRAINT storage_volume_kind_check
    CHECK (kind IN ('removable'));
ALTER TABLE storage_volume DROP CONSTRAINT IF EXISTS storage_volume_state_check;
ALTER TABLE storage_volume ADD CONSTRAINT storage_volume_state_check
    CHECK (state IN ('present', 'absent'));
ALTER TABLE app_user_profile DROP CONSTRAINT IF EXISTS app_user_profile_onboarding_status_check;
ALTER TABLE app_user_profile ADD CONSTRAINT app_user_profile_onboarding_status_check
    CHECK (onboarding_status IN ('new', 'onboarding', 'active'));
