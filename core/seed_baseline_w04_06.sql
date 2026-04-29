-- =============================================================================
-- Baseline Seed: Weeks 4-6 — December 15, 2025 through January 4, 2026
-- Event IDs: ev_b0211 through ev_b0420
-- =============================================================================
--
-- Holiday period:
--   Dec 24-26: Off work (Christmas Eve, Christmas Day, day after)
--   Dec 22-23, Dec 29-30: Lighter work / WFH
--   Dec 31: New Year's Eve (social evening)
--   Jan 1: Quiet recovery day
--   Jan 2: Light WFH day
--
-- Game night at Jess's: Dec 19 (Fri), Jan 2 (Fri). Skipping Dec 26 (Christmas).
-- Mom call: Dec 20 (Sat), Dec 27 (Sat), Jan 3 (Sat)
-- No house-hunting, no Rachel.
-- =============================================================================

-- Idempotency: clear any existing events in this range
DELETE FROM wiki_events WHERE id LIKE 'ev_b0%' AND CAST(SUBSTR(id, 5) AS INTEGER) BETWEEN 211 AND 420;

-- ─────────────────────────────────────────────────────────────────────────────
-- WIKI DAYS
-- ─────────────────────────────────────────────────────────────────────────────

-- Week 4: Dec 15-21
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-12-15', '2025-12-15', 'America/Chicago', 'America/Chicago', 0.48);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-12-16', '2025-12-16', 'America/Chicago', 'America/Chicago', 0.52);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-12-17', '2025-12-17', 'America/Chicago', 'America/Chicago', 0.50);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-12-18', '2025-12-18', 'America/Chicago', 'America/Chicago', 0.45);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-12-19', '2025-12-19', 'America/Chicago', 'America/Chicago', 0.53);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-12-20', '2025-12-20', 'America/Chicago', 'America/Chicago', 0.55);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-12-21', '2025-12-21', 'America/Chicago', 'America/Chicago', 0.50);

-- Week 5: Dec 22-28 (Christmas week)
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-12-22', '2025-12-22', 'America/Chicago', 'America/Chicago', 0.47);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-12-23', '2025-12-23', 'America/Chicago', 'America/Chicago', 0.50);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-12-24', '2025-12-24', 'America/Chicago', 'America/Chicago', 0.55);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-12-25', '2025-12-25', 'America/Chicago', 'America/Chicago', 0.58);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-12-26', '2025-12-26', 'America/Chicago', 'America/Chicago', 0.52);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-12-27', '2025-12-27', 'America/Chicago', 'America/Chicago', 0.50);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-12-28', '2025-12-28', 'America/Chicago', 'America/Chicago', 0.48);

-- Week 6: Dec 29 - Jan 4 (New Year's week)
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-12-29', '2025-12-29', 'America/Chicago', 'America/Chicago', 0.46);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-12-30', '2025-12-30', 'America/Chicago', 'America/Chicago', 0.50);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-12-31', '2025-12-31', 'America/Chicago', 'America/Chicago', 0.52);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2026-01-01', '2026-01-01', 'America/Chicago', 'America/Chicago', 0.42);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2026-01-02', '2026-01-02', 'America/Chicago', 'America/Chicago', 0.45);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2026-01-03', '2026-01-03', 'America/Chicago', 'America/Chicago', 0.50);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2026-01-04', '2026-01-04', 'America/Chicago', 'America/Chicago', 0.48);


-- ─────────────────────────────────────────────────────────────────────────────
-- WIKI EVENTS
-- ─────────────────────────────────────────────────────────────────────────────
-- All times UTC (CST + 6). December/January is CST (UTC-6).
-- Midnight CST = 06:00 UTC, 6:30am CST = 12:30 UTC, etc.

-- =============================================================================
-- MONDAY December 15, 2025 — Normal weekday
-- =============================================================================

-- Sleep (00:00-06:30 CST = 06:00-12:30 UTC)
INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0211', 'day_2025-12-15', '2025-12-15T06:00:00Z', '2025-12-15T12:30:00Z', 'Sleep', 'Home', '["sleep"]', 0, 0, 1, 0, 0, 'Sleep from midnight to 6:30am, 6.5 hours.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 56);

-- Morning routine (06:30-07:15 CST = 12:30-13:15 UTC)
INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0212', 'day_2025-12-15', '2025-12-15T12:30:00Z', '2025-12-15T13:15:00Z', 'Morning routine', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Coffee and checking messages before heading out.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 63);

-- Bike commute (07:15-07:45 CST = 13:15-13:45 UTC)
INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0213', 'day_2025-12-15', '2025-12-15T13:15:00Z', '2025-12-15T13:45:00Z', 'Bike commute', NULL, '["location_visit", "steps"]', 0, 1, 0, 0, 0, 'Bike commute from Mueller to downtown office, chilly morning.', '["commute", "cycling", "morning"]', '[]', NULL, NULL, NULL, 'NEW', 133);

-- Coffee and Slack (07:45-08:15 CST = 13:45-14:15 UTC)
INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0214', 'day_2025-12-15', '2025-12-15T13:45:00Z', '2025-12-15T14:15:00Z', 'Coffee and Slack', 'Office', '["app_usage", "message"]', 0, 0, 0, 0, 0, 'Office coffee and catching up on Slack before standup.', '["messaging", "work"]', '["place_demo_office", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 69);

-- Design standup (08:15-08:45 CST = 14:15-14:45 UTC)
INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0215', 'day_2025-12-15', '2025-12-15T14:15:00Z', '2025-12-15T14:45:00Z', 'Design standup', 'Office', '["calendar", "transcription"]', 0, 0, 0, 0, 0, 'Monday standup with Maya and David, reviewing sprint priorities.', '["meeting", "standup", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 73);

-- Focused design work (09:00-11:30 CST = 15:00-17:30 UTC)
INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0216', 'day_2025-12-15', '2025-12-15T15:00:00Z', '2025-12-15T17:30:00Z', 'Focused design work', 'Office', '["app_usage"]', 0, 0, 0, 0, 0, 'Deep work in Figma on the settings page redesign.', '["design", "figma", "focus", "deep-work"]', '["place_demo_office", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 63);

-- Lunch solo (11:30-12:15 CST = 17:30-18:15 UTC)
INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0217', 'day_2025-12-15', '2025-12-15T17:30:00Z', '2025-12-15T18:15:00Z', 'Lunch', 'Office', '["location_visit"]', 0, 0, 0, 0, 0, 'Solo lunch at the office, ate leftover soup at her desk.', '["food"]', '["place_demo_office"]', NULL, NULL, NULL, 'NEW', 72);

-- Afternoon work (12:30-16:30 CST = 18:30-22:30 UTC)
INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0218', 'day_2025-12-15', '2025-12-15T18:30:00Z', '2025-12-15T22:30:00Z', 'Afternoon work', 'Office', '["app_usage"]', 0, 0, 0, 0, 0, 'Worked on component library updates and responded to design review comments.', '["work", "design", "figma", "code-review"]', '["place_demo_office", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 65);

