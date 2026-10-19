-- The dead-table tier of the 2026-08-28 schema audit
-- (docs/schema-cleanup-checklist.md R1/R3-R6/R26/R27, every drop
-- adversarially re-verified). Code sweeps landed first — nothing in the
-- binary references any of these.

-- R1: consumers were two modules no mod.rs ever declared — Rust that never
-- compiled into any binary. 0 rows everywhere.
DROP TABLE IF EXISTS app_mcp_tools;
DROP TABLE IF EXISTS app_mcp_servers;

-- R3: the per-question interview, superseded 2026-08-27 by the interview
-- CHAT (chat_narrative_interview); the drafter reads the transcript.
DROP TABLE IF EXISTS wiki_narrative_interview;

-- R4: a Spotify ontology no collector ever fed. Descriptor, lane, and
-- dayline read removed in the same wave; if listening ever lands, it
-- returns WITH its collector.
DROP TABLE IF EXISTS data_activity_listening;

-- R5: written once per git import, read by nothing.
DROP TABLE IF EXISTS app_applet_package;

-- R6: a byte-clone archive only the sweeper wrote; the sweeper deletes
-- plainly now. Auth history with no query path is retention risk.
DROP TABLE IF EXISTS app_auth_event_archive;

-- R26: the quota subsystem enforced nothing — its counter was written by
-- three call sites that fired rarely-to-never, so every check_limit read
-- 0-of-N forever. app_ai_calls.cost_micros is the real accounting. If
-- caps on paid egress are ever wanted, they come back as a per-call
-- budget, not a tier table.
DROP TABLE IF EXISTS app_api_usage;
DROP TABLE IF EXISTS app_usage_limits;

-- R27: reserved for an owner-erasure sweeper that was never built (0003
-- documents the reservation). The schema returns with the feature.
DROP TABLE IF EXISTS app_erasure;
