-- =============================================================================
-- Baseline Seed: Weeks 7-9 — January 5 through January 25, 2026
-- =============================================================================
--
-- Character: UX designer, early 30s, lives in Mueller (East Austin), works
--            downtown at Canopy (B2B SaaS). See seed_baseline_guide.md.
--
-- Key narrative beats:
--   - Jan 8 (Thu):  Rachel Torres contacts her about house hunting (first appearance)
--   - ~Jan 12+:     Onboarding redesign project starts ramping up at work
--   - Jan 25 (Sun): Second Rachel appearance — house showing (not the S 3rd house)
--   - Game nights at Jess's: Jan 9 and Jan 16 (skip Jan 23)
--   - Mom calls weekly: Jan 10, Jan 17, Jan 24
--
-- Event IDs: ev_b0421 through ev_b0630
-- All times UTC. January 2026 is CST = UTC-6.
-- CST midnight = 06:00 UTC, CST 06:30 = 12:30 UTC, etc.
--
-- Usage: sqlite3 core/data/virtues.db < core/seed_baseline_w07_09.sql
-- =============================================================================

-- ─────────────────────────────────────────────────────────────────────────────
-- CLEANUP
-- ─────────────────────────────────────────────────────────────────────────────
DELETE FROM wiki_events WHERE id LIKE 'ev_b0%' AND CAST(SUBSTR(id, 5) AS INTEGER) BETWEEN 421 AND 630;

-- ─────────────────────────────────────────────────────────────────────────────
-- WIKI DAYS
-- ─────────────────────────────────────────────────────────────────────────────
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2026-01-05', '2026-01-05', 'America/Chicago', 'America/Chicago', 0.52);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2026-01-06', '2026-01-06', 'America/Chicago', 'America/Chicago', 0.48);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2026-01-07', '2026-01-07', 'America/Chicago', 'America/Chicago', 0.50);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2026-01-08', '2026-01-08', 'America/Chicago', 'America/Chicago', 0.45);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2026-01-09', '2026-01-09', 'America/Chicago', 'America/Chicago', 0.53);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2026-01-10', '2026-01-10', 'America/Chicago', 'America/Chicago', 0.55);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2026-01-11', '2026-01-11', 'America/Chicago', 'America/Chicago', 0.47);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2026-01-12', '2026-01-12', 'America/Chicago', 'America/Chicago', 0.50);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2026-01-13', '2026-01-13', 'America/Chicago', 'America/Chicago', 0.51);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2026-01-14', '2026-01-14', 'America/Chicago', 'America/Chicago', 0.49);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2026-01-15', '2026-01-15', 'America/Chicago', 'America/Chicago', 0.54);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2026-01-16', '2026-01-16', 'America/Chicago', 'America/Chicago', 0.46);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2026-01-17', '2026-01-17', 'America/Chicago', 'America/Chicago', 0.52);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2026-01-18', '2026-01-18', 'America/Chicago', 'America/Chicago', 0.58);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2026-01-19', '2026-01-19', 'America/Chicago', 'America/Chicago', 0.44);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2026-01-20', '2026-01-20', 'America/Chicago', 'America/Chicago', 0.50);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2026-01-21', '2026-01-21', 'America/Chicago', 'America/Chicago', 0.48);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2026-01-22', '2026-01-22', 'America/Chicago', 'America/Chicago', 0.53);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2026-01-23', '2026-01-23', 'America/Chicago', 'America/Chicago', 0.42);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2026-01-24', '2026-01-24', 'America/Chicago', 'America/Chicago', 0.56);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2026-01-25', '2026-01-25', 'America/Chicago', 'America/Chicago', 0.49);

-- ─────────────────────────────────────────────────────────────────────────────
-- WIKI EVENTS
-- ─────────────────────────────────────────────────────────────────────────────

-- =============================================================================
-- WEEK 7: January 5 (Mon) - January 11 (Sun)
-- =============================================================================

-- ── Monday, January 5, 2026 ─────────────────────────────────────────────────

-- Sleep (00:00-06:30 CST = 06:00-12:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0421', 'day_2026-01-05',
    '2026-01-05T06:00:00Z', '2026-01-05T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Sleep from midnight to 6:30am, about 6.5 hours.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 56
);

-- Morning routine (06:30-07:15 CST = 12:30-13:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0422', 'day_2026-01-05',
    '2026-01-05T12:30:00Z', '2026-01-05T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Coffee, checked Slack and email to catch up after the weekend.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 67
);

-- Bike commute (07:15-07:45 CST = 13:15-13:45 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0423', 'day_2026-01-05',
    '2026-01-05T13:15:00Z', '2026-01-05T13:45:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    0, 1, 0, 0, 0,
    'Bike commute to the office, cold morning but sunny.', '["commute", "cycling", "podcast"]', '[]',
    NULL, NULL, NULL, 'NEW', 128
);

-- Coffee and Slack (07:45-08:15 CST = 13:45-14:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0424', 'day_2026-01-05',
    '2026-01-05T13:45:00Z', '2026-01-05T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    0, 0, 0, 0, 0,
    'Grabbed coffee at the office and caught up on Slack threads from the holiday break.', '["messaging", "work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 72
);

-- Standup (08:15-09:00 CST = 14:15-15:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0425', 'day_2026-01-05',
    '2026-01-05T14:15:00Z', '2026-01-05T15:00:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    0, 0, 0, 0, 0,
    'First standup of the new year with Maya and David, reviewed Q1 priorities.', '["meeting", "standup", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 77
);

-- Focused design work (09:00-11:30 CST = 15:00-17:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0426', 'day_2026-01-05',
    '2026-01-05T15:00:00Z', '2026-01-05T17:30:00Z',
    'Focused design work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Long Figma session working on the settings page redesign.', '["design", "figma", "focus", "deep-work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 69
);

-- Lunch solo (11:30-12:15 CST = 17:30-18:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0427', 'day_2026-01-05',
    '2026-01-05T17:30:00Z', '2026-01-05T18:15:00Z',
    'Lunch', 'Office', '["location_visit"]',
    0, 0, 0, 0, 0,
    'Quick solo lunch at desk, leftover soup from the weekend.', '["food"]', '["place_demo_office"]',
    NULL, NULL, NULL, 'NEW', 66
);

-- Afternoon work block (12:15-16:30 CST = 18:15-22:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0428', 'day_2026-01-05',
    '2026-01-05T18:15:00Z', '2026-01-05T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Continued on settings page wireframes, responded to Slack threads about Q1 roadmap.', '["design", "figma", "work", "messaging"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 69
);

-- Bike commute home (16:30-17:00 CST = 22:30-23:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0429', 'day_2026-01-05',
    '2026-01-05T22:30:00Z', '2026-01-05T23:00:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    0, 1, 0, 0, 0,
    'Bike ride home from the office.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 130
);

-- Evening run (17:30-18:15 CST = 23:30-00:15+1 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0430', 'day_2026-01-05',
    '2026-01-05T23:30:00Z', '2026-01-06T00:15:00Z',
    'Evening run', 'Mueller Trails', '["steps", "workout"]',
    0, 0, 0, 0, 0,
    'Short 3-mile run on Mueller trails to shake off the Monday sluggishness.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
    NULL, NULL, NULL, 'NEW', 157
);