-- Bike commute home (16:30-17:00 CST = 22:30-23:00 UTC)
INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0219', 'day_2025-12-15', '2025-12-15T22:30:00Z', '2025-12-15T23:00:00Z', 'Bike commute', NULL, '["location_visit", "steps"]', 0, 1, 0, 0, 0, 'Bike ride home from the office.', '["commute", "cycling"]', '[]', NULL, NULL, NULL, 'NEW', 131);

-- Evening at home (17:30-21:30 CST = 23:30-03:30+1 UTC)
INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0220', 'day_2025-12-15', '2025-12-15T23:30:00Z', '2025-12-16T03:30:00Z', 'Evening at home', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Made stir-fry for dinner, then read on the couch for a couple hours.', '["food", "leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 68);

-- Wind down (21:30-00:00 CST = 03:30-06:00+1 UTC)
INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0221', 'day_2025-12-15', '2025-12-16T03:30:00Z', '2025-12-16T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Browsed Reddit and watched a YouTube video before bed.', '["leisure", "browsing"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 58);

-- =============================================================================
-- TUESDAY December 16, 2025 — Normal weekday, evening run
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0222', 'day_2025-12-16', '2025-12-16T06:00:00Z', '2025-12-16T12:30:00Z', 'Sleep', 'Home', '["sleep"]', 0, 0, 1, 0, 0, 'Sleep from midnight to 6:30am, 6.5 hours.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 61);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0223', 'day_2025-12-16', '2025-12-16T12:30:00Z', '2025-12-16T13:15:00Z', 'Morning routine', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Morning coffee and scrolling through news.', '["routine", "morning", "coffee", "browsing"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 63);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0224', 'day_2025-12-16', '2025-12-16T13:15:00Z', '2025-12-16T13:45:00Z', 'Bike commute', NULL, '["location_visit", "steps"]', 0, 1, 0, 0, 0, 'Bike commute to the office, cold but clear.', '["commute", "cycling", "morning"]', '[]', NULL, NULL, NULL, 'NEW', 110);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0225', 'day_2025-12-16', '2025-12-16T13:45:00Z', '2025-12-16T14:15:00Z', 'Coffee and Slack', 'Office', '["app_usage", "message"]', 0, 0, 0, 0, 0, 'Checked Slack and email over coffee at the office.', '["messaging", "work"]', '["place_demo_office", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 66);

-- Standup + design review with David (Tuesday pattern)
INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0226', 'day_2025-12-16', '2025-12-16T14:15:00Z', '2025-12-16T15:15:00Z', 'Standup and design review', 'Office', '["calendar", "transcription"]', 0, 0, 0, 0, 0, 'Standup followed by design review with David on the dashboard components.', '["meeting", "standup", "design", "design-review"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 73);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0227', 'day_2025-12-16', '2025-12-16T15:15:00Z', '2025-12-16T17:30:00Z', 'Focused design work', 'Office', '["app_usage"]', 0, 0, 0, 0, 0, 'Iterated on dashboard wireframes in Figma after review feedback.', '["design", "figma", "focus", "deep-work"]', '["place_demo_office", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 63);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0228', 'day_2025-12-16', '2025-12-16T17:30:00Z', '2025-12-16T18:15:00Z', 'Lunch', 'Office', '["location_visit"]', 0, 0, 0, 0, 0, 'Grabbed a sandwich from the deli downstairs.', '["food"]', '["place_demo_office"]', NULL, NULL, NULL, 'NEW', 78);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0229', 'day_2025-12-16', '2025-12-16T18:30:00Z', '2025-12-16T22:30:00Z', 'Afternoon work', 'Office', '["app_usage"]', 0, 0, 0, 0, 0, 'Continued dashboard iteration and prepped assets for handoff.', '["work", "design", "figma"]', '["place_demo_office", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 64);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0230', 'day_2025-12-16', '2025-12-16T22:30:00Z', '2025-12-16T23:00:00Z', 'Bike commute', NULL, '["location_visit", "steps"]', 0, 1, 0, 0, 0, 'Bike ride home from the office.', '["commute", "cycling"]', '[]', NULL, NULL, NULL, 'NEW', 127);

-- Evening run on Mueller trails (Tuesday pattern)
INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0231', 'day_2025-12-16', '2025-12-16T23:30:00Z', '2025-12-17T00:15:00Z', 'Evening run', 'Mueller Trails', '["steps", "workout"]', 0, 0, 0, 0, 0, 'Evening run on the Mueller trails, 3.2 miles in the cold.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]', NULL, NULL, NULL, 'NEW', 63);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0232', 'day_2025-12-16', '2025-12-17T00:30:00Z', '2025-12-17T03:30:00Z', 'Evening at home', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Showered, heated up leftovers, watched an episode of a show.', '["food", "leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 68);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0233', 'day_2025-12-16', '2025-12-17T03:30:00Z', '2025-12-17T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Scrolled through Instagram and texted Jess about Friday plans.', '["leisure", "messaging"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 61);

-- =============================================================================
-- WEDNESDAY December 17, 2025 — Lunch at Tatsu-ya with Maya
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0234', 'day_2025-12-17', '2025-12-17T06:00:00Z', '2025-12-17T12:30:00Z', 'Sleep', 'Home', '["sleep"]', 0, 0, 1, 0, 0, 'Sleep from midnight to 6:30am, 6.5 hours.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 58);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0235', 'day_2025-12-17', '2025-12-17T12:30:00Z', '2025-12-17T13:15:00Z', 'Morning routine', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Coffee and catching up on Slack messages before heading out.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 66);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0236', 'day_2025-12-17', '2025-12-17T13:15:00Z', '2025-12-17T13:45:00Z', 'Bike commute', NULL, '["location_visit", "steps"]', 0, 1, 0, 0, 0, 'Bike commute to the office.', '["commute", "cycling", "morning"]', '[]', NULL, NULL, NULL, 'NEW', 128);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0237', 'day_2025-12-17', '2025-12-17T13:45:00Z', '2025-12-17T14:15:00Z', 'Coffee and Slack', 'Office', '["app_usage", "message"]', 0, 0, 0, 0, 0, 'Morning coffee and reviewing design feedback in Slack.', '["messaging", "work", "code-review"]', '["place_demo_office", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 69);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0238', 'day_2025-12-17', '2025-12-17T14:15:00Z', '2025-12-17T14:45:00Z', 'Design standup', 'Office', '["calendar", "transcription"]', 0, 0, 0, 0, 0, 'Wednesday standup, quick sync on year-end tasks.', '["meeting", "standup", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 70);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0239', 'day_2025-12-17', '2025-12-17T15:00:00Z', '2025-12-17T17:30:00Z', 'Focused work', 'Office', '["app_usage"]', 0, 0, 0, 0, 0, 'Deep work session on the settings page flow.', '["design", "figma", "focus", "deep-work"]', '["place_demo_office", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 68);

-- Wednesday: Lunch at Tatsu-ya with Maya
INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0240', 'day_2025-12-17', '2025-12-17T17:30:00Z', '2025-12-17T18:30:00Z', 'Lunch with Maya', 'Ramen Tatsu-ya', '["location_visit", "transcription"]', 0, 0, 0, 0, 0, 'Weekly lunch at Tatsu-ya with Maya, talked about holiday plans.', '["food", "social", "ramen"]', '["person_demo_maya", "place_demo_ramen"]', NULL, NULL, NULL, 'NEW', 72);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0241', 'day_2025-12-17', '2025-12-17T18:30:00Z', '2025-12-17T22:30:00Z', 'Afternoon work', 'Office', '["app_usage"]', 0, 0, 0, 0, 0, 'Finalized settings page mockups and shared with the team.', '["work", "design", "figma"]', '["place_demo_office", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 70);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0242', 'day_2025-12-17', '2025-12-17T22:30:00Z', '2025-12-17T23:00:00Z', 'Bike commute', NULL, '["location_visit", "steps"]', 0, 1, 0, 0, 0, 'Bike ride home from the office.', '["commute", "cycling"]', '[]', NULL, NULL, NULL, 'NEW', 120);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0243', 'day_2025-12-17', '2025-12-17T23:30:00Z', '2025-12-18T04:00:00Z', 'Evening at home', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Made pasta for dinner, then watched a documentary about architecture.', '["food", "leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 64);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0244', 'day_2025-12-17', '2025-12-18T04:00:00Z', '2025-12-18T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Reading in bed before falling asleep.', '["leisure", "reflection"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 59);

-- =============================================================================
-- THURSDAY December 18, 2025 — WFH afternoon, walk
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0245', 'day_2025-12-18', '2025-12-18T06:00:00Z', '2025-12-18T12:15:00Z', 'Sleep', 'Home', '["sleep"]', 0, 0, 1, 0, 0, 'Sleep from midnight to 6:15am, 6.25 hours.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 58);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0246', 'day_2025-12-18', '2025-12-18T12:15:00Z', '2025-12-18T13:15:00Z', 'Morning routine', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Slow morning, coffee and checking messages.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 65);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0247', 'day_2025-12-18', '2025-12-18T13:15:00Z', '2025-12-18T13:45:00Z', 'Bike commute', NULL, '["location_visit", "steps"]', 0, 1, 0, 0, 0, 'Bike commute to the office.', '["commute", "cycling", "morning"]', '[]', NULL, NULL, NULL, 'NEW', 113);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0248', 'day_2025-12-18', '2025-12-18T14:15:00Z', '2025-12-18T14:45:00Z', 'Design standup', 'Office', '["calendar", "transcription"]', 0, 0, 0, 0, 0, 'Thursday standup, most of the team already wrapping up before the holidays.', '["meeting", "standup", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 71);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0249', 'day_2025-12-18', '2025-12-18T15:00:00Z', '2025-12-18T17:30:00Z', 'Focused work', 'Office', '["app_usage"]', 0, 0, 0, 0, 0, 'Morning design work on year-end polish items.', '["design", "figma", "focus", "deep-work"]', '["place_demo_office", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 65);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0250', 'day_2025-12-18', '2025-12-18T17:30:00Z', '2025-12-18T18:15:00Z', 'Lunch', 'Office', '["location_visit"]', 0, 0, 0, 0, 0, 'Quick lunch at the office before heading home.', '["food"]', '["place_demo_office"]', NULL, NULL, NULL, 'NEW', 71);

-- WFH afternoon
INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0251', 'day_2025-12-18', '2025-12-18T18:30:00Z', '2025-12-18T19:00:00Z', 'Bike commute', NULL, '["location_visit", "steps"]', 0, 1, 0, 0, 0, 'Rode home early to work from home for the afternoon.', '["commute", "cycling"]', '[]', NULL, NULL, NULL, 'NEW', 121);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0252', 'day_2025-12-18', '2025-12-18T19:00:00Z', '2025-12-18T22:00:00Z', 'WFH afternoon', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Worked from home on Slack messages and final design tweaks before the holiday break.', '["work", "messaging", "design"]', '["place_demo_home", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 67);

-- Walk in the afternoon
INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0253', 'day_2025-12-18', '2025-12-18T22:30:00Z', '2025-12-18T23:15:00Z', 'Walk', 'Mueller Trails', '["steps"]', 0, 0, 0, 0, 0, 'Late afternoon walk around Mueller trails to clear her head.', '["exercise", "outdoors"]', '["place_demo_mueller_trails"]', NULL, NULL, NULL, 'NEW', 93);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0254', 'day_2025-12-18', '2025-12-18T23:30:00Z', '2025-12-19T04:00:00Z', 'Evening at home', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Cooked a big batch of chili for the week, listened to a podcast.', '["food", "leisure", "podcast"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 60);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0255', 'day_2025-12-18', '2025-12-19T04:00:00Z', '2025-12-19T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Browsed holiday gift ideas online.', '["leisure", "browsing", "errands"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 63);

-- =============================================================================
-- FRIDAY December 19, 2025 — Shorter day, game night at Jess's, Mom call
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0256', 'day_2025-12-19', '2025-12-19T06:00:00Z', '2025-12-19T12:30:00Z', 'Sleep', 'Home', '["sleep"]', 0, 0, 1, 0, 0, 'Sleep from midnight to 6:30am, 6.5 hours.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 62);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0257', 'day_2025-12-19', '2025-12-19T12:30:00Z', '2025-12-19T13:15:00Z', 'Morning routine', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Coffee and quick morning browse.', '["routine", "morning", "coffee"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 67);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0258', 'day_2025-12-19', '2025-12-19T13:15:00Z', '2025-12-19T13:45:00Z', 'Bike commute', NULL, '["location_visit", "steps"]', 0, 1, 0, 0, 0, 'Bike commute to the office.', '["commute", "cycling", "morning"]', '[]', NULL, NULL, NULL, 'NEW', 113);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0259', 'day_2025-12-19', '2025-12-19T14:15:00Z', '2025-12-19T14:45:00Z', 'Design standup', 'Office', '["calendar", "transcription"]', 0, 0, 0, 0, 0, 'Friday standup, last one before the holiday break.', '["meeting", "standup", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 76);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0260', 'day_2025-12-19', '2025-12-19T15:00:00Z', '2025-12-19T17:30:00Z', 'Focused work', 'Office', '["app_usage"]', 0, 0, 0, 0, 0, 'Wrapping up loose ends before the holiday break.', '["work", "design", "figma"]', '["place_demo_office", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 62);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0261', 'day_2025-12-19', '2025-12-19T17:30:00Z', '2025-12-19T18:15:00Z', 'Lunch', 'Office', '["location_visit"]', 0, 0, 0, 0, 0, 'Lunch in the break room, the office already felt half-empty.', '["food"]', '["place_demo_office"]', NULL, NULL, NULL, 'NEW', 78);