-- Dinner and reading (18:30-22:00 CST = 00:30-04:00+1 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0431', 'day_2026-01-05',
    '2026-01-06T00:30:00Z', '2026-01-06T04:00:00Z',
    'Dinner and reading', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Made stir fry for dinner then read on the couch for a couple hours.', '["food", "leisure"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 59
);

-- ── Tuesday, January 6, 2026 ────────────────────────────────────────────────

-- Sleep (00:00-06:15 CST = 06:00-12:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0432', 'day_2026-01-06',
    '2026-01-06T06:00:00Z', '2026-01-06T12:15:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Sleep from midnight to about 6:15am.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 58
);

-- Morning routine (06:15-07:10 CST = 12:15-13:10 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0433', 'day_2026-01-06',
    '2026-01-06T12:15:00Z', '2026-01-06T13:10:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Morning coffee, scrolled through texts, got ready for the day.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 64
);

-- Bike commute (07:10-07:40 CST = 13:10-13:40 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0434', 'day_2026-01-06',
    '2026-01-06T13:10:00Z', '2026-01-06T13:40:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    0, 1, 0, 0, 0,
    'Bike commute to office, chilly but clear.', '["commute", "cycling", "podcast"]', '[]',
    NULL, NULL, NULL, 'NEW', 118
);

-- Coffee and Slack (07:40-08:15 CST = 13:40-14:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0435', 'day_2026-01-06',
    '2026-01-06T13:40:00Z', '2026-01-06T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    0, 0, 0, 0, 0,
    'Coffee at the office, caught up on overnight Slack messages.', '["messaging", "work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 64
);

-- Standup + design review with David (08:15-09:30 CST = 14:15-15:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0436', 'day_2026-01-06',
    '2026-01-06T14:15:00Z', '2026-01-06T15:30:00Z',
    'Standup and design review', 'Office', '["calendar", "message", "transcription"]',
    0, 0, 0, 0, 0,
    'Standup followed by design review with David on the settings page component library.', '["meeting", "standup", "design", "design-review"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 77
);

-- Focused work (09:30-11:30 CST = 15:30-17:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0437', 'day_2026-01-06',
    '2026-01-06T15:30:00Z', '2026-01-06T17:30:00Z',
    'Focused design work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Deep work on settings page Figma prototypes, refining interaction flows.', '["design", "figma", "focus", "deep-work", "navigation"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 66
);

-- Lunch solo (11:30-12:15 CST = 17:30-18:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0438', 'day_2026-01-06',
    '2026-01-06T17:30:00Z', '2026-01-06T18:15:00Z',
    'Lunch', 'Office', '["location_visit"]',
    0, 0, 0, 0, 0,
    'Solo lunch at the office, sandwich from the deli downstairs.', '["food"]', '["place_demo_office"]',
    NULL, NULL, NULL, 'NEW', 70
);

-- Afternoon work (12:15-16:30 CST = 18:15-22:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0439', 'day_2026-01-06',
    '2026-01-06T18:15:00Z', '2026-01-06T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Worked on component specs and documentation for the settings redesign.', '["design", "figma", "work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 67
);

-- Bike commute home (16:30-17:00 CST = 22:30-23:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0440', 'day_2026-01-06',
    '2026-01-06T22:30:00Z', '2026-01-06T23:00:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    0, 1, 0, 0, 0,
    'Biked home from the office.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 130
);

-- Evening run on Mueller trails (17:30-18:20 CST = 23:30-00:20+1 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0441', 'day_2026-01-06',
    '2026-01-06T23:30:00Z', '2026-01-07T00:20:00Z',
    'Evening run', 'Mueller Trails', '["steps", "workout"]',
    0, 0, 0, 0, 0,
    'Tuesday evening run on Mueller trails, 3.5 miles.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
    NULL, NULL, NULL, 'NEW', 151
);

-- Dinner and TV (19:00-22:00 CST = 01:00-04:00+1 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0442', 'day_2026-01-06',
    '2026-01-07T01:00:00Z', '2026-01-07T04:00:00Z',
    'Dinner and TV', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Cooked pasta for dinner and watched a couple episodes of a documentary series.', '["food", "leisure"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 66
);

-- ── Wednesday, January 7, 2026 ──────────────────────────────────────────────

-- Sleep (00:00-06:30 CST = 06:00-12:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0443', 'day_2026-01-07',
    '2026-01-07T06:00:00Z', '2026-01-07T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Slept from midnight to 6:30am, 6.5 hours.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 59
);

-- Morning routine (06:30-07:15 CST = 12:30-13:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0444', 'day_2026-01-07',
    '2026-01-07T12:30:00Z', '2026-01-07T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Morning coffee and caught up on texts from Jess about Friday plans.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 63
);

-- Bike commute (07:15-07:45 CST = 13:15-13:45 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0445', 'day_2026-01-07',
    '2026-01-07T13:15:00Z', '2026-01-07T13:45:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    0, 1, 0, 0, 0,
    'Biked to office, overcast day.', '["commute", "cycling", "podcast"]', '[]',
    NULL, NULL, NULL, 'NEW', 116
);

-- Coffee and Slack (07:45-08:15 CST = 13:45-14:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0446', 'day_2026-01-07',
    '2026-01-07T13:45:00Z', '2026-01-07T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    0, 0, 0, 0, 0,
    'Office coffee and Slack catch-up before standup.', '["messaging", "work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 72
);

-- Standup (08:15-08:45 CST = 14:15-14:45 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0447', 'day_2026-01-07',
    '2026-01-07T14:15:00Z', '2026-01-07T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    0, 0, 0, 0, 0,
    'Quick standup with Maya and David, everyone aligned on settings page progress.', '["meeting", "standup", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 73
);

-- Focused work (08:45-11:30 CST = 14:45-17:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0448', 'day_2026-01-07',
    '2026-01-07T14:45:00Z', '2026-01-07T17:30:00Z',
    'Focused design work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Deep work in Figma on settings page interaction patterns.', '["design", "figma", "focus", "deep-work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 64
);

-- Lunch with Maya at Tatsu-ya (11:30-12:30 CST = 17:30-18:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0449', 'day_2026-01-07',
    '2026-01-07T17:30:00Z', '2026-01-07T18:30:00Z',
    'Lunch with Maya', 'Ramen Tatsu-ya', '["location_visit", "transcription"]',
    0, 0, 0, 0, 0,
    'Weekly lunch at Ramen Tatsu-ya with Maya, talked about team goals for Q1.', '["social", "food", "ramen"]', '["person_demo_maya", "place_demo_ramen"]',
    NULL, NULL, NULL, 'NEW', 75
);

-- Afternoon work (12:30-16:30 CST = 18:30-22:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0450', 'day_2026-01-07',
    '2026-01-07T18:30:00Z', '2026-01-07T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Continued on settings page, wrote up spec notes for David to implement.', '["design", "figma", "work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 66
);

-- Bike commute home (16:30-17:00 CST = 22:30-23:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0451', 'day_2026-01-07',
    '2026-01-07T22:30:00Z', '2026-01-07T23:00:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    0, 1, 0, 0, 0,
    'Biked home, stopped to pick up groceries on the way.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 112
);

-- Dinner and browsing (18:00-22:00 CST = 00:00-04:00+1 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0452', 'day_2026-01-07',
    '2026-01-08T00:00:00Z', '2026-01-08T04:00:00Z',
    'Dinner and browsing', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Made a salad for dinner, then browsed apartment listings and read online.', '["food", "leisure", "browsing", "house-hunting"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 68
);

-- ── Thursday, January 8, 2026 — RACHEL FIRST CONTACT ────────────────────────

-- Sleep (00:00-06:20 CST = 06:00-12:20 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0453', 'day_2026-01-08',
    '2026-01-08T06:00:00Z', '2026-01-08T12:20:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Slept from midnight to about 6:20am.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 62
);

-- Morning routine (06:20-07:10 CST = 12:20-13:10 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0454', 'day_2026-01-08',
    '2026-01-08T12:20:00Z', '2026-01-08T13:10:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Coffee and morning routine, checked email — saw a message from a realtor named Rachel Torres.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 68
);

-- Bike commute (07:10-07:40 CST = 13:10-13:40 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0455', 'day_2026-01-08',
    '2026-01-08T13:10:00Z', '2026-01-08T13:40:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    0, 1, 0, 0, 0,
    'Bike commute to the office.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 133
);

-- Coffee and Slack (07:40-08:15 CST = 13:40-14:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0456', 'day_2026-01-08',
    '2026-01-08T13:40:00Z', '2026-01-08T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    0, 0, 0, 0, 0,
    'Coffee at the office, caught up on Slack.', '["messaging", "work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 69
);

-- Standup (08:15-08:45 CST = 14:15-14:45 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0457', 'day_2026-01-08',
    '2026-01-08T14:15:00Z', '2026-01-08T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    0, 0, 0, 0, 0,
    'Standup with Maya and David, discussed design review feedback from Tuesday.', '["meeting", "standup", "design", "design-review"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 72
);

-- Focused work (08:45-11:30 CST = 14:45-17:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0458', 'day_2026-01-08',
    '2026-01-08T14:45:00Z', '2026-01-08T17:30:00Z',
    'Focused design work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Worked on settings page prototypes in Figma.', '["design", "figma", "focus", "deep-work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 69
);

-- Lunch solo (11:30-12:15 CST = 17:30-18:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0459', 'day_2026-01-08',
    '2026-01-08T17:30:00Z', '2026-01-08T18:15:00Z',
    'Lunch', 'Office', '["location_visit"]',
    0, 0, 0, 0, 0,
    'Ate lunch at the office, packed leftovers.', '["food"]', '["place_demo_office"]',
    NULL, NULL, NULL, 'NEW', 71
);

-- Afternoon work (12:15-16:00 CST = 18:15-22:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0460', 'day_2026-01-08',
    '2026-01-08T18:15:00Z', '2026-01-08T22:00:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Wrapped up a Figma prototype and shared it in the design channel for async feedback.', '["design", "figma", "work", "messaging"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 64
);

-- Bike commute home (16:00-16:30 CST = 22:00-22:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0461', 'day_2026-01-08',
    '2026-01-08T22:00:00Z', '2026-01-08T22:30:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    0, 1, 0, 0, 0,
    'Biked home from the office, left a bit early.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 133
);

-- ** RACHEL FIRST CONTACT ** Phone call (17:00-17:20 CST = 23:00-23:20 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0462', 'day_2026-01-08',
    '2026-01-08T23:00:00Z', '2026-01-08T23:20:00Z',
    'Phone call with Rachel Torres', 'Home', '["message", "transcription"]',
    0, 0, 0, 0, 0,
    'Rachel Torres from Torres Realty called about house hunting — she has some listings in East Austin and Bouldin Creek she thinks would be a good fit.', '["phone-call", "house-hunting", "real-estate"]', '["person_demo_rachel", "org_demo_realty", "place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 65
);

-- Walk on Mueller trails (17:30-18:15 CST = 23:30-00:15+1 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0463', 'day_2026-01-08',
    '2026-01-08T23:30:00Z', '2026-01-09T00:15:00Z',
    'Walk', 'Mueller Trails', '["steps"]',
    0, 0, 0, 0, 0,
    'Went for an evening walk on Mueller trails, thinking about whether to seriously start house hunting.', '["exercise", "outdoors", "mueller-trails", "house-hunting"]', '["place_demo_mueller_trails"]',
    NULL, NULL, NULL, 'NEW', 148
);

-- Dinner and evening (19:00-22:00 CST = 01:00-04:00+1 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0464', 'day_2026-01-08',
    '2026-01-09T01:00:00Z', '2026-01-09T04:00:00Z',
    'Dinner and browsing', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Made tacos for dinner, then browsed Zillow looking at East Austin listings Rachel mentioned.', '["food", "leisure", "browsing", "house-hunting", "real-estate"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 61
);

-- ── Friday, January 9, 2026 — Game night at Jess's ─────────────────────────

-- Sleep (00:00-06:30 CST = 06:00-12:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0465', 'day_2026-01-09',
    '2026-01-09T06:00:00Z', '2026-01-09T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Slept from midnight to 6:30am.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 58
);

-- Morning routine (06:30-07:15 CST = 12:30-13:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0466', 'day_2026-01-09',
    '2026-01-09T12:30:00Z', '2026-01-09T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Coffee and morning routine, texted Jess to confirm game night tonight.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 66
);

-- Bike commute (07:15-07:45 CST = 13:15-13:45 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0467', 'day_2026-01-09',
    '2026-01-09T13:15:00Z', '2026-01-09T13:45:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    0, 1, 0, 0, 0,
    'Biked to the office on a crisp Friday morning.', '["commute", "cycling", "podcast"]', '[]',
    NULL, NULL, NULL, 'NEW', 135
);

-- Coffee and Slack (07:45-08:15 CST = 13:45-14:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0468', 'day_2026-01-09',
    '2026-01-09T13:45:00Z', '2026-01-09T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    0, 0, 0, 0, 0,
    'Coffee and Slack at the office.', '["messaging", "work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 68
);

-- Standup (08:15-08:45 CST = 14:15-14:45 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0469', 'day_2026-01-09',
    '2026-01-09T14:15:00Z', '2026-01-09T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    0, 0, 0, 0, 0,
    'Friday standup — reviewed the week and Maya mentioned the onboarding funnel might become a priority soon.', '["meeting", "standup", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 78
);

-- Focused work (08:45-11:30 CST = 14:45-17:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0470', 'day_2026-01-09',
    '2026-01-09T14:45:00Z', '2026-01-09T17:30:00Z',
    'Focused design work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Wrapped up the settings page first draft, tidied up layers in Figma.', '["design", "figma", "focus"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 67
);

-- Lunch solo (11:30-12:15 CST = 17:30-18:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0471', 'day_2026-01-09',
    '2026-01-09T17:30:00Z', '2026-01-09T18:15:00Z',
    'Lunch', 'Office', '["location_visit"]',
    0, 0, 0, 0, 0,
    'Quick lunch at the office before heading out for a shorter Friday afternoon.', '["food"]', '["place_demo_office"]',
    NULL, NULL, NULL, 'NEW', 64
);

-- Afternoon work — shorter Friday (12:15-15:30 CST = 18:15-21:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0472', 'day_2026-01-09',
    '2026-01-09T18:15:00Z', '2026-01-09T21:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Light Friday afternoon — cleaned up design files and responded to a few PRs.', '["work", "design", "figma", "code-review"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 66
);

-- Bike commute home (15:30-16:00 CST = 21:30-22:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0473', 'day_2026-01-09',
    '2026-01-09T21:30:00Z', '2026-01-09T22:00:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    0, 1, 0, 0, 0,
    'Biked home early on Friday.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 124
);

-- Mom call (17:00-17:40 CST = 23:00-23:40 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0474', 'day_2026-01-09',
    '2026-01-09T23:00:00Z', '2026-01-09T23:40:00Z',
    'Phone call with Mom', 'Home', '["message", "transcription"]',
    0, 0, 0, 0, 0,
    'Weekly call with Mom, caught up on her week and mentioned the realtor who reached out.', '["family", "phone-call"]', '["person_demo_mom", "place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 66
);

-- Game night at Jess's (19:00-23:00 CST = 01:00-05:00+1 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0475', 'day_2026-01-09',
    '2026-01-10T01:00:00Z', '2026-01-10T05:00:00Z',
    'Game night', 'Jess''s Place', '["location_visit", "transcription"]',
    0, 0, 0, 0, 0,
    'Game night at Jess''s with Priya — played Catan and Ticket to Ride, ordered pizza.', '["social", "games", "food"]', '["person_demo_jess", "person_demo_priya", "place_demo_jess"]',
    NULL, NULL, NULL, 'NEW', 74
);

-- ── Saturday, January 10, 2026 ──────────────────────────────────────────────

-- Sleep (01:00-08:00 CST = 07:00-14:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0476', 'day_2026-01-10',
    '2026-01-10T07:00:00Z', '2026-01-10T14:00:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Slept in after game night, about 7 hours.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 57
);

-- Slow morning (08:00-09:30 CST = 14:00-15:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0477', 'day_2026-01-10',
    '2026-01-10T14:00:00Z', '2026-01-10T15:30:00Z',
    'Slow morning', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Lazy Saturday morning, coffee on the couch, scrolled through Instagram.', '["routine", "morning", "coffee", "browsing"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 64
);

-- Lady Bird Lake walk (10:00-11:30 CST = 16:00-17:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0478', 'day_2026-01-10',
    '2026-01-10T16:00:00Z', '2026-01-10T17:30:00Z',
    'Lady Bird Lake walk', 'Lady Bird Lake', '["steps", "location_visit"]',
    0, 0, 0, 0, 0,
    'Walked the boardwalk loop at Lady Bird Lake, cool but pleasant morning.', '["exercise", "outdoors", "walking"]', '["place_demo_ladybird"]',
    NULL, NULL, NULL, 'NEW', 92
);

-- Errands and lunch (11:30-13:30 CST = 17:30-19:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0479', 'day_2026-01-10',
    '2026-01-10T17:30:00Z', '2026-01-10T19:30:00Z',
    'Errands and lunch', NULL, '["location_visit"]',
    0, 0, 0, 0, 0,
    'Ran errands at HEB, grabbed a taco from a food truck on the way home.', '["food"]', '[]',
    NULL, NULL, NULL, 'NEW', 73
);

-- Afternoon reading (14:00-17:00 CST = 20:00-23:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0480', 'day_2026-01-10',
    '2026-01-10T20:00:00Z', '2026-01-10T23:00:00Z',
    'Reading', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Spent the afternoon reading and doing a bit of journaling.', '["leisure", "reflection"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 58
);

-- Dinner and movie (18:00-22:00 CST = 00:00-04:00+1 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0481', 'day_2026-01-10',
    '2026-01-11T00:00:00Z', '2026-01-11T04:00:00Z',
    'Dinner and movie', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Cooked a stir fry for dinner and watched a movie at home.', '["food", "leisure"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 68
);

-- ── Sunday, January 11, 2026 ────────────────────────────────────────────────

-- Sleep (00:00-08:00 CST = 06:00-14:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0482', 'day_2026-01-11',
    '2026-01-11T06:00:00Z', '2026-01-11T14:00:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Slept in on Sunday, about 8 hours.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 55
);

-- Slow morning (08:00-09:30 CST = 14:00-15:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0483', 'day_2026-01-11',
    '2026-01-11T14:00:00Z', '2026-01-11T15:30:00Z',
    'Slow morning', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Quiet Sunday morning with coffee and the NYT crossword.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 67
);

-- Mueller trails run (09:30-10:30 CST = 15:30-16:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0484', 'day_2026-01-11',
    '2026-01-11T15:30:00Z', '2026-01-11T16:30:00Z',
    'Morning run', 'Mueller Trails', '["steps", "workout"]',
    0, 0, 0, 0, 0,
    'Sunday morning run on Mueller trails, 4 miles.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
    NULL, NULL, NULL, 'NEW', 68
);

-- Cooking and meal prep (11:00-13:00 CST = 17:00-19:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0485', 'day_2026-01-11',
    '2026-01-11T17:00:00Z', '2026-01-11T19:00:00Z',
    'Cooking', 'Home', '["location_visit"]',
    0, 0, 0, 0, 0,
    'Meal prepped soup and grain bowls for the week ahead.', '["food"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 72
);

-- Afternoon reading and browsing (13:00-17:00 CST = 19:00-23:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0486', 'day_2026-01-11',
    '2026-01-11T19:00:00Z', '2026-01-11T23:00:00Z',
    'Reading and browsing', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Read a design book for a while, then browsed some articles about onboarding UX patterns.', '["leisure", "browsing", "onboarding"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 62
);

-- Dinner and wind down (18:00-22:00 CST = 00:00-04:00+1 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0487', 'day_2026-01-11',
    '2026-01-12T00:00:00Z', '2026-01-12T04:00:00Z',
    'Dinner and wind down', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Simple dinner from the meal prep, then watched TV and got ready for the week.', '["food", "leisure"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 68
);

-- =============================================================================
-- WEEK 8: January 12 (Mon) - January 18 (Sun) — Onboarding project ramps up
-- =============================================================================

-- ── Monday, January 12, 2026 ────────────────────────────────────────────────

-- Sleep (00:00-06:30 CST = 06:00-12:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0488', 'day_2026-01-12',
    '2026-01-12T06:00:00Z', '2026-01-12T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Sleep from midnight to 6:30am.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 62
);

-- Morning routine (06:30-07:15 CST = 12:30-13:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0489', 'day_2026-01-12',
    '2026-01-12T12:30:00Z', '2026-01-12T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Coffee and morning routine, saw a Slack message from Maya about the onboarding project kickoff this week.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 67
);

-- Bike commute (07:15-07:45 CST = 13:15-13:45 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0490', 'day_2026-01-12',
    '2026-01-12T13:15:00Z', '2026-01-12T13:45:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    0, 1, 0, 0, 0,
    'Biked to the office.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 120
);

-- Coffee and Slack (07:45-08:15 CST = 13:45-14:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0491', 'day_2026-01-12',
    '2026-01-12T13:45:00Z', '2026-01-12T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    0, 0, 0, 0, 0,
    'Coffee and Slack, read the brief for the onboarding funnel redesign project.', '["messaging", "work", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 71
);

-- Standup + onboarding kickoff (08:15-09:30 CST = 14:15-15:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0492', 'day_2026-01-12',
    '2026-01-12T14:15:00Z', '2026-01-12T15:30:00Z',
    'Standup and onboarding kickoff', 'Office', '["calendar", "message", "transcription"]',
    0, 0, 0, 0, 0,
    'Standup followed by onboarding redesign kickoff meeting with Maya and David — reviewed funnel metrics and drop-off points.', '["meeting", "standup", "design", "onboarding"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 71
);

-- Focused work on onboarding audit (09:30-11:30 CST = 15:30-17:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0493', 'day_2026-01-12',
    '2026-01-12T15:30:00Z', '2026-01-12T17:30:00Z',
    'Onboarding audit', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Started auditing the current onboarding flow in Figma, mapping out every screen and drop-off point.', '["design", "figma", "focus", "deep-work", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 64
);

-- Lunch solo (11:30-12:15 CST = 17:30-18:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0494', 'day_2026-01-12',
    '2026-01-12T17:30:00Z', '2026-01-12T18:15:00Z',
    'Lunch', 'Office', '["location_visit"]',
    0, 0, 0, 0, 0,
    'Ate the grain bowl from Sunday meal prep at my desk.', '["food"]', '["place_demo_office"]',
    NULL, NULL, NULL, 'NEW', 70
);

-- Afternoon work (12:15-16:30 CST = 18:15-22:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0495', 'day_2026-01-12',
    '2026-01-12T18:15:00Z', '2026-01-12T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Continued the onboarding audit and started collecting competitor screenshots.', '["design", "figma", "work", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 72
);

-- Bike commute home (16:30-17:00 CST = 22:30-23:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0496', 'day_2026-01-12',
    '2026-01-12T22:30:00Z', '2026-01-12T23:00:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    0, 1, 0, 0, 0,
    'Biked home from the office.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 112
);

-- Evening — dinner and reading (18:00-22:00 CST = 00:00-04:00+1 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0497', 'day_2026-01-12',
    '2026-01-13T00:00:00Z', '2026-01-13T04:00:00Z',
    'Dinner and reading', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Made soup from the meal prep batch, spent the evening reading.', '["food", "leisure"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 64
);

-- ── Tuesday, January 13, 2026 ───────────────────────────────────────────────

-- Sleep (00:00-06:20 CST = 06:00-12:20 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0498', 'day_2026-01-13',
    '2026-01-13T06:00:00Z', '2026-01-13T12:20:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Slept from midnight to 6:20am.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 56
);

-- Morning routine (06:20-07:10 CST = 12:20-13:10 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0499', 'day_2026-01-13',
    '2026-01-13T12:20:00Z', '2026-01-13T13:10:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Coffee, quick check of email and Slack.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 68
);

-- Bike commute (07:10-07:40 CST = 13:10-13:40 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0500', 'day_2026-01-13',
    '2026-01-13T13:10:00Z', '2026-01-13T13:40:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    0, 1, 0, 0, 0,
    'Biked to the office.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 111
);

-- Coffee and Slack (07:40-08:15 CST = 13:40-14:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0501', 'day_2026-01-13',
    '2026-01-13T13:40:00Z', '2026-01-13T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    0, 0, 0, 0, 0,
    'Coffee and Slack catch-up.', '["messaging", "work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 68
);

-- Standup + design review (08:15-09:30 CST = 14:15-15:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0502', 'day_2026-01-13',
    '2026-01-13T14:15:00Z', '2026-01-13T15:30:00Z',
    'Standup and design review', 'Office', '["calendar", "message", "transcription"]',
    0, 0, 0, 0, 0,
    'Standup then design review with David on the onboarding audit findings so far.', '["meeting", "standup", "design", "design-review", "onboarding"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 75
);

-- Focused work (09:30-11:30 CST = 15:30-17:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0503', 'day_2026-01-13',
    '2026-01-13T15:30:00Z', '2026-01-13T17:30:00Z',
    'Focused design work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Deep work mapping the onboarding user journey and identifying friction points.', '["design", "figma", "focus", "deep-work", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 70
);

-- Lunch solo (11:30-12:15 CST = 17:30-18:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0504', 'day_2026-01-13',
    '2026-01-13T17:30:00Z', '2026-01-13T18:15:00Z',
    'Lunch', 'Office', '["location_visit"]',
    0, 0, 0, 0, 0,
    'Lunch at the office, leftover grain bowl.', '["food"]', '["place_demo_office"]',
    NULL, NULL, NULL, 'NEW', 68
);

-- Afternoon work (12:15-16:30 CST = 18:15-22:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0505', 'day_2026-01-13',
    '2026-01-13T18:15:00Z', '2026-01-13T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Worked on wireframe sketches for the new onboarding flow and shared them in the design channel.', '["design", "figma", "work", "onboarding", "messaging"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 71
);

-- Bike commute home (16:30-17:00 CST = 22:30-23:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0506', 'day_2026-01-13',
    '2026-01-13T22:30:00Z', '2026-01-13T23:00:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    0, 1, 0, 0, 0,
    'Biked home.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 111
);

-- Evening run (17:30-18:20 CST = 23:30-00:20+1 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0507', 'day_2026-01-13',
    '2026-01-13T23:30:00Z', '2026-01-14T00:20:00Z',
    'Evening run', 'Mueller Trails', '["steps", "workout"]',
    0, 0, 0, 0, 0,
    'Tuesday evening run on Mueller trails, 3 miles.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
    NULL, NULL, NULL, 'NEW', 154
);

-- Dinner and TV (19:00-22:00 CST = 01:00-04:00+1 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0508', 'day_2026-01-13',
    '2026-01-14T01:00:00Z', '2026-01-14T04:00:00Z',
    'Dinner and TV', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Quick dinner then watched a couple episodes of a series.', '["food", "leisure"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 72
);

-- ── Wednesday, January 14, 2026 ─────────────────────────────────────────────

-- Sleep (00:00-06:30 CST = 06:00-12:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0509', 'day_2026-01-14',
    '2026-01-14T06:00:00Z', '2026-01-14T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Slept midnight to 6:30am, about 6.5 hours.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 55
);

-- Morning routine (06:30-07:15 CST = 12:30-13:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0510', 'day_2026-01-14',
    '2026-01-14T12:30:00Z', '2026-01-14T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Morning coffee and Slack check.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 63
);

-- Bike commute (07:15-07:45 CST = 13:15-13:45 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0511', 'day_2026-01-14',
    '2026-01-14T13:15:00Z', '2026-01-14T13:45:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    0, 1, 0, 0, 0,
    'Biked to the office, warmer than usual for January.', '["commute", "cycling", "podcast"]', '[]',
    NULL, NULL, NULL, 'NEW', 124
);

-- Coffee and Slack (07:45-08:15 CST = 13:45-14:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0512', 'day_2026-01-14',
    '2026-01-14T13:45:00Z', '2026-01-14T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    0, 0, 0, 0, 0,
    'Coffee and caught up on Slack, David had feedback on the onboarding wireframes.', '["messaging", "work", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 65
);

-- Standup (08:15-08:45 CST = 14:15-14:45 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0513', 'day_2026-01-14',
    '2026-01-14T14:15:00Z', '2026-01-14T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    0, 0, 0, 0, 0,
    'Standup — focused on onboarding redesign progress and next steps.', '["meeting", "standup", "design", "onboarding"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 71
);

-- Focused work (08:45-11:30 CST = 14:45-17:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0514', 'day_2026-01-14',
    '2026-01-14T14:45:00Z', '2026-01-14T17:30:00Z',
    'Focused design work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Iterated on onboarding wireframes in Figma based on David''s feedback.', '["design", "figma", "focus", "deep-work", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 66
);

-- Lunch with Maya at Tatsu-ya (11:30-12:30 CST = 17:30-18:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0515', 'day_2026-01-14',
    '2026-01-14T17:30:00Z', '2026-01-14T18:30:00Z',
    'Lunch with Maya', 'Ramen Tatsu-ya', '["location_visit", "transcription"]',
    0, 0, 0, 0, 0,
    'Lunch at Ramen Tatsu-ya with Maya, talked about the onboarding project scope and user research plans.', '["social", "food", "ramen", "onboarding"]', '["person_demo_maya", "place_demo_ramen"]',
    NULL, NULL, NULL, 'NEW', 71
);

-- Afternoon work (12:30-16:30 CST = 18:30-22:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0516', 'day_2026-01-14',
    '2026-01-14T18:30:00Z', '2026-01-14T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Continued refining onboarding wireframes, started a user research plan doc.', '["design", "figma", "work", "onboarding", "research"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 65
);

-- Bike commute home (16:30-17:00 CST = 22:30-23:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0517', 'day_2026-01-14',
    '2026-01-14T22:30:00Z', '2026-01-14T23:00:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    0, 1, 0, 0, 0,
    'Biked home from the office.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 127
);

-- Evening walk and dinner (17:30-22:00 CST = 23:30-04:00+1 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0518', 'day_2026-01-14',
    '2026-01-14T23:30:00Z', '2026-01-15T04:00:00Z',
    'Evening walk and dinner', 'Home', '["steps", "app_usage"]',
    0, 0, 0, 0, 0,
    'Short walk around the neighborhood, then made pasta and read before bed.', '["exercise", "outdoors", "walking", "food", "leisure"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 67
);

-- ── Thursday, January 15, 2026 — WFH afternoon ─────────────────────────────

-- Sleep (00:00-06:15 CST = 06:00-12:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0519', 'day_2026-01-15',
    '2026-01-15T06:00:00Z', '2026-01-15T12:15:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Slept from midnight to about 6:15am.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 60
);

-- Morning routine (06:15-07:10 CST = 12:15-13:10 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0520', 'day_2026-01-15',
    '2026-01-15T12:15:00Z', '2026-01-15T13:10:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Coffee, morning routine, checked messages.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 64
);

-- Bike commute (07:10-07:40 CST = 13:10-13:40 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0521', 'day_2026-01-15',
    '2026-01-15T13:10:00Z', '2026-01-15T13:40:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    0, 1, 0, 0, 0,
    'Biked to the office.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 123
);

-- Coffee and Slack (07:40-08:15 CST = 13:40-14:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0522', 'day_2026-01-15',
    '2026-01-15T13:40:00Z', '2026-01-15T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    0, 0, 0, 0, 0,
    'Coffee and Slack at the office.', '["messaging", "work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 70
);

-- Standup (08:15-08:45 CST = 14:15-14:45 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0523', 'day_2026-01-15',
    '2026-01-15T14:15:00Z', '2026-01-15T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    0, 0, 0, 0, 0,
    'Standup with Maya and David, shared progress on onboarding wireframes.', '["meeting", "standup", "design", "onboarding"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 72
);

-- Focused work (08:45-11:30 CST = 14:45-17:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0524', 'day_2026-01-15',
    '2026-01-15T14:45:00Z', '2026-01-15T17:30:00Z',
    'Focused design work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Worked on high-fidelity Figma screens for the first two steps of the new onboarding flow.', '["design", "figma", "focus", "deep-work", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 69
);

-- Lunch solo (11:30-12:15 CST = 17:30-18:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0525', 'day_2026-01-15',
    '2026-01-15T17:30:00Z', '2026-01-15T18:15:00Z',
    'Lunch', 'Office', '["location_visit"]',
    0, 0, 0, 0, 0,
    'Ate lunch at the office, salad from the place down the block.', '["food"]', '["place_demo_office"]',
    NULL, NULL, NULL, 'NEW', 72
);

-- Bike commute home early for WFH (12:30-13:00 CST = 18:30-19:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0526', 'day_2026-01-15',
    '2026-01-15T18:30:00Z', '2026-01-15T19:00:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    0, 1, 0, 0, 0,
    'Headed home early to WFH for the afternoon.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 111
);

-- WFH afternoon work (13:30-16:30 CST = 19:30-22:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0527', 'day_2026-01-15',
    '2026-01-15T19:30:00Z', '2026-01-15T22:30:00Z',
    'WFH afternoon', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Worked from home on the onboarding research plan and drafted interview questions.', '["work", "design", "onboarding", "research", "remote"]', '["place_demo_home", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 72
);

-- Mueller trails walk (17:00-17:45 CST = 23:00-23:45 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0528', 'day_2026-01-15',
    '2026-01-15T23:00:00Z', '2026-01-15T23:45:00Z',
    'Walk', 'Mueller Trails', '["steps"]',
    0, 0, 0, 0, 0,
    'Afternoon walk on Mueller trails to clear my head.', '["exercise", "outdoors", "walking", "mueller-trails"]', '["place_demo_mueller_trails"]',
    NULL, NULL, NULL, 'NEW', 149
);

-- Dinner and browsing (18:30-22:00 CST = 00:30-04:00+1 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0529', 'day_2026-01-15',
    '2026-01-16T00:30:00Z', '2026-01-16T04:00:00Z',
    'Dinner and browsing', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Made a quick dinner, then spent the evening browsing design inspiration for the onboarding project.', '["food", "browsing", "leisure", "onboarding"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 68
);

-- ── Friday, January 16, 2026 — Game night at Jess's ────────────────────────

-- Sleep (00:00-06:30 CST = 06:00-12:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0530', 'day_2026-01-16',
    '2026-01-16T06:00:00Z', '2026-01-16T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Slept from midnight to 6:30am.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 56
);

-- Morning routine (06:30-07:15 CST = 12:30-13:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0531', 'day_2026-01-16',
    '2026-01-16T12:30:00Z', '2026-01-16T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Coffee and morning routine, confirmed game night plans with Jess.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 65
);

-- Bike commute (07:15-07:45 CST = 13:15-13:45 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0532', 'day_2026-01-16',
    '2026-01-16T13:15:00Z', '2026-01-16T13:45:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    0, 1, 0, 0, 0,
    'Biked to the office.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 124
);

-- Coffee and Slack (07:45-08:15 CST = 13:45-14:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0533', 'day_2026-01-16',
    '2026-01-16T13:45:00Z', '2026-01-16T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    0, 0, 0, 0, 0,
    'Coffee and Slack, reviewed feedback on onboarding wireframes from the team.', '["messaging", "work", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 64
);

-- Standup (08:15-08:45 CST = 14:15-14:45 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0534', 'day_2026-01-16',
    '2026-01-16T14:15:00Z', '2026-01-16T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    0, 0, 0, 0, 0,
    'Friday standup, wrapped up the week on onboarding progress.', '["meeting", "standup", "design", "onboarding"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 72
);

-- Focused work (08:45-11:30 CST = 14:45-17:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0535', 'day_2026-01-16',
    '2026-01-16T14:45:00Z', '2026-01-16T17:30:00Z',
    'Focused design work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Polished the onboarding prototype for the step-1 and step-2 screens.', '["design", "figma", "focus", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 69
);

-- Lunch (11:30-12:15 CST = 17:30-18:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0536', 'day_2026-01-16',
    '2026-01-16T17:30:00Z', '2026-01-16T18:15:00Z',
    'Lunch', 'Office', '["location_visit"]',
    0, 0, 0, 0, 0,
    'Quick lunch at the office.', '["food"]', '["place_demo_office"]',
    NULL, NULL, NULL, 'NEW', 68
);

-- Afternoon work — shorter Friday (12:15-15:30 CST = 18:15-21:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0537', 'day_2026-01-16',
    '2026-01-16T18:15:00Z', '2026-01-16T21:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Wrapped up loose ends for the week, cleaned up Figma files.', '["work", "design", "figma"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 67
);

-- Bike commute home (15:30-16:00 CST = 21:30-22:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0538', 'day_2026-01-16',
    '2026-01-16T21:30:00Z', '2026-01-16T22:00:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    0, 1, 0, 0, 0,
    'Biked home early for Friday.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 121
);

-- Game night at Jess's (19:00-23:00 CST = 01:00-05:00+1 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0539', 'day_2026-01-16',
    '2026-01-17T01:00:00Z', '2026-01-17T05:00:00Z',
    'Game night', 'Jess''s Place', '["location_visit", "transcription"]',
    0, 0, 0, 0, 0,
    'Game night at Jess''s place with Priya — played Catan and a new card game Priya brought.', '["social", "games", "food"]', '["person_demo_jess", "person_demo_priya", "place_demo_jess"]',
    NULL, NULL, NULL, 'NEW', 68
);

-- ── Saturday, January 17, 2026 — Mom call ───────────────────────────────────

-- Sleep (01:00-08:30 CST = 07:00-14:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0540', 'day_2026-01-17',
    '2026-01-17T07:00:00Z', '2026-01-17T14:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Slept in after game night, about 7.5 hours.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 62
);

-- Slow morning (08:30-10:00 CST = 14:30-16:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0541', 'day_2026-01-17',
    '2026-01-17T14:30:00Z', '2026-01-17T16:00:00Z',
    'Slow morning', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Lazy Saturday morning, coffee and scrolling through Instagram.', '["routine", "morning", "coffee", "browsing"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 67
);

-- Lady Bird Lake walk (10:00-11:30 CST = 16:00-17:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0542', 'day_2026-01-17',
    '2026-01-17T16:00:00Z', '2026-01-17T17:30:00Z',
    'Lady Bird Lake walk', 'Lady Bird Lake', '["steps", "location_visit"]',
    0, 0, 0, 0, 0,
    'Long walk along Lady Bird Lake, boardwalk section.', '["exercise", "outdoors", "walking"]', '["place_demo_ladybird"]',
    NULL, NULL, NULL, 'NEW', 85
);

-- Errands (11:30-13:00 CST = 17:30-19:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0543', 'day_2026-01-17',
    '2026-01-17T17:30:00Z', '2026-01-17T19:00:00Z',
    'Errands and lunch', NULL, '["location_visit"]',
    0, 0, 0, 0, 0,
    'Ran errands, stopped for a breakfast taco at a food truck.', '["food"]', '[]',
    NULL, NULL, NULL, 'NEW', 78
);

-- Mom call (14:00-14:45 CST = 20:00-20:45 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0544', 'day_2026-01-17',
    '2026-01-17T20:00:00Z', '2026-01-17T20:45:00Z',
    'Phone call with Mom', 'Home', '["message", "transcription"]',
    0, 0, 0, 0, 0,
    'Weekly call with Mom, she asked about work and whether I''m still thinking about buying a place.', '["family", "phone-call"]', '["person_demo_mom", "place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 71
);

-- Afternoon reading (15:00-17:30 CST = 21:00-23:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0545', 'day_2026-01-17',
    '2026-01-17T21:00:00Z', '2026-01-17T23:30:00Z',
    'Reading', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Spent the afternoon reading and journaling.', '["leisure", "reflection"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 65
);

-- Dinner and movie (18:00-22:00 CST = 00:00-04:00+1 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0546', 'day_2026-01-17',
    '2026-01-18T00:00:00Z', '2026-01-18T04:00:00Z',
    'Dinner and movie', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Cooked chicken and rice, watched a movie at home.', '["food", "leisure"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 70
);

-- ── Sunday, January 18, 2026 ────────────────────────────────────────────────

-- Sleep (00:00-08:00 CST = 06:00-14:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0547', 'day_2026-01-18',
    '2026-01-18T06:00:00Z', '2026-01-18T14:00:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Slept in on Sunday, about 8 hours.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 57
);

-- Slow morning (08:00-09:30 CST = 14:00-15:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0548', 'day_2026-01-18',
    '2026-01-18T14:00:00Z', '2026-01-18T15:30:00Z',
    'Slow morning', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Sunday morning, coffee and the crossword.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 65
);

-- Morning run (09:30-10:30 CST = 15:30-16:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0549', 'day_2026-01-18',
    '2026-01-18T15:30:00Z', '2026-01-18T16:30:00Z',
    'Morning run', 'Mueller Trails', '["steps", "workout"]',
    0, 0, 0, 0, 0,
    'Sunday run on Mueller trails, 4 miles at an easy pace.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
    NULL, NULL, NULL, 'NEW', 63
);

-- Jo's Coffee (11:00-12:00 CST = 17:00-18:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0550', 'day_2026-01-18',
    '2026-01-18T17:00:00Z', '2026-01-18T18:00:00Z',
    'Coffee', 'Jo''s Coffee', '["location_visit"]',
    0, 0, 0, 0, 0,
    'Stopped by Jo''s on South Congress for a latte and some reading.', '["coffee", "leisure"]', '["place_demo_jos"]',
    NULL, NULL, NULL, 'NEW', 68
);

-- Cooking and meal prep (12:30-14:30 CST = 18:30-20:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0551', 'day_2026-01-18',
    '2026-01-18T18:30:00Z', '2026-01-18T20:30:00Z',
    'Cooking', 'Home', '["location_visit"]',
    0, 0, 0, 0, 0,
    'Meal prepped chili and rice for the week.', '["food"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 75
);

-- Afternoon browsing (15:00-17:30 CST = 21:00-23:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0552', 'day_2026-01-18',
    '2026-01-18T21:00:00Z', '2026-01-18T23:30:00Z',
    'Browsing and reading', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Browsed onboarding UX articles and took notes for Monday.', '["browsing", "leisure", "onboarding"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 64
);

-- Dinner and wind down (18:00-22:00 CST = 00:00-04:00+1 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0553', 'day_2026-01-18',
    '2026-01-19T00:00:00Z', '2026-01-19T04:00:00Z',
    'Dinner and wind down', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Ate the chili, then TV and early to bed for the week ahead.', '["food", "leisure"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 70
);

-- =============================================================================
-- WEEK 9: January 19 (Mon) - January 25 (Sun) — Onboarding in full swing
-- =============================================================================

-- ── Monday, January 19, 2026 ────────────────────────────────────────────────

-- Sleep (00:00-06:30 CST = 06:00-12:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0554', 'day_2026-01-19',
    '2026-01-19T06:00:00Z', '2026-01-19T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Slept from midnight to 6:30am.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 58
);

-- Morning routine (06:30-07:15 CST = 12:30-13:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0555', 'day_2026-01-19',
    '2026-01-19T12:30:00Z', '2026-01-19T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Coffee and morning routine, reviewed onboarding notes from the weekend.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 63
);

-- Bike commute (07:15-07:45 CST = 13:15-13:45 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0556', 'day_2026-01-19',
    '2026-01-19T13:15:00Z', '2026-01-19T13:45:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    0, 1, 0, 0, 0,
    'Biked to office.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 127
);

-- Coffee and Slack (07:45-08:15 CST = 13:45-14:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0557', 'day_2026-01-19',
    '2026-01-19T13:45:00Z', '2026-01-19T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    0, 0, 0, 0, 0,
    'Coffee and Slack at the office, caught up on weekend messages.', '["messaging", "work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 69
);

-- Standup (08:15-09:00 CST = 14:15-15:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0558', 'day_2026-01-19',
    '2026-01-19T14:15:00Z', '2026-01-19T15:00:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    0, 0, 0, 0, 0,
    'Monday standup — discussed onboarding user research schedule for this week.', '["meeting", "standup", "design", "onboarding"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 77
);

-- User research session (09:00-10:30 CST = 15:00-16:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0559', 'day_2026-01-19',
    '2026-01-19T15:00:00Z', '2026-01-19T16:30:00Z',
    'User research session', 'Office', '["calendar", "transcription"]',
    0, 0, 0, 0, 0,
    'First onboarding user research session — interviewed a customer about their setup experience.', '["research", "onboarding", "design"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 64
);

-- Focused work (10:30-11:30 CST = 16:30-17:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0560', 'day_2026-01-19',
    '2026-01-19T16:30:00Z', '2026-01-19T17:30:00Z',
    'Research notes', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Synthesized notes from the user research session, tagged key themes.', '["research", "onboarding", "focus"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 69
);

-- Lunch solo (11:30-12:15 CST = 17:30-18:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0561', 'day_2026-01-19',
    '2026-01-19T17:30:00Z', '2026-01-19T18:15:00Z',
    'Lunch', 'Office', '["location_visit"]',
    0, 0, 0, 0, 0,
    'Ate the chili from meal prep at my desk.', '["food"]', '["place_demo_office"]',
    NULL, NULL, NULL, 'NEW', 68
);

-- Afternoon work (12:15-16:30 CST = 18:15-22:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0562', 'day_2026-01-19',
    '2026-01-19T18:15:00Z', '2026-01-19T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Worked on onboarding Figma prototypes, incorporating feedback from the research session.', '["design", "figma", "work", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 68
);

-- Bike commute home (16:30-17:00 CST = 22:30-23:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0563', 'day_2026-01-19',
    '2026-01-19T22:30:00Z', '2026-01-19T23:00:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    0, 1, 0, 0, 0,
    'Biked home.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 122
);

-- Evening run (17:30-18:15 CST = 23:30-00:15+1 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0564', 'day_2026-01-19',
    '2026-01-19T23:30:00Z', '2026-01-20T00:15:00Z',
    'Evening run', 'Mueller Trails', '["steps", "workout"]',
    0, 0, 0, 0, 0,
    'Quick 3-mile run on Mueller trails.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
    NULL, NULL, NULL, 'NEW', 154
);

-- Dinner and reading (19:00-22:00 CST = 01:00-04:00+1 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0565', 'day_2026-01-19',
    '2026-01-20T01:00:00Z', '2026-01-20T04:00:00Z',
    'Dinner and reading', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Dinner at home, then read for a couple hours before bed.', '["food", "leisure"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 59
);

-- ── Tuesday, January 20, 2026 ───────────────────────────────────────────────

-- Sleep (00:00-06:15 CST = 06:00-12:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0566', 'day_2026-01-20',
    '2026-01-20T06:00:00Z', '2026-01-20T12:15:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Slept midnight to about 6:15am.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 60
);

-- Morning routine (06:15-07:10 CST = 12:15-13:10 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0567', 'day_2026-01-20',
    '2026-01-20T12:15:00Z', '2026-01-20T13:10:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Coffee and morning routine.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 66
);

-- Bike commute (07:10-07:40 CST = 13:10-13:40 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0568', 'day_2026-01-20',
    '2026-01-20T13:10:00Z', '2026-01-20T13:40:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    0, 1, 0, 0, 0,
    'Biked to the office, foggy morning.', '["commute", "cycling", "podcast"]', '[]',
    NULL, NULL, NULL, 'NEW', 111
);

-- Coffee and Slack (07:40-08:15 CST = 13:40-14:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0569', 'day_2026-01-20',
    '2026-01-20T13:40:00Z', '2026-01-20T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    0, 0, 0, 0, 0,
    'Coffee and Slack at the office.', '["messaging", "work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 70
);

-- Standup + design review (08:15-09:30 CST = 14:15-15:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0570', 'day_2026-01-20',
    '2026-01-20T14:15:00Z', '2026-01-20T15:30:00Z',
    'Standup and design review', 'Office', '["calendar", "message", "transcription"]',
    0, 0, 0, 0, 0,
    'Standup then design review with David — walked through the onboarding prototype and got good feedback.', '["meeting", "standup", "design", "design-review", "onboarding"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 75
);

-- User research session (09:30-11:00 CST = 15:30-17:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0571', 'day_2026-01-20',
    '2026-01-20T15:30:00Z', '2026-01-20T17:00:00Z',
    'User research session', 'Office', '["calendar", "transcription"]',
    0, 0, 0, 0, 0,
    'Second onboarding user research interview — this customer had a very different setup journey.', '["research", "onboarding", "design"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 68
);

-- Lunch solo (11:00-12:00 CST = 17:00-18:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0572', 'day_2026-01-20',
    '2026-01-20T17:00:00Z', '2026-01-20T18:00:00Z',
    'Lunch', 'Office', '["location_visit"]',
    0, 0, 0, 0, 0,
    'Lunch at the office, leftover chili.', '["food"]', '["place_demo_office"]',
    NULL, NULL, NULL, 'NEW', 70
);

-- Afternoon work (12:00-16:30 CST = 18:00-22:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0573', 'day_2026-01-20',
    '2026-01-20T18:00:00Z', '2026-01-20T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Synthesized research notes and updated the onboarding journey map in Figma.', '["design", "figma", "work", "onboarding", "research"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 69
);

-- Bike commute home (16:30-17:00 CST = 22:30-23:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0574', 'day_2026-01-20',
    '2026-01-20T22:30:00Z', '2026-01-20T23:00:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    0, 1, 0, 0, 0,
    'Biked home.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 115
);

-- Evening run (17:30-18:20 CST = 23:30-00:20+1 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0575', 'day_2026-01-20',
    '2026-01-20T23:30:00Z', '2026-01-21T00:20:00Z',
    'Evening run', 'Mueller Trails', '["steps", "workout"]',
    0, 0, 0, 0, 0,
    'Tuesday evening run on Mueller trails, 3.5 miles.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
    NULL, NULL, NULL, 'NEW', 158
);

-- Dinner and TV (19:00-22:00 CST = 01:00-04:00+1 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0576', 'day_2026-01-20',
    '2026-01-21T01:00:00Z', '2026-01-21T04:00:00Z',
    'Dinner and TV', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Made a quick stir fry, watched TV.', '["food", "leisure"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 65
);

-- ── Wednesday, January 21, 2026 ─────────────────────────────────────────────

-- Sleep (00:00-06:30 CST = 06:00-12:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0577', 'day_2026-01-21',
    '2026-01-21T06:00:00Z', '2026-01-21T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Slept midnight to 6:30am.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 58
);

-- Morning routine (06:30-07:15 CST = 12:30-13:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0578', 'day_2026-01-21',
    '2026-01-21T12:30:00Z', '2026-01-21T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Coffee and checked messages.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 68
);

-- Bike commute (07:15-07:45 CST = 13:15-13:45 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0579', 'day_2026-01-21',
    '2026-01-21T13:15:00Z', '2026-01-21T13:45:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    0, 1, 0, 0, 0,
    'Biked to the office.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 131
);

-- Coffee and Slack (07:45-08:15 CST = 13:45-14:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0580', 'day_2026-01-21',
    '2026-01-21T13:45:00Z', '2026-01-21T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    0, 0, 0, 0, 0,
    'Coffee and Slack.', '["messaging", "work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 64
);

-- Standup (08:15-08:45 CST = 14:15-14:45 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0581', 'day_2026-01-21',
    '2026-01-21T14:15:00Z', '2026-01-21T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    0, 0, 0, 0, 0,
    'Standup — shared research findings with Maya and David, aligned on onboarding design direction.', '["meeting", "standup", "design", "onboarding"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 78
);

-- Focused work (08:45-11:30 CST = 14:45-17:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0582', 'day_2026-01-21',
    '2026-01-21T14:45:00Z', '2026-01-21T17:30:00Z',
    'Focused design work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Iterated on the onboarding flow based on research insights — simplified the account setup step.', '["design", "figma", "focus", "deep-work", "onboarding", "form-validation"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 68
);

-- Lunch with Maya at Tatsu-ya (11:30-12:30 CST = 17:30-18:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0583', 'day_2026-01-21',
    '2026-01-21T17:30:00Z', '2026-01-21T18:30:00Z',
    'Lunch with Maya', 'Ramen Tatsu-ya', '["location_visit", "transcription"]',
    0, 0, 0, 0, 0,
    'Lunch at Ramen Tatsu-ya with Maya, chatted about the research sessions and weekend plans.', '["social", "food", "ramen"]', '["person_demo_maya", "place_demo_ramen"]',
    NULL, NULL, NULL, 'NEW', 74
);

-- Afternoon work (12:30-16:30 CST = 18:30-22:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0584', 'day_2026-01-21',
    '2026-01-21T18:30:00Z', '2026-01-21T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Continued iterating on the onboarding prototype and prepared a research summary doc.', '["design", "figma", "work", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 72
);

-- Bike commute home (16:30-17:00 CST = 22:30-23:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0585', 'day_2026-01-21',
    '2026-01-21T22:30:00Z', '2026-01-21T23:00:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    0, 1, 0, 0, 0,
    'Biked home.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 133
);

-- Dinner and browsing (18:00-22:00 CST = 00:00-04:00+1 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0586', 'day_2026-01-21',
    '2026-01-22T00:00:00Z', '2026-01-22T04:00:00Z',
    'Dinner and browsing', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Made a salad for dinner, browsed the web and read before bed.', '["food", "leisure", "browsing"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 66
);

-- ── Thursday, January 22, 2026 ──────────────────────────────────────────────

-- Sleep (00:00-06:20 CST = 06:00-12:20 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0587', 'day_2026-01-22',
    '2026-01-22T06:00:00Z', '2026-01-22T12:20:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Slept from midnight to about 6:20am.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 60
);

-- Morning routine (06:20-07:10 CST = 12:20-13:10 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0588', 'day_2026-01-22',
    '2026-01-22T12:20:00Z', '2026-01-22T13:10:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Coffee and morning routine, checked texts from Rachel about scheduling a house showing this weekend.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 64
);

-- Bike commute (07:10-07:40 CST = 13:10-13:40 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0589', 'day_2026-01-22',
    '2026-01-22T13:10:00Z', '2026-01-22T13:40:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    0, 1, 0, 0, 0,
    'Biked to the office.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 134
);

-- Coffee and Slack (07:40-08:15 CST = 13:40-14:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0590', 'day_2026-01-22',
    '2026-01-22T13:40:00Z', '2026-01-22T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    0, 0, 0, 0, 0,
    'Coffee and Slack at the office.', '["messaging", "work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 69
);

-- Standup (08:15-08:45 CST = 14:15-14:45 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0591', 'day_2026-01-22',
    '2026-01-22T14:15:00Z', '2026-01-22T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    0, 0, 0, 0, 0,
    'Standup with Maya and David, talked about finishing the onboarding prototype this week.', '["meeting", "standup", "design", "onboarding"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 73
);

-- Focused work (08:45-11:30 CST = 14:45-17:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0592', 'day_2026-01-22',
    '2026-01-22T14:45:00Z', '2026-01-22T17:30:00Z',
    'Focused design work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Built out the step-3 and step-4 screens for the onboarding prototype in Figma.', '["design", "figma", "focus", "deep-work", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 63
);

-- Lunch solo (11:30-12:15 CST = 17:30-18:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0593', 'day_2026-01-22',
    '2026-01-22T17:30:00Z', '2026-01-22T18:15:00Z',
    'Lunch', 'Office', '["location_visit"]',
    0, 0, 0, 0, 0,
    'Ate lunch at my desk, sandwich from the deli.', '["food"]', '["place_demo_office"]',
    NULL, NULL, NULL, 'NEW', 72
);

-- Afternoon work (12:15-16:30 CST = 18:15-22:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0594', 'day_2026-01-22',
    '2026-01-22T18:15:00Z', '2026-01-22T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Finished the onboarding prototype first pass, shared it with the team for feedback.', '["design", "figma", "work", "onboarding", "messaging"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 68
);

-- Bike commute home (16:30-17:00 CST = 22:30-23:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0595', 'day_2026-01-22',
    '2026-01-22T22:30:00Z', '2026-01-22T23:00:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    0, 1, 0, 0, 0,
    'Biked home from the office.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 115
);

-- Walk on Mueller trails (17:30-18:15 CST = 23:30-00:15+1 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0596', 'day_2026-01-22',
    '2026-01-22T23:30:00Z', '2026-01-23T00:15:00Z',
    'Walk', 'Mueller Trails', '["steps"]',
    0, 0, 0, 0, 0,
    'Evening walk on Mueller trails.', '["exercise", "outdoors", "walking", "mueller-trails"]', '["place_demo_mueller_trails"]',
    NULL, NULL, NULL, 'NEW', 150
);

-- Dinner and reading (19:00-22:00 CST = 01:00-04:00+1 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0597', 'day_2026-01-22',
    '2026-01-23T01:00:00Z', '2026-01-23T04:00:00Z',
    'Dinner and reading', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Cooked pasta, read a few chapters of a novel before bed.', '["food", "leisure"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 58
);

-- ── Friday, January 23, 2026 — No game night this week, quiet Friday ────────

-- Sleep (00:00-06:30 CST = 06:00-12:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0598', 'day_2026-01-23',
    '2026-01-23T06:00:00Z', '2026-01-23T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Slept from midnight to 6:30am.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 61
);

-- Morning routine (06:30-07:15 CST = 12:30-13:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0599', 'day_2026-01-23',
    '2026-01-23T12:30:00Z', '2026-01-23T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Coffee and morning routine, texted Jess — she''s busy this weekend so no game night.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 67
);

-- Bike commute (07:15-07:45 CST = 13:15-13:45 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0600', 'day_2026-01-23',
    '2026-01-23T13:15:00Z', '2026-01-23T13:45:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    0, 1, 0, 0, 0,
    'Biked to the office.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 133
);

-- Coffee and Slack (07:45-08:15 CST = 13:45-14:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0601', 'day_2026-01-23',
    '2026-01-23T13:45:00Z', '2026-01-23T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    0, 0, 0, 0, 0,
    'Coffee at the office, checked Slack.', '["messaging", "work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 67
);

-- Standup (08:15-08:45 CST = 14:15-14:45 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0602', 'day_2026-01-23',
    '2026-01-23T14:15:00Z', '2026-01-23T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    0, 0, 0, 0, 0,
    'Friday standup, reviewed the week on the onboarding project.', '["meeting", "standup", "design", "onboarding"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 72
);

-- Focused work (08:45-11:30 CST = 14:45-17:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0603', 'day_2026-01-23',
    '2026-01-23T14:45:00Z', '2026-01-23T17:30:00Z',
    'Focused design work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Refined the onboarding prototype based on team feedback, polished transitions.', '["design", "figma", "focus", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 66
);

-- Lunch solo (11:30-12:15 CST = 17:30-18:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0604', 'day_2026-01-23',
    '2026-01-23T17:30:00Z', '2026-01-23T18:15:00Z',
    'Lunch', 'Office', '["location_visit"]',
    0, 0, 0, 0, 0,
    'Lunch at the office.', '["food"]', '["place_demo_office"]',
    NULL, NULL, NULL, 'NEW', 71
);

-- Afternoon work (12:15-15:30 CST = 18:15-21:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0605', 'day_2026-01-23',
    '2026-01-23T18:15:00Z', '2026-01-23T21:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Lighter Friday afternoon, wrapped up loose ends and organized research notes.', '["work", "design", "figma", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 66
);

-- Bike commute home (15:30-16:00 CST = 21:30-22:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0606', 'day_2026-01-23',
    '2026-01-23T21:30:00Z', '2026-01-23T22:00:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    0, 1, 0, 0, 0,
    'Biked home.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 120
);

-- Mom call (17:00-17:35 CST = 23:00-23:35 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0607', 'day_2026-01-23',
    '2026-01-23T23:00:00Z', '2026-01-23T23:35:00Z',
    'Phone call with Mom', 'Home', '["message", "transcription"]',
    0, 0, 0, 0, 0,
    'Weekly call with Mom, told her about seeing a house this weekend with Rachel.', '["family", "phone-call"]', '["person_demo_mom", "place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 69
);

-- Quiet Friday evening (18:00-22:00 CST = 00:00-04:00+1 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0608', 'day_2026-01-23',
    '2026-01-24T00:00:00Z', '2026-01-24T04:00:00Z',
    'Quiet evening', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Quiet Friday night at home — cooked dinner, watched a movie, early to bed.', '["food", "leisure"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 66
);

-- ── Saturday, January 24, 2026 ──────────────────────────────────────────────

-- Sleep (00:00-07:30 CST = 06:00-13:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0609', 'day_2026-01-24',
    '2026-01-24T06:00:00Z', '2026-01-24T13:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Slept in on Saturday, about 7.5 hours.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 57
);

-- Slow morning (07:30-09:00 CST = 13:30-15:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0610', 'day_2026-01-24',
    '2026-01-24T13:30:00Z', '2026-01-24T15:00:00Z',
    'Slow morning', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Lazy Saturday morning, coffee and catching up on articles.', '["routine", "morning", "coffee", "browsing"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 63
);

-- Lady Bird Lake walk (09:30-11:00 CST = 15:30-17:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0611', 'day_2026-01-24',
    '2026-01-24T15:30:00Z', '2026-01-24T17:00:00Z',
    'Lady Bird Lake walk', 'Lady Bird Lake', '["steps", "location_visit"]',
    0, 0, 0, 0, 0,
    'Walked the Lady Bird Lake boardwalk loop, beautiful clear winter morning.', '["exercise", "outdoors", "walking"]', '["place_demo_ladybird"]',
    NULL, NULL, NULL, 'NEW', 98
);

-- Errands and lunch (11:00-13:00 CST = 17:00-19:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0612', 'day_2026-01-24',
    '2026-01-24T17:00:00Z', '2026-01-24T19:00:00Z',
    'Errands and lunch', NULL, '["location_visit"]',
    0, 0, 0, 0, 0,
    'Ran errands at Target, grabbed a taco on the way home.', '["food"]', '[]',
    NULL, NULL, NULL, 'NEW', 82
);

-- Afternoon reading (13:30-17:00 CST = 19:30-23:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0613', 'day_2026-01-24',
    '2026-01-24T19:30:00Z', '2026-01-24T23:00:00Z',
    'Reading and journaling', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Read and did some journaling about the upcoming house showing tomorrow.', '["leisure", "reflection", "house-hunting"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 64
);

-- Dinner and movie (18:00-22:00 CST = 00:00-04:00+1 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0614', 'day_2026-01-24',
    '2026-01-25T00:00:00Z', '2026-01-25T04:00:00Z',
    'Dinner and movie', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Cooked dinner, then watched a movie at home.', '["food", "leisure"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 67
);

-- ── Sunday, January 25, 2026 — RACHEL SECOND APPEARANCE (house showing) ────

-- Sleep (00:00-07:30 CST = 06:00-13:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0615', 'day_2026-01-25',
    '2026-01-25T06:00:00Z', '2026-01-25T13:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Slept in on Sunday, about 7.5 hours.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 55
);

-- Slow morning (07:30-09:00 CST = 13:30-15:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0616', 'day_2026-01-25',
    '2026-01-25T13:30:00Z', '2026-01-25T15:00:00Z',
    'Slow morning', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Sunday morning, coffee and the crossword, a bit nervous about the house showing later.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 66
);

-- Morning run (09:00-10:00 CST = 15:00-16:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0617', 'day_2026-01-25',
    '2026-01-25T15:00:00Z', '2026-01-25T16:00:00Z',
    'Morning run', 'Mueller Trails', '["steps", "workout"]',
    0, 0, 0, 0, 0,
    'Quick run on Mueller trails before the house showing, 3 miles.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
    NULL, NULL, NULL, 'NEW', 67
);

-- ** RACHEL SECOND APPEARANCE ** House showing (11:00-11:45 CST = 17:00-17:45 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0618', 'day_2026-01-25',
    '2026-01-25T17:00:00Z', '2026-01-25T17:45:00Z',
    'House showing', 'East Austin', '["location_visit", "transcription"]',
    0, 0, 0, 0, 0,
    'Toured a 2-bed bungalow on Webberville Rd with Rachel — cute but the kitchen was too small and the yard was tiny, not feeling it.', '["house-hunting", "real-estate", "neighborhood"]', '["person_demo_rachel", "org_demo_realty"]',
    NULL, NULL, NULL, 'NEW', 82
);

-- Lunch (12:00-13:00 CST = 18:00-19:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0619', 'day_2026-01-25',
    '2026-01-25T18:00:00Z', '2026-01-25T19:00:00Z',
    'Lunch', NULL, '["location_visit"]',
    0, 0, 0, 0, 0,
    'Grabbed a quick lunch at a taco truck after the showing.', '["food"]', '[]',
    NULL, NULL, NULL, 'NEW', 76
);

-- Cooking and meal prep (13:30-15:30 CST = 19:30-21:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0620', 'day_2026-01-25',
    '2026-01-25T19:30:00Z', '2026-01-25T21:30:00Z',
    'Cooking', 'Home', '["location_visit"]',
    0, 0, 0, 0, 0,
    'Meal prepped curry and rice for the week ahead.', '["food"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 69
);

-- Afternoon reading (15:30-17:30 CST = 21:30-23:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0621', 'day_2026-01-25',
    '2026-01-25T21:30:00Z', '2026-01-25T23:30:00Z',
    'Reading', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Read for a couple hours, reflected on the house showing — not the right place.', '["leisure", "reflection", "house-hunting"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 59
);

-- Dinner and wind down (18:00-22:00 CST = 00:00-04:00+1 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0622', 'day_2026-01-25',
    '2026-01-26T00:00:00Z', '2026-01-26T04:00:00Z',
    'Dinner and wind down', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Simple dinner from the curry batch, watched TV and got ready for the week.', '["food", "leisure"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 67
);