-- Left early on Friday
INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0262', 'day_2025-12-19', '2025-12-19T20:00:00Z', '2025-12-19T20:30:00Z', 'Bike commute', NULL, '["location_visit", "steps"]', 0, 1, 0, 0, 0, 'Left the office early, biked home.', '["commute", "cycling"]', '[]', NULL, NULL, NULL, 'NEW', 119);

-- Mom call (Friday evening)
INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0263', 'day_2025-12-19', '2025-12-19T23:00:00Z', '2025-12-19T23:45:00Z', 'Phone call with Mom', 'Home', '["transcription"]', 0, 0, 0, 0, 0, 'Weekly call with Mom, talked about Christmas plans and what to bring.', '["family", "phone-call"]', '["person_demo_mom", "place_demo_home"]', NULL, NULL, NULL, 'NEW', 70);

-- Game night at Jess's
INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0264', 'day_2025-12-19', '2025-12-20T01:00:00Z', '2025-12-20T05:00:00Z', 'Game night', 'Jess''s Place', '["location_visit", "transcription"]', 0, 0, 0, 0, 0, 'Game night at Jess''s with Priya, played Catan and drank mulled wine.', '["social", "games"]', '["person_demo_jess", "person_demo_priya", "place_demo_jess"]', NULL, NULL, NULL, 'NEW', 71);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0265', 'day_2025-12-19', '2025-12-20T05:00:00Z', '2025-12-20T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Got home late from game night, quick scroll before bed.', '["leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 63);

-- =============================================================================
-- SATURDAY December 20, 2025 — Lady Bird Lake walk, errands
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0266', 'day_2025-12-20', '2025-12-20T06:00:00Z', '2025-12-20T13:30:00Z', 'Sleep', 'Home', '["sleep"]', 0, 0, 1, 0, 0, 'Slept in after game night, midnight to 7:30am.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 56);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0267', 'day_2025-12-20', '2025-12-20T13:30:00Z', '2025-12-20T15:00:00Z', 'Slow morning', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Lazy Saturday morning, coffee and browsing.', '["routine", "morning", "coffee", "leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 63);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0268', 'day_2025-12-20', '2025-12-20T15:00:00Z', '2025-12-20T16:30:00Z', 'Walk at Lady Bird Lake', 'Lady Bird Lake', '["steps", "location_visit"]', 0, 0, 0, 0, 0, 'Morning walk along Lady Bird Lake, crisp winter air.', '["exercise", "outdoors"]', '["place_demo_ladybird"]', NULL, NULL, NULL, 'NEW', 92);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0269', 'day_2025-12-20', '2025-12-20T17:00:00Z', '2025-12-20T19:00:00Z', 'Holiday errands', NULL, '["location_visit"]', 0, 0, 0, 0, 0, 'Holiday shopping and picking up last-minute gifts.', '["leisure", "errands"]', '[]', NULL, NULL, NULL, 'NEW', 76);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0270', 'day_2025-12-20', '2025-12-20T19:30:00Z', '2025-12-20T23:00:00Z', 'Afternoon at home', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Wrapped presents and watched holiday baking shows.', '["leisure", "family"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 59);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0271', 'day_2025-12-20', '2025-12-20T23:00:00Z', '2025-12-21T01:00:00Z', 'Dinner', 'Home', '["location_visit"]', 0, 0, 0, 0, 0, 'Made tacos for dinner and ate on the couch.', '["food"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 68);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0272', 'day_2025-12-20', '2025-12-21T01:00:00Z', '2025-12-21T06:00:00Z', 'Evening and wind down', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Watched a movie and then read before bed.', '["leisure", "reflection"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 64);

-- =============================================================================
-- SUNDAY December 21, 2025 — Slow day, cooking, reading
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0273', 'day_2025-12-21', '2025-12-21T06:00:00Z', '2025-12-21T14:00:00Z', 'Sleep', 'Home', '["sleep"]', 0, 0, 1, 0, 0, 'Sleep from midnight to 8am, nice long sleep.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 56);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0274', 'day_2025-12-21', '2025-12-21T14:00:00Z', '2025-12-21T15:30:00Z', 'Slow morning', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Lazy Sunday morning, coffee and reading.', '["routine", "morning", "coffee", "leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 66);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0275', 'day_2025-12-21', '2025-12-21T16:00:00Z', '2025-12-21T17:00:00Z', 'Run', 'Mueller Trails', '["steps", "workout"]', 0, 0, 0, 0, 0, 'Easy Sunday run on the Mueller trails, 2.5 miles.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]', NULL, NULL, NULL, 'NEW', 149);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0276', 'day_2025-12-21', '2025-12-21T17:30:00Z', '2025-12-21T20:00:00Z', 'Cooking', 'Home', '["location_visit"]', 0, 0, 0, 0, 0, 'Batch cooking for the week — soup and roasted vegetables.', '["food", "cooking"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 75);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0277', 'day_2025-12-21', '2025-12-21T20:00:00Z', '2025-12-22T01:00:00Z', 'Afternoon reading', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Read for a few hours and did some light tidying up.', '["leisure", "reflection"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 63);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0278', 'day_2025-12-21', '2025-12-22T01:00:00Z', '2025-12-22T04:00:00Z', 'Evening', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Watched a holiday movie and packed for Christmas travel.', '["leisure", "family"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 62);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0279', 'day_2025-12-21', '2025-12-22T04:00:00Z', '2025-12-22T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Pre-sleep browsing and setting alarms for tomorrow.', '["leisure", "browsing"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 60);

-- =============================================================================
-- MONDAY December 22, 2025 — Light WFH day (holiday week starts)
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0280', 'day_2025-12-22', '2025-12-22T06:00:00Z', '2025-12-22T12:30:00Z', 'Sleep', 'Home', '["sleep"]', 0, 0, 1, 0, 0, 'Sleep from midnight to 6:30am, 6.5 hours.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 60);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0281', 'day_2025-12-22', '2025-12-22T12:30:00Z', '2025-12-22T13:30:00Z', 'Morning routine', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Coffee and checking Slack, most channels pretty quiet.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 64);

-- WFH for the day
INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0282', 'day_2025-12-22', '2025-12-22T14:00:00Z', '2025-12-22T17:00:00Z', 'WFH morning', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Light work from home, tying up loose ends and writing documentation.', '["work"]', '["place_demo_home", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 68);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0283', 'day_2025-12-22', '2025-12-22T17:00:00Z', '2025-12-22T18:00:00Z', 'Lunch', 'Home', '["location_visit"]', 0, 0, 0, 0, 0, 'Leftover chili for lunch.', '["food"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 74);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0284', 'day_2025-12-22', '2025-12-22T18:00:00Z', '2025-12-22T20:00:00Z', 'WFH afternoon', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'A bit more work, then signed off early for the holidays.', '["work"]', '["place_demo_home", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 63);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0285', 'day_2025-12-22', '2025-12-22T21:00:00Z', '2025-12-22T22:00:00Z', 'Walk', 'Mueller Trails', '["steps"]', 0, 0, 0, 0, 0, 'Afternoon walk through the neighborhood.', '["exercise", "outdoors"]', '["place_demo_mueller_trails"]', NULL, NULL, NULL, 'NEW', 90);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0286', 'day_2025-12-22', '2025-12-22T23:00:00Z', '2025-12-23T03:00:00Z', 'Evening at home', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Baked cookies for Christmas, listened to holiday music.', '["food", "leisure", "cooking", "family"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 68);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0287', 'day_2025-12-22', '2025-12-23T03:00:00Z', '2025-12-23T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Watched a few YouTube videos before bed.', '["leisure", "browsing"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 63);

-- =============================================================================
-- TUESDAY December 23, 2025 — Light WFH, holiday prep
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0288', 'day_2025-12-23', '2025-12-23T06:00:00Z', '2025-12-23T13:00:00Z', 'Sleep', 'Home', '["sleep"]', 0, 0, 1, 0, 0, 'Sleep from midnight to 7am.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 58);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0289', 'day_2025-12-23', '2025-12-23T13:00:00Z', '2025-12-23T14:00:00Z', 'Morning routine', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Slow morning coffee, reading holiday recipes.', '["routine", "morning", "coffee"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 64);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0290', 'day_2025-12-23', '2025-12-23T14:00:00Z', '2025-12-23T16:00:00Z', 'WFH morning', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Checked in on a few things, mostly quiet — half the team already off.', '["work", "messaging"]', '["place_demo_home", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 66);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0291', 'day_2025-12-23', '2025-12-23T17:00:00Z', '2025-12-23T19:00:00Z', 'Holiday errands', NULL, '["location_visit"]', 0, 0, 0, 0, 0, 'Ran out to pick up groceries and a last-minute gift.', '["leisure", "errands"]', '[]', NULL, NULL, NULL, 'NEW', 78);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0292', 'day_2025-12-23', '2025-12-23T19:30:00Z', '2025-12-23T22:00:00Z', 'Cooking', 'Home', '["location_visit"]', 0, 0, 0, 0, 0, 'Prepped food for Christmas Eve dinner.', '["food", "family"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 72);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0293', 'day_2025-12-23', '2025-12-23T22:00:00Z', '2025-12-24T01:00:00Z', 'Evening', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Wrapped the last presents and watched a holiday movie.', '["leisure", "family"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 68);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0294', 'day_2025-12-23', '2025-12-24T01:00:00Z', '2025-12-24T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Read in bed and fell asleep early.', '["leisure", "reflection"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 59);

-- =============================================================================
-- WEDNESDAY December 24, 2025 — Christmas Eve (off work, cozy day)
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0295', 'day_2025-12-24', '2025-12-24T06:00:00Z', '2025-12-24T13:30:00Z', 'Sleep', 'Home', '["sleep"]', 0, 0, 1, 0, 0, 'Sleep from midnight to 7:30am.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 60);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0296', 'day_2025-12-24', '2025-12-24T13:30:00Z', '2025-12-24T15:00:00Z', 'Slow morning', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Christmas Eve morning, coffee and cinnamon rolls.', '["routine", "morning", "coffee", "food"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 63);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0297', 'day_2025-12-24', '2025-12-24T15:00:00Z', '2025-12-24T16:00:00Z', 'Walk', 'Mueller Trails', '["steps"]', 0, 0, 0, 0, 0, 'Short walk through the Mueller neighborhood, holiday lights everywhere.', '["exercise", "outdoors"]', '["place_demo_mueller_trails"]', NULL, NULL, NULL, 'NEW', 92);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0298', 'day_2025-12-24', '2025-12-24T17:00:00Z', '2025-12-24T20:00:00Z', 'Christmas Eve cooking', 'Home', '["location_visit"]', 0, 0, 0, 0, 0, 'Spent the afternoon cooking Christmas Eve dinner.', '["food", "family"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 68);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0299', 'day_2025-12-24', '2025-12-25T00:00:00Z', '2025-12-25T00:45:00Z', 'Phone call with Mom', 'Home', '["transcription"]', 0, 0, 0, 0, 0, 'FaceTime with Mom on Christmas Eve, she showed off the tree.', '["family", "phone-call"]', '["person_demo_mom", "place_demo_home"]', NULL, NULL, NULL, 'NEW', 70);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0300', 'day_2025-12-24', '2025-12-25T01:00:00Z', '2025-12-25T04:00:00Z', 'Christmas Eve evening', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Quiet evening at home, watched It''s a Wonderful Life.', '["leisure", "family"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 66);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0301', 'day_2025-12-24', '2025-12-25T04:00:00Z', '2025-12-25T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Texted friends Merry Christmas and fell asleep.', '["messaging", "leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 60);

-- =============================================================================
-- THURSDAY December 25, 2025 — Christmas Day (quiet, home)
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0302', 'day_2025-12-25', '2025-12-25T06:00:00Z', '2025-12-25T14:00:00Z', 'Sleep', 'Home', '["sleep"]', 0, 0, 1, 0, 0, 'Slept in on Christmas morning, midnight to 8am.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 56);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0303', 'day_2025-12-25', '2025-12-25T14:00:00Z', '2025-12-25T16:00:00Z', 'Christmas morning', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Slow Christmas morning, opened a gift from Mom that arrived in the mail.', '["routine", "morning", "coffee", "family"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 64);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0304', 'day_2025-12-25', '2025-12-25T16:00:00Z', '2025-12-25T17:00:00Z', 'Phone call with Mom', 'Home', '["transcription"]', 0, 0, 0, 0, 0, 'Long Christmas morning call with Mom, caught up on family news.', '["family", "phone-call"]', '["person_demo_mom", "place_demo_home"]', NULL, NULL, NULL, 'NEW', 70);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0305', 'day_2025-12-25', '2025-12-25T17:30:00Z', '2025-12-25T19:00:00Z', 'Walk', 'Mueller Trails', '["steps"]', 0, 0, 0, 0, 0, 'Christmas walk around Mueller, streets were quiet.', '["exercise", "outdoors"]', '["place_demo_mueller_trails"]', NULL, NULL, NULL, 'NEW', 91);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0306', 'day_2025-12-25', '2025-12-25T19:30:00Z', '2025-12-25T22:00:00Z', 'Christmas cooking', 'Home', '["location_visit"]', 0, 0, 0, 0, 0, 'Made a proper Christmas dinner for herself — roasted chicken and potatoes.', '["food", "family"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 75);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0307', 'day_2025-12-25', '2025-12-25T22:00:00Z', '2025-12-26T04:00:00Z', 'Christmas evening', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Curled up with a new book she got as a gift, then watched a movie.', '["leisure", "family"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 66);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0308', 'day_2025-12-25', '2025-12-26T04:00:00Z', '2025-12-26T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Light browsing before bed, peaceful Christmas night.', '["leisure", "browsing", "family"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 63);

-- =============================================================================
-- FRIDAY December 26, 2025 — Day off, quiet recovery (no game night — Christmas)
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0309', 'day_2025-12-26', '2025-12-26T06:00:00Z', '2025-12-26T14:00:00Z', 'Sleep', 'Home', '["sleep"]', 0, 0, 1, 0, 0, 'Slept in, midnight to 8am.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 62);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0310', 'day_2025-12-26', '2025-12-26T14:00:00Z', '2025-12-26T15:30:00Z', 'Slow morning', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Leisurely morning with coffee and leftover Christmas food.', '["routine", "morning", "coffee", "food"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 64);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0311', 'day_2025-12-26', '2025-12-26T16:00:00Z', '2025-12-26T17:30:00Z', 'Walk at Lady Bird Lake', 'Lady Bird Lake', '["steps", "location_visit"]', 0, 0, 0, 0, 0, 'Post-Christmas walk along Lady Bird Lake, beautiful winter day.', '["exercise", "outdoors"]', '["place_demo_ladybird"]', NULL, NULL, NULL, 'NEW', 93);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0312', 'day_2025-12-26', '2025-12-26T18:00:00Z', '2025-12-26T22:00:00Z', 'Afternoon at home', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Read her new book for most of the afternoon.', '["leisure", "reflection"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 60);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0313', 'day_2025-12-26', '2025-12-26T22:00:00Z', '2025-12-27T00:00:00Z', 'Dinner', 'Home', '["location_visit"]', 0, 0, 0, 0, 0, 'Used up Christmas leftovers for a simple dinner.', '["food"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 68);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0314', 'day_2025-12-26', '2025-12-27T00:00:00Z', '2025-12-27T04:00:00Z', 'Evening', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Watched a couple episodes of a show and messaged Jess about New Year''s plans.', '["leisure", "messaging"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 68);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0315', 'day_2025-12-26', '2025-12-27T04:00:00Z', '2025-12-27T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Browsed online sales before bed.', '["leisure", "browsing", "errands"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 62);

-- =============================================================================
-- SATURDAY December 27, 2025 — Errands, Mom call, quiet
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0316', 'day_2025-12-27', '2025-12-27T06:00:00Z', '2025-12-27T13:30:00Z', 'Sleep', 'Home', '["sleep"]', 0, 0, 1, 0, 0, 'Sleep from midnight to 7:30am.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 59);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0317', 'day_2025-12-27', '2025-12-27T13:30:00Z', '2025-12-27T15:00:00Z', 'Slow morning', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Coffee and catching up on messages from the holidays.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 68);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0318', 'day_2025-12-27', '2025-12-27T16:00:00Z', '2025-12-27T17:00:00Z', 'Run', 'Mueller Trails', '["steps", "workout"]', 0, 0, 0, 0, 0, 'Saturday morning run on Mueller trails, working off Christmas food.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]', NULL, NULL, NULL, 'NEW', 154);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0319', 'day_2025-12-27', '2025-12-27T18:00:00Z', '2025-12-27T20:00:00Z', 'Errands', NULL, '["location_visit"]', 0, 0, 0, 0, 0, 'Grocery run and returned a couple of things at the store.', '["leisure", "errands"]', '[]', NULL, NULL, NULL, 'NEW', 78);

-- Mom call (Saturday)
INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0320', 'day_2025-12-27', '2025-12-27T22:00:00Z', '2025-12-27T22:40:00Z', 'Phone call with Mom', 'Home', '["transcription"]', 0, 0, 0, 0, 0, 'Called Mom to recap Christmas and talk about New Year''s plans.', '["family", "phone-call"]', '["person_demo_mom", "place_demo_home"]', NULL, NULL, NULL, 'NEW', 71);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0321', 'day_2025-12-27', '2025-12-27T23:00:00Z', '2025-12-28T03:00:00Z', 'Evening at home', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Cooked a simple stir-fry and watched a movie.', '["food", "leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 65);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0322', 'day_2025-12-27', '2025-12-28T03:00:00Z', '2025-12-28T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Read before bed.', '["leisure", "reflection"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 59);

-- =============================================================================
-- SUNDAY December 28, 2025 — Quiet Sunday, reading, cooking
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0323', 'day_2025-12-28', '2025-12-28T06:00:00Z', '2025-12-28T14:00:00Z', 'Sleep', 'Home', '["sleep"]', 0, 0, 1, 0, 0, 'Sleep from midnight to 8am.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 57);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0324', 'day_2025-12-28', '2025-12-28T14:00:00Z', '2025-12-28T15:30:00Z', 'Slow morning', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Lazy Sunday, coffee and reading.', '["routine", "morning", "coffee", "leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 67);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0325', 'day_2025-12-28', '2025-12-28T16:00:00Z', '2025-12-28T17:30:00Z', 'Walk at Lady Bird Lake', 'Lady Bird Lake', '["steps", "location_visit"]', 0, 0, 0, 0, 0, 'Sunday walk along Lady Bird Lake.', '["exercise", "outdoors"]', '["place_demo_ladybird"]', NULL, NULL, NULL, 'NEW', 100);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0326', 'day_2025-12-28', '2025-12-28T18:00:00Z', '2025-12-28T20:00:00Z', 'Cooking', 'Home', '["location_visit"]', 0, 0, 0, 0, 0, 'Made a big pot of lentil soup for the week.', '["food", "cooking"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 69);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0327', 'day_2025-12-28', '2025-12-28T20:00:00Z', '2025-12-29T01:00:00Z', 'Afternoon at home', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Read and did some light journaling about the year.', '["leisure", "reflection"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 58);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0328', 'day_2025-12-28', '2025-12-29T01:00:00Z', '2025-12-29T04:00:00Z', 'Evening', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Watched a documentary and had soup for dinner.', '["leisure", "food"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 61);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0329', 'day_2025-12-28', '2025-12-29T04:00:00Z', '2025-12-29T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Browsing and reading before bed.', '["leisure", "browsing", "reflection"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 59);

-- =============================================================================
-- MONDAY December 29, 2025 — Light WFH day (holiday week)
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0330', 'day_2025-12-29', '2025-12-29T06:00:00Z', '2025-12-29T13:00:00Z', 'Sleep', 'Home', '["sleep"]', 0, 0, 1, 0, 0, 'Sleep from midnight to 7am.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 57);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0331', 'day_2025-12-29', '2025-12-29T13:00:00Z', '2025-12-29T14:00:00Z', 'Morning routine', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Coffee and checking in on Slack, still pretty quiet.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 68);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0332', 'day_2025-12-29', '2025-12-29T14:00:00Z', '2025-12-29T17:00:00Z', 'WFH morning', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Light work from home, cleaning up Jira tickets and design files.', '["work", "design"]', '["place_demo_home", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 66);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0333', 'day_2025-12-29', '2025-12-29T17:00:00Z', '2025-12-29T18:00:00Z', 'Lunch', 'Home', '["location_visit"]', 0, 0, 0, 0, 0, 'Lentil soup from yesterday for lunch.', '["food"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 71);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0334', 'day_2025-12-29', '2025-12-29T18:00:00Z', '2025-12-29T20:00:00Z', 'WFH afternoon', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Wrapped up a few things and signed off for the day.', '["work"]', '["place_demo_home", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 68);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0335', 'day_2025-12-29', '2025-12-29T21:00:00Z', '2025-12-29T22:00:00Z', 'Run', 'Mueller Trails', '["steps", "workout"]', 0, 0, 0, 0, 0, 'Afternoon run on Mueller trails, 3 miles.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]', NULL, NULL, NULL, 'NEW', 151);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0336', 'day_2025-12-29', '2025-12-29T23:00:00Z', '2025-12-30T03:00:00Z', 'Evening at home', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Made a simple dinner and started a new show.', '["food", "leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 67);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0337', 'day_2025-12-29', '2025-12-30T03:00:00Z', '2025-12-30T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Browsed year-end lists and articles before bed.', '["leisure", "browsing", "reflection"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 62);

-- =============================================================================
-- TUESDAY December 30, 2025 — Light WFH day
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0338', 'day_2025-12-30', '2025-12-30T06:00:00Z', '2025-12-30T12:30:00Z', 'Sleep', 'Home', '["sleep"]', 0, 0, 1, 0, 0, 'Sleep from midnight to 6:30am.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 59);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0339', 'day_2025-12-30', '2025-12-30T12:30:00Z', '2025-12-30T13:30:00Z', 'Morning routine', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Coffee and catching up on messages.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 67);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0340', 'day_2025-12-30', '2025-12-30T14:00:00Z', '2025-12-30T17:00:00Z', 'WFH morning', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Worked on organizing design files and reviewing Q1 roadmap drafts.', '["work", "design", "figma"]', '["place_demo_home", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 63);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0341', 'day_2025-12-30', '2025-12-30T17:00:00Z', '2025-12-30T18:00:00Z', 'Lunch', 'Home', '["location_visit"]', 0, 0, 0, 0, 0, 'Soup and bread for lunch.', '["food"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 71);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0342', 'day_2025-12-30', '2025-12-30T18:00:00Z', '2025-12-30T20:00:00Z', 'WFH afternoon', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Quick video call with Maya to sync on January plans, then signed off.', '["work", "meeting"]', '["person_demo_maya", "place_demo_home", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 70);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0343', 'day_2025-12-30', '2025-12-30T21:00:00Z', '2025-12-30T22:00:00Z', 'Walk', 'Mueller Trails', '["steps"]', 0, 0, 0, 0, 0, 'Afternoon walk through the neighborhood.', '["exercise", "outdoors"]', '["place_demo_mueller_trails"]', NULL, NULL, NULL, 'NEW', 93);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0344', 'day_2025-12-30', '2025-12-30T23:00:00Z', '2025-12-31T03:00:00Z', 'Evening at home', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Made rice and vegetables for dinner, then binge-watched a show.', '["food", "leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 65);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0345', 'day_2025-12-30', '2025-12-31T03:00:00Z', '2025-12-31T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Read a bit before bed, thinking about New Year''s resolutions.', '["leisure", "reflection"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 58);

-- =============================================================================
-- WEDNESDAY December 31, 2025 — New Year's Eve (social evening with Jess & Priya)
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0346', 'day_2025-12-31', '2025-12-31T06:00:00Z', '2025-12-31T13:00:00Z', 'Sleep', 'Home', '["sleep"]', 0, 0, 1, 0, 0, 'Sleep from midnight to 7am.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 59);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0347', 'day_2025-12-31', '2025-12-31T13:00:00Z', '2025-12-31T14:30:00Z', 'Slow morning', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Slow morning on New Year''s Eve, coffee and planning the day.', '["routine", "morning", "coffee"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 66);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0348', 'day_2025-12-31', '2025-12-31T15:00:00Z', '2025-12-31T16:30:00Z', 'Walk at Lady Bird Lake', 'Lady Bird Lake', '["steps", "location_visit"]', 0, 0, 0, 0, 0, 'Late morning walk along Lady Bird Lake, reflecting on the year.', '["exercise", "outdoors", "reflection"]', '["place_demo_ladybird"]', NULL, NULL, NULL, 'NEW', 90);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0349', 'day_2025-12-31', '2025-12-31T17:00:00Z', '2025-12-31T19:00:00Z', 'Afternoon at home', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Tidied up the apartment and did some year-end journaling.', '["leisure", "reflection"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 77);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0350', 'day_2025-12-31', '2025-12-31T22:00:00Z', '2025-12-31T23:00:00Z', 'Getting ready', 'Home', '["location_visit"]', 0, 0, 0, 0, 0, 'Got ready and made appetizers to bring to Jess''s NYE party.', '["routine", "food"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 68);

-- NYE party at Jess's
INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0351', 'day_2025-12-31', '2026-01-01T00:00:00Z', '2026-01-01T06:30:00Z', 'New Year''s Eve at Jess''s', 'Jess''s Place', '["location_visit", "transcription"]', 0, 0, 0, 0, 0, 'New Year''s Eve party at Jess''s with Priya and a few others, champagne at midnight.', '["social", "games"]', '["person_demo_jess", "person_demo_priya", "place_demo_jess"]', NULL, NULL, NULL, 'NEW', 74);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0352', 'day_2025-12-31', '2026-01-01T06:30:00Z', '2026-01-01T07:00:00Z', 'Wind down', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Got home around 12:30am and fell straight into bed.', '["leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 62);

-- =============================================================================
-- THURSDAY January 1, 2026 — New Year's Day (quiet recovery)
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0353', 'day_2026-01-01', '2026-01-01T07:00:00Z', '2026-01-01T15:00:00Z', 'Sleep', 'Home', '["sleep"]', 0, 0, 1, 0, 0, 'Slept in after New Year''s Eve, 1am to 9am.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 57);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0354', 'day_2026-01-01', '2026-01-01T15:00:00Z', '2026-01-01T16:30:00Z', 'Slow morning', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Very slow New Year''s morning, coffee and toast.', '["routine", "morning", "coffee", "food"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 67);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0355', 'day_2026-01-01', '2026-01-01T17:00:00Z', '2026-01-01T18:00:00Z', 'Walk', 'Mueller Trails', '["steps"]', 0, 0, 0, 0, 0, 'Short New Year''s Day walk to get some fresh air.', '["exercise", "outdoors"]', '["place_demo_mueller_trails"]', NULL, NULL, NULL, 'NEW', 88);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0356', 'day_2026-01-01', '2026-01-01T18:30:00Z', '2026-01-01T22:00:00Z', 'Afternoon at home', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Spent the afternoon on the couch reading and writing New Year''s goals.', '["leisure", "reflection"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 62);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0357', 'day_2026-01-01', '2026-01-01T22:00:00Z', '2026-01-02T01:00:00Z', 'Dinner', 'Home', '["location_visit"]', 0, 0, 0, 0, 0, 'Made a simple dinner and watched the first episode of a new show.', '["food", "leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 68);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0358', 'day_2026-01-01', '2026-01-02T01:00:00Z', '2026-01-02T05:00:00Z', 'Evening', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Quiet evening, journaling about plans for the new year.', '["leisure", "reflection"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 62);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0359', 'day_2026-01-01', '2026-01-02T05:00:00Z', '2026-01-02T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Early to bed, ready to get back to normal.', '["leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 60);

-- =============================================================================
-- FRIDAY January 2, 2026 — Light WFH, game night at Jess's
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0360', 'day_2026-01-02', '2026-01-02T06:00:00Z', '2026-01-02T12:30:00Z', 'Sleep', 'Home', '["sleep"]', 0, 0, 1, 0, 0, 'Sleep from midnight to 6:30am.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 57);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0361', 'day_2026-01-02', '2026-01-02T12:30:00Z', '2026-01-02T13:30:00Z', 'Morning routine', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Coffee and Slack, team starting to come back online.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 67);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0362', 'day_2026-01-02', '2026-01-02T14:00:00Z', '2026-01-02T17:00:00Z', 'WFH morning', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Eased back into work from home, reviewed Q1 priorities and cleared out email.', '["work", "onboarding"]', '["place_demo_home", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 67);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0363', 'day_2026-01-02', '2026-01-02T17:00:00Z', '2026-01-02T18:00:00Z', 'Lunch', 'Home', '["location_visit"]', 0, 0, 0, 0, 0, 'Leftover soup for lunch.', '["food"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 70);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0364', 'day_2026-01-02', '2026-01-02T18:00:00Z', '2026-01-02T20:00:00Z', 'WFH afternoon', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Caught up with David on Slack about a design spec, signed off early.', '["work", "messaging", "design-review"]', '["person_demo_david", "place_demo_home", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 67);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0365', 'day_2026-01-02', '2026-01-02T21:00:00Z', '2026-01-02T21:45:00Z', 'Phone call with Mom', 'Home', '["transcription"]', 0, 0, 0, 0, 0, 'Quick call with Mom, talked about how the holidays went.', '["family", "phone-call"]', '["person_demo_mom", "place_demo_home"]', NULL, NULL, NULL, 'NEW', 72);

-- Game night at Jess's
INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0366', 'day_2026-01-02', '2026-01-03T01:00:00Z', '2026-01-03T05:00:00Z', 'Game night', 'Jess''s Place', '["location_visit", "transcription"]', 0, 0, 0, 0, 0, 'First game night of the new year at Jess''s, played Ticket to Ride with Priya.', '["social", "games"]', '["person_demo_jess", "person_demo_priya", "place_demo_jess"]', NULL, NULL, NULL, 'NEW', 68);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0367', 'day_2026-01-02', '2026-01-03T05:00:00Z', '2026-01-03T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Got home and went straight to bed.', '["leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 58);

-- =============================================================================
-- SATURDAY January 3, 2026 — Lady Bird Lake, errands, Mom call
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0368', 'day_2026-01-03', '2026-01-03T06:00:00Z', '2026-01-03T13:30:00Z', 'Sleep', 'Home', '["sleep"]', 0, 0, 1, 0, 0, 'Slept in after game night, midnight to 7:30am.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 60);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0369', 'day_2026-01-03', '2026-01-03T13:30:00Z', '2026-01-03T15:00:00Z', 'Slow morning', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Saturday morning, coffee and reading the news.', '["routine", "morning", "coffee", "browsing"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 65);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0370', 'day_2026-01-03', '2026-01-03T15:30:00Z', '2026-01-03T17:00:00Z', 'Walk at Lady Bird Lake', 'Lady Bird Lake', '["steps", "location_visit"]', 0, 0, 0, 0, 0, 'Saturday walk at Lady Bird Lake, cool January morning.', '["exercise", "outdoors"]', '["place_demo_ladybird"]', NULL, NULL, NULL, 'NEW', 92);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0371', 'day_2026-01-03', '2026-01-03T17:30:00Z', '2026-01-03T19:30:00Z', 'Errands', NULL, '["location_visit"]', 0, 0, 0, 0, 0, 'Grocery shopping and picking up a few things for the new year.', '["leisure", "errands"]', '[]', NULL, NULL, NULL, 'NEW', 72);

-- Mom call (Saturday)
INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0372', 'day_2026-01-03', '2026-01-03T22:00:00Z', '2026-01-03T22:45:00Z', 'Phone call with Mom', 'Home', '["transcription"]', 0, 0, 0, 0, 0, 'Weekly call with Mom, she asked about New Year''s resolutions.', '["family", "phone-call"]', '["person_demo_mom", "place_demo_home"]', NULL, NULL, NULL, 'NEW', 68);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0373', 'day_2026-01-03', '2026-01-03T23:00:00Z', '2026-01-04T02:00:00Z', 'Evening at home', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Made a stir-fry for dinner and started a new book.', '["food", "leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 61);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0374', 'day_2026-01-03', '2026-01-04T02:00:00Z', '2026-01-04T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Watched a show and fell asleep on the couch.', '["leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 58);

-- =============================================================================
-- SUNDAY January 4, 2026 — Slow day, prep for back to normal
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0375', 'day_2026-01-04', '2026-01-04T06:00:00Z', '2026-01-04T14:00:00Z', 'Sleep', 'Home', '["sleep"]', 0, 0, 1, 0, 0, 'Sleep from midnight to 8am.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 62);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0376', 'day_2026-01-04', '2026-01-04T14:00:00Z', '2026-01-04T15:30:00Z', 'Slow morning', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Lazy Sunday morning, coffee and reading.', '["routine", "morning", "coffee", "leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 63);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0377', 'day_2026-01-04', '2026-01-04T16:00:00Z', '2026-01-04T17:00:00Z', 'Run', 'Mueller Trails', '["steps", "workout"]', 0, 0, 0, 0, 0, 'Sunday morning run on Mueller trails, 3 miles to start the year right.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]', NULL, NULL, NULL, 'NEW', 157);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0378', 'day_2026-01-04', '2026-01-04T17:30:00Z', '2026-01-04T20:00:00Z', 'Cooking and meal prep', 'Home', '["location_visit"]', 0, 0, 0, 0, 0, 'Big Sunday meal prep — made soup, roasted vegetables, and prepped lunches.', '["food", "cooking"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 70);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0379', 'day_2026-01-04', '2026-01-04T20:00:00Z', '2026-01-05T00:00:00Z', 'Afternoon at home', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Read for a while and organized her desk for the week ahead.', '["leisure", "reflection"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 60);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0380', 'day_2026-01-04', '2026-01-05T00:00:00Z', '2026-01-05T03:00:00Z', 'Evening', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Made pasta for dinner and watched a movie, early night to get back on schedule.', '["food", "leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 67);

INSERT OR IGNORE INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0381', 'day_2026-01-04', '2026-01-05T03:00:00Z', '2026-01-05T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', 0, 0, 0, 0, 0, 'Set alarms for Monday, read a few pages, and fell asleep.', '["leisure", "reflection"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 62);
