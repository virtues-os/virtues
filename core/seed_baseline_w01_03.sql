-- =============================================================================
-- Baseline Seed: Weeks 1-3 — November 24 through December 14, 2025
-- =============================================================================
--
-- Character: UX designer, early 30s, lives in Mueller (East Austin), works
--            downtown at Canopy (B2B SaaS). This is the start of the baseline —
--            no house-hunting, no Rachel, generic work topics.
--
-- Event IDs: ev_b0001 through ev_b0210 (approximately)
-- Day IDs:   day_2025-11-24 through day_2025-12-14
--
-- All times UTC. CST (UTC-6) in effect for November/December 2025.
-- Example: 06:30 CST = 12:30 UTC, midnight CST = 06:00 UTC next day.
--
-- Thanksgiving: Thursday Nov 27 — quiet day at home, no office.
-- Game nights at Jess's: Fri Nov 28 and Fri Dec 12 (skip Dec 5).
-- Mom calls: Sat Nov 29 and Sun Dec 7 (skip one weekend).
--
-- Usage: sqlite3 core/data/virtues.db < core/seed_baseline_w01_03.sql
-- =============================================================================

-- ─────────────────────────────────────────────────────────────────────────────
-- 0. IDEMPOTENT CLEANUP
-- ─────────────────────────────────────────────────────────────────────────────
DELETE FROM wiki_events WHERE id LIKE 'ev_b0%' AND CAST(SUBSTR(id, 5) AS INTEGER) BETWEEN 1 AND 210;

-- ─────────────────────────────────────────────────────────────────────────────
-- 1. WIKI DAYS
-- ─────────────────────────────────────────────────────────────────────────────
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-11-24', '2025-11-24', 'America/Chicago', 'America/Chicago', 0.48);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-11-25', '2025-11-25', 'America/Chicago', 'America/Chicago', 0.52);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-11-26', '2025-11-26', 'America/Chicago', 'America/Chicago', 0.50);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-11-27', '2025-11-27', 'America/Chicago', 'America/Chicago', 0.55);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-11-28', '2025-11-28', 'America/Chicago', 'America/Chicago', 0.45);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-11-29', '2025-11-29', 'America/Chicago', 'America/Chicago', 0.50);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-11-30', '2025-11-30', 'America/Chicago', 'America/Chicago', 0.47);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-12-01', '2025-12-01', 'America/Chicago', 'America/Chicago', 0.50);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-12-02', '2025-12-02', 'America/Chicago', 'America/Chicago', 0.53);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-12-03', '2025-12-03', 'America/Chicago', 'America/Chicago', 0.48);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-12-04', '2025-12-04', 'America/Chicago', 'America/Chicago', 0.51);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-12-05', '2025-12-05', 'America/Chicago', 'America/Chicago', 0.46);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-12-06', '2025-12-06', 'America/Chicago', 'America/Chicago', 0.54);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-12-07', '2025-12-07', 'America/Chicago', 'America/Chicago', 0.49);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-12-08', '2025-12-08', 'America/Chicago', 'America/Chicago', 0.52);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-12-09', '2025-12-09', 'America/Chicago', 'America/Chicago', 0.50);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-12-10', '2025-12-10', 'America/Chicago', 'America/Chicago', 0.44);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-12-11', '2025-12-11', 'America/Chicago', 'America/Chicago', 0.55);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-12-12', '2025-12-12', 'America/Chicago', 'America/Chicago', 0.48);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-12-13', '2025-12-13', 'America/Chicago', 'America/Chicago', 0.52);
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline) VALUES ('day_2025-12-14', '2025-12-14', 'America/Chicago', 'America/Chicago', 0.50);

-- ─────────────────────────────────────────────────────────────────────────────
-- 2. WIKI EVENTS
-- ─────────────────────────────────────────────────────────────────────────────

-- ═══════════════════════════════════════════════════════════════════════════
-- WEEK 1: Nov 24 (Mon) – Nov 30 (Sun)
-- ═══════════════════════════════════════════════════════════════════════════

-- ── Monday, November 24, 2025 ──────────────────────────────────────────────

-- E01: Sleep (midnight-6:30 CST = 06:00-12:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0001', 'day_2025-11-24',
    '2025-11-24T06:00:00Z', '2025-11-24T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Slept about 6.5 hours, woke up at 6:30am.', '["sleep"]', '[]',
    NULL, 'NEW', 62
);

-- E02: Morning routine (06:30-07:15 CST = 12:30-13:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0002', 'day_2025-11-24',
    '2025-11-24T12:30:00Z', '2025-11-24T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Coffee and scrolling through messages at home.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
    NULL, 'NEW', 68
);

-- E03: Bike commute (07:15-07:45 CST = 13:15-13:45 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0003', 'day_2025-11-24',
    '2025-11-24T13:15:00Z', '2025-11-24T13:45:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    0, 1, 0, 0, 0,
    'Biked to the office, chilly morning.', '["commute", "cycling", "morning"]', '[]',
    NULL, 'NEW', 130
);

-- E04: Coffee and Slack (07:45-08:15 CST = 13:45-14:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0004', 'day_2025-11-24',
    '2025-11-24T13:45:00Z', '2025-11-24T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    0, 0, 0, 0, 0,
    'Settled in at the office with coffee, catching up on Slack.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 64
);

-- E05: Design standup (08:15-08:45 CST = 14:15-14:45 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0005', 'day_2025-11-24',
    '2025-11-24T14:15:00Z', '2025-11-24T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    0, 0, 0, 0, 0,
    'Monday standup with Maya and David, planning the week.', '["meeting", "standup", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 77
);

-- E06: Focused design work (08:45-11:30 CST = 14:45-17:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0006', 'day_2025-11-24',
    '2025-11-24T14:45:00Z', '2025-11-24T17:30:00Z',
    'Focused design work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Deep work in Figma on the settings page redesign.', '["design", "figma", "deep-work", "focus"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 64
);

-- E07: Solo lunch (11:30-12:15 CST = 17:30-18:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0007', 'day_2025-11-24',
    '2025-11-24T17:30:00Z', '2025-11-24T18:15:00Z',
    'Lunch', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Quick solo lunch at the office, ate at desk.', '["food", "lunch"]', '["place_demo_office"]',
    NULL, 'NEW', 70
);

-- E08: Afternoon work (12:15-16:30 CST = 18:15-22:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0008', 'day_2025-11-24',
    '2025-11-24T18:15:00Z', '2025-11-24T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Worked on wireframes and responded to design feedback on Slack.', '["work", "design", "figma"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 72
);

-- E09: Bike commute home (16:30-17:00 CST = 22:30-23:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0009', 'day_2025-11-24',
    '2025-11-24T22:30:00Z', '2025-11-24T23:00:00Z',
    'Bike commute', NULL, '["location_visit"]',
    0, 1, 0, 0, 0,
    'Biked home from the office.', '["commute", "cycling"]', '[]',
    NULL, 'NEW', 119
);

-- E10: Evening at home (17:00-22:00 CST = 23:00-04:00+1 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0010', 'day_2025-11-24',
    '2025-11-24T23:00:00Z', '2025-11-25T04:00:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Made pasta for dinner, watched a couple episodes of a show, read before bed.', '["food", "leisure", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 68
);

-- ── Tuesday, November 25, 2025 ─────────────────────────────────────────────

-- E11: Sleep
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0011', 'day_2025-11-25',
    '2025-11-25T04:00:00Z', '2025-11-25T12:45:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Slept about 6.75 hours, woke a bit before the alarm.', '["sleep"]', '[]',
    NULL, 'NEW', 59
);

-- E12: Morning routine
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0012', 'day_2025-11-25',
    '2025-11-25T12:45:00Z', '2025-11-25T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Quick morning routine, checked email and weather.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
    NULL, 'NEW', 67
);

-- E13: Bike commute
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0013', 'day_2025-11-25',
    '2025-11-25T13:15:00Z', '2025-11-25T13:45:00Z',
    'Bike commute', NULL, '["location_visit"]',
    0, 1, 0, 0, 0,
    'Biked to the office, cool and overcast.', '["commute", "cycling", "morning"]', '[]',
    NULL, 'NEW', 126
);

-- E14: Coffee and Slack
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0014', 'day_2025-11-25',
    '2025-11-25T13:45:00Z', '2025-11-25T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    0, 0, 0, 0, 0,
    'Coffee at the office, catching up on overnight Slack threads.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 67
);

-- E15: Standup
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0015', 'day_2025-11-25',
    '2025-11-25T14:15:00Z', '2025-11-25T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    0, 0, 0, 0, 0,
    'Standup with Maya and David, talked about the short week ahead of Thanksgiving.', '["meeting", "standup", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 75
);

-- E16: Design review with David (Tuesday special)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0016', 'day_2025-11-25',
    '2025-11-25T15:00:00Z', '2025-11-25T16:00:00Z',
    'Design review', 'Office', '["calendar", "message"]',
    0, 0, 0, 0, 0,
    'Design review with David going over component library updates.', '["meeting", "design-review", "design"]', '["person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 77
);

-- E17: Focused work
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0017', 'day_2025-11-25',
    '2025-11-25T16:00:00Z', '2025-11-25T17:30:00Z',
    'Focused work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Heads-down time in Figma iterating on dashboard layouts.', '["design", "figma", "deep-work", "focus"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 68
);

-- E18: Lunch (solo)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0018', 'day_2025-11-25',
    '2025-11-25T17:30:00Z', '2025-11-25T18:15:00Z',
    'Lunch', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Grabbed a salad from the place downstairs, ate in the break room.', '["food", "lunch"]', '["place_demo_office"]',
    NULL, 'NEW', 69
);

-- E19: Afternoon work
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0019', 'day_2025-11-25',
    '2025-11-25T18:15:00Z', '2025-11-25T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Finished up the settings page mockups and pushed to the design repo.', '["work", "design", "figma"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 68
);

-- E20: Bike commute home
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0020', 'day_2025-11-25',
    '2025-11-25T22:30:00Z', '2025-11-25T23:00:00Z',
    'Bike commute', NULL, '["location_visit"]',
    0, 1, 0, 0, 0,
    'Biked home, sun setting early now.', '["commute", "cycling"]', '[]',
    NULL, 'NEW', 119
);

-- E21: Evening run (Tuesday = Mueller trails)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0021', 'day_2025-11-25',
    '2025-11-25T23:15:00Z', '2025-11-26T00:00:00Z',
    'Evening run', 'Mueller Trails', '["steps", "workout"]',
    0, 0, 0, 0, 0,
    'Quick 3-mile run on Mueller trails before it got dark.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
    NULL, 'NEW', 150
);

-- E22: Evening at home
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0022', 'day_2025-11-25',
    '2025-11-26T00:00:00Z', '2025-11-26T04:00:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Showered, made stir-fry, browsed the internet for a while.', '["food", "leisure", "browsing", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 63
);

-- ── Wednesday, November 26, 2025 (day before Thanksgiving) ─────────────────

-- E23: Sleep
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0023', 'day_2025-11-26',
    '2025-11-26T04:00:00Z', '2025-11-26T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Slept about 6.5 hours.', '["sleep"]', '[]',
    NULL, 'NEW', 59
);

-- E24: Morning routine
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0024', 'day_2025-11-26',
    '2025-11-26T12:30:00Z', '2025-11-26T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Morning coffee and checking texts, people already off for the holiday.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
    NULL, 'NEW', 67
);

-- E25: Bike commute
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0025', 'day_2025-11-26',
    '2025-11-26T13:15:00Z', '2025-11-26T13:45:00Z',
    'Bike commute', NULL, '["location_visit"]',
    0, 1, 0, 0, 0,
    'Biked to the office, half the office already on holiday.', '["commute", "cycling", "morning"]', '[]',
    NULL, 'NEW', 135
);

-- E26: Coffee and Slack
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0026', 'day_2025-11-26',
    '2025-11-26T13:45:00Z', '2025-11-26T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    0, 0, 0, 0, 0,
    'Quiet office, half the team out. Quick coffee.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 67
);

-- E27: Standup (short, pre-holiday)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0027', 'day_2025-11-26',
    '2025-11-26T14:15:00Z', '2025-11-26T14:30:00Z',
    'Design standup', 'Office', '["calendar"]',
    0, 0, 0, 0, 0,
    'Quick pre-holiday standup, just Maya on the call.', '["meeting", "standup"]', '["person_demo_maya", "place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 76
);

-- E28: Focused work (wrapping up before holiday)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0028', 'day_2025-11-26',
    '2025-11-26T14:30:00Z', '2025-11-26T17:30:00Z',
    'Focused work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Wrapped up loose ends before the holiday break, updated Jira tickets.', '["work", "focus", "deep-work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 71
);

-- E29: Lunch with Maya at Tatsu-ya (Wednesday = Tatsu-ya day)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0029', 'day_2025-11-26',
    '2025-11-26T17:30:00Z', '2025-11-26T18:30:00Z',
    'Lunch at Ramen Tatsu-ya', 'Ramen Tatsu-ya', '["location_visit"]',
    0, 0, 0, 0, 0,
    'Pre-Thanksgiving ramen with Maya at Tatsu-ya.', '["food", "social", "ramen"]', '["person_demo_maya", "place_demo_ramen"]',
    NULL, 'NEW', 73
);

-- E30: Short afternoon (left early)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0030', 'day_2025-11-26',
    '2025-11-26T18:30:00Z', '2025-11-26T21:00:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Came back to the office for a couple hours then headed out early.', '["work", "design"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 69
);

-- E31: Bike commute home (early)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0031', 'day_2025-11-26',
    '2025-11-26T21:00:00Z', '2025-11-26T21:30:00Z',
    'Bike commute', NULL, '["location_visit"]',
    0, 1, 0, 0, 0,
    'Biked home early, nice to leave in daylight for once.', '["commute", "cycling"]', '[]',
    NULL, 'NEW', 126
);

-- E32: Evening — groceries and cooking
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0032', 'day_2025-11-26',
    '2025-11-26T21:30:00Z', '2025-11-27T04:00:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Stopped for groceries, prepped some food for tomorrow, watched a movie.', '["food", "leisure", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 62
);

-- ── Thursday, November 27, 2025 (Thanksgiving) ─────────────────────────────

-- E33: Sleep (slept in)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0033', 'day_2025-11-27',
    '2025-11-27T04:00:00Z', '2025-11-27T14:00:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Slept in on Thanksgiving morning, about 8 hours.', '["sleep"]', '[]',
    NULL, 'NEW', 61
);

-- E34: Slow morning
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0034', 'day_2025-11-27',
    '2025-11-27T14:00:00Z', '2025-11-27T15:30:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Lazy Thanksgiving morning with coffee and texts from family.', '["routine", "morning", "coffee", "family"]', '["place_demo_home"]',
    NULL, 'NEW', 63
);

-- E35: Morning walk
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0035', 'day_2025-11-27',
    '2025-11-27T15:30:00Z', '2025-11-27T16:30:00Z',
    'Morning walk', 'Mueller Trails', '["steps"]',
    0, 0, 0, 0, 0,
    'Went for a walk around Mueller to clear my head, the neighborhood was quiet.', '["exercise", "outdoors"]', '["place_demo_mueller_trails"]',
    NULL, 'NEW', 67
);

-- E36: Cooking Thanksgiving meal
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0036', 'day_2025-11-27',
    '2025-11-27T16:30:00Z', '2025-11-27T20:00:00Z',
    'Cooking', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Made a small Thanksgiving dinner for one — roasted chicken, mashed potatoes, pie from the store.', '["food", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 75
);

-- E37: Phone call with Mom
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0037', 'day_2025-11-27',
    '2025-11-27T20:00:00Z', '2025-11-27T20:45:00Z',
    'Phone call with Mom', 'Home', '["transcription"]',
    0, 0, 0, 0, 0,
    'Called Mom for Thanksgiving, she was at her sister''s. Talked about her garden and holiday plans.', '["family", "phone-call"]', '["person_demo_mom", "place_demo_home"]',
    NULL, 'NEW', 66
);

-- E38: Thanksgiving evening
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0038', 'day_2025-11-27',
    '2025-11-27T20:45:00Z', '2025-11-28T04:00:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Ate Thanksgiving dinner, watched a movie, texted Jess about plans for tomorrow.', '["food", "leisure", "messaging", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 63
);

-- ── Friday, November 28, 2025 (Black Friday — game night at Jess's) ────────

-- E39: Sleep
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0039', 'day_2025-11-28',
    '2025-11-28T04:00:00Z', '2025-11-28T14:00:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Slept in, no work today.', '["sleep"]', '[]',
    NULL, 'NEW', 62
);

-- E40: Slow morning
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0040', 'day_2025-11-28',
    '2025-11-28T14:00:00Z', '2025-11-28T15:30:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Slow Black Friday morning, coffee and reading online.', '["routine", "morning", "coffee", "browsing"]', '["place_demo_home"]',
    NULL, 'NEW', 66
);

-- E41: Lady Bird Lake walk
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0041', 'day_2025-11-28',
    '2025-11-28T16:00:00Z', '2025-11-28T17:30:00Z',
    'Walk at Lady Bird Lake', 'Lady Bird Lake', '["steps", "location_visit"]',
    0, 0, 0, 0, 0,
    'Walked around Lady Bird Lake, the trail was busy with holiday runners.', '["exercise", "outdoors"]', '["place_demo_ladybird"]',
    NULL, 'NEW', 92
);

-- E42: Afternoon at home
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0042', 'day_2025-11-28',
    '2025-11-28T17:30:00Z', '2025-11-28T23:00:00Z',
    'Afternoon at home', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Leftover Thanksgiving food for lunch, read a book, did some online browsing.', '["food", "leisure", "browsing", "reading"]', '["place_demo_home"]',
    NULL, 'NEW', 67
);

-- E43: Game night at Jess's
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0043', 'day_2025-11-28',
    '2025-11-29T00:00:00Z', '2025-11-29T04:00:00Z',
    'Game night', 'Jess''s Place', '["location_visit"]',
    0, 0, 0, 0, 0,
    'Game night at Jess''s with Priya — played Catan and Codenames, ate leftover pie.', '["social", "games"]', '["person_demo_jess", "person_demo_priya", "place_demo_jess"]',
    NULL, 'NEW', 71
);

-- ── Saturday, November 29, 2025 ────────────────────────────────────────────

-- E44: Sleep
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0044', 'day_2025-11-29',
    '2025-11-29T04:00:00Z', '2025-11-29T13:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Slept in until about 7:30am after game night.', '["sleep"]', '[]',
    NULL, 'NEW', 56
);

-- E45: Slow morning
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0045', 'day_2025-11-29',
    '2025-11-29T13:30:00Z', '2025-11-29T15:00:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Slow Saturday morning, coffee and podcasts.', '["routine", "morning", "coffee", "podcast"]', '["place_demo_home"]',
    NULL, 'NEW', 65
);

-- E46: Lady Bird Lake walk
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0046', 'day_2025-11-29',
    '2025-11-29T15:00:00Z', '2025-11-29T16:30:00Z',
    'Walk at Lady Bird Lake', 'Lady Bird Lake', '["steps", "location_visit"]',
    0, 0, 0, 0, 0,
    'Nice walk around the lake, warm for late November.', '["exercise", "outdoors"]', '["place_demo_ladybird"]',
    NULL, 'NEW', 88
);

-- E47: Errands and afternoon
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0047', 'day_2025-11-29',
    '2025-11-29T16:30:00Z', '2025-11-29T21:00:00Z',
    'Errands and reading', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Ran some errands, then spent the afternoon reading at home.', '["leisure", "reading"]', '["place_demo_home"]',
    NULL, 'NEW', 65
);

-- E48: Mom call (Saturday this week since Thanksgiving was Thursday)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0048', 'day_2025-11-29',
    '2025-11-29T21:00:00Z', '2025-11-29T21:30:00Z',
    'Phone call with Mom', 'Home', '["transcription"]',
    0, 0, 0, 0, 0,
    'Quick follow-up call with Mom, she wanted the pie recipe I used.', '["family", "phone-call"]', '["person_demo_mom", "place_demo_home"]',
    NULL, 'NEW', 65
);

-- E49: Evening
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0049', 'day_2025-11-29',
    '2025-11-29T21:30:00Z', '2025-11-30T04:30:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Cooked dinner, watched a documentary, early night.', '["food", "leisure", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 67
);

-- ── Sunday, November 30, 2025 ──────────────────────────────────────────────

-- E50: Sleep
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0050', 'day_2025-11-30',
    '2025-11-30T04:30:00Z', '2025-11-30T14:00:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Slept well, about 7.5 hours.', '["sleep"]', '[]',
    NULL, 'NEW', 60
);

-- E51: Slow morning
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0051', 'day_2025-11-30',
    '2025-11-30T14:00:00Z', '2025-11-30T15:30:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Sunday morning, made a big breakfast and read the news.', '["routine", "morning", "coffee", "food"]', '["place_demo_home"]',
    NULL, 'NEW', 63
);

-- E52: Mueller run
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0052', 'day_2025-11-30',
    '2025-11-30T15:30:00Z', '2025-11-30T16:15:00Z',
    'Morning run', 'Mueller Trails', '["steps", "workout"]',
    0, 0, 0, 0, 0,
    'Short run around Mueller, legs were a bit tired.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
    NULL, 'NEW', 67
);

-- E53: Afternoon reading and meal prep
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0053', 'day_2025-11-30',
    '2025-11-30T16:15:00Z', '2025-11-30T21:00:00Z',
    'Afternoon at home', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Meal prepped for the week, did some reading, reorganized the bookshelf.', '["food", "leisure", "reading", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 64
);

-- E54: Evening
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0054', 'day_2025-11-30',
    '2025-11-30T21:00:00Z', '2025-12-01T04:00:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Quiet Sunday evening, prepped bag for tomorrow, caught up on a podcast.', '["leisure", "routine", "podcast"]', '["place_demo_home"]',
    NULL, 'NEW', 64
);

-- ═══════════════════════════════════════════════════════════════════════════
-- WEEK 2: Dec 1 (Mon) – Dec 7 (Sun)
-- ═══════════════════════════════════════════════════════════════════════════

-- ── Monday, December 1, 2025 ───────────────────────────────────────────────

-- E55: Sleep
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0055', 'day_2025-12-01',
    '2025-12-01T04:00:00Z', '2025-12-01T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'About 6.5 hours sleep, alarm went off at 6:30.', '["sleep"]', '[]',
    NULL, 'NEW', 59
);

-- E56: Morning routine
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0056', 'day_2025-12-01',
    '2025-12-01T12:30:00Z', '2025-12-01T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Back to the grind after the long weekend, coffee and Slack.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
    NULL, 'NEW', 63
);

-- E57: Bike commute
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0057', 'day_2025-12-01',
    '2025-12-01T13:15:00Z', '2025-12-01T13:45:00Z',
    'Bike commute', NULL, '["location_visit"]',
    0, 1, 0, 0, 0,
    'Biked to the office, cold December morning.', '["commute", "cycling", "morning"]', '[]',
    NULL, 'NEW', 133
);

-- E58: Coffee and Slack
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0058', 'day_2025-12-01',
    '2025-12-01T13:45:00Z', '2025-12-01T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    0, 0, 0, 0, 0,
    'Office coffee, catching up on the backlog from the holiday break.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 69
);

-- E59: Standup
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0059', 'day_2025-12-01',
    '2025-12-01T14:15:00Z', '2025-12-01T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    0, 0, 0, 0, 0,
    'Monday standup with Maya and David, recapping what got done before the break.', '["meeting", "standup", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 78
);

-- E60: Focused work (long morning block)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0060', 'day_2025-12-01',
    '2025-12-01T14:45:00Z', '2025-12-01T17:30:00Z',
    'Focused design work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Long focus block in Figma working on the notifications page.', '["design", "figma", "deep-work", "focus"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 64
);

-- E61: Lunch (solo)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0061', 'day_2025-12-01',
    '2025-12-01T17:30:00Z', '2025-12-01T18:15:00Z',
    'Lunch', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Brought leftovers from the weekend meal prep.', '["food", "lunch"]', '["place_demo_office"]',
    NULL, 'NEW', 67
);

-- E62: Afternoon meetings + work
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0062', 'day_2025-12-01',
    '2025-12-01T18:15:00Z', '2025-12-01T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage", "message"]',
    0, 0, 0, 0, 0,
    'Afternoon of Slack conversations and wrapping up the notifications page.', '["work", "messaging", "design"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 71
);

-- E63: Bike commute home
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0063', 'day_2025-12-01',
    '2025-12-01T22:30:00Z', '2025-12-01T23:00:00Z',
    'Bike commute', NULL, '["location_visit"]',
    0, 1, 0, 0, 0,
    'Biked home, dark already at 5pm.', '["commute", "cycling"]', '[]',
    NULL, 'NEW', 134
);

-- E64: Evening
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0064', 'day_2025-12-01',
    '2025-12-01T23:00:00Z', '2025-12-02T04:00:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Made tacos for dinner, watched some TV, early bedtime.', '["food", "leisure", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 61
);

-- ── Tuesday, December 2, 2025 ──────────────────────────────────────────────

-- E65: Sleep
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0065', 'day_2025-12-02',
    '2025-12-02T04:00:00Z', '2025-12-02T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Slept okay, woke up once around 4am.', '["sleep"]', '[]',
    NULL, 'NEW', 60
);

-- E66: Morning routine
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0066', 'day_2025-12-02',
    '2025-12-02T12:30:00Z', '2025-12-02T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Coffee and morning routine, checked messages.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
    NULL, 'NEW', 64
);

-- E67: Bike commute
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0067', 'day_2025-12-02',
    '2025-12-02T13:15:00Z', '2025-12-02T13:45:00Z',
    'Bike commute', NULL, '["location_visit"]',
    0, 1, 0, 0, 0,
    'Biked to the office, windy morning.', '["commute", "cycling", "morning"]', '[]',
    NULL, 'NEW', 130
);

-- E68: Coffee and Slack
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0068', 'day_2025-12-02',
    '2025-12-02T13:45:00Z', '2025-12-02T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    0, 0, 0, 0, 0,
    'Got settled in with coffee, reviewed PRs on Slack.', '["messaging", "work", "coffee", "code-review"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 72
);

-- E69: Standup
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0069', 'day_2025-12-02',
    '2025-12-02T14:15:00Z', '2025-12-02T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    0, 0, 0, 0, 0,
    'Standup with Maya and David, discussed upcoming sprint goals.', '["meeting", "standup", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 70
);

-- E70: Design review with David
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0070', 'day_2025-12-02',
    '2025-12-02T15:00:00Z', '2025-12-02T16:00:00Z',
    'Design review', 'Office', '["calendar", "message"]',
    0, 0, 0, 0, 0,
    'Design review with David, going through the notification flow iterations.', '["meeting", "design-review", "design"]', '["person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 75
);

-- E71: Focused work
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0071', 'day_2025-12-02',
    '2025-12-02T16:00:00Z', '2025-12-02T17:30:00Z',
    'Focused work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Applied review feedback to the notification designs.', '["design", "figma", "focus"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 65
);

-- E72: Lunch (solo)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0072', 'day_2025-12-02',
    '2025-12-02T17:30:00Z', '2025-12-02T18:15:00Z',
    'Lunch', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Lunch at desk, leftovers again.', '["food", "lunch"]', '["place_demo_office"]',
    NULL, 'NEW', 71
);

-- E73: Afternoon work
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0073', 'day_2025-12-02',
    '2025-12-02T18:15:00Z', '2025-12-02T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Finished the notifications mockups, shared in Slack for async feedback.', '["work", "design", "figma"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 72
);

-- E74: Bike commute home
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0074', 'day_2025-12-02',
    '2025-12-02T22:30:00Z', '2025-12-02T23:00:00Z',
    'Bike commute', NULL, '["location_visit"]',
    0, 1, 0, 0, 0,
    'Biked home in the dark.', '["commute", "cycling"]', '[]',
    NULL, 'NEW', 116
);

-- E75: Evening run (Tuesday = Mueller)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0075', 'day_2025-12-02',
    '2025-12-02T23:15:00Z', '2025-12-03T00:00:00Z',
    'Evening run', 'Mueller Trails', '["steps", "workout"]',
    0, 0, 0, 0, 0,
    'Ran 3.5 miles on Mueller trails, felt good despite the cold.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
    NULL, 'NEW', 156
);

-- E76: Evening at home
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0076', 'day_2025-12-02',
    '2025-12-03T00:00:00Z', '2025-12-03T04:30:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Shower, quick dinner, read before bed.', '["food", "leisure", "reading"]', '["place_demo_home"]',
    NULL, 'NEW', 62
);

-- ── Wednesday, December 3, 2025 ────────────────────────────────────────────

-- E77: Sleep
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0077', 'day_2025-12-03',
    '2025-12-03T04:30:00Z', '2025-12-03T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'About 6 hours sleep, stayed up a bit late reading.', '["sleep"]', '[]',
    NULL, 'NEW', 62
);

-- E78: Morning routine
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0078', 'day_2025-12-03',
    '2025-12-03T12:30:00Z', '2025-12-03T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Groggy morning, extra coffee needed.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
    NULL, 'NEW', 63
);

-- E79: Bike commute
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0079', 'day_2025-12-03',
    '2025-12-03T13:15:00Z', '2025-12-03T13:45:00Z',
    'Bike commute', NULL, '["location_visit"]',
    0, 1, 0, 0, 0,
    'Biked to the office, wore an extra layer today.', '["commute", "cycling", "morning"]', '[]',
    NULL, 'NEW', 132
);

-- E80: Coffee and Slack
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0080', 'day_2025-12-03',
    '2025-12-03T13:45:00Z', '2025-12-03T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    0, 0, 0, 0, 0,
    'Coffee and Slack, the usual.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 72
);

-- E81: Standup
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0081', 'day_2025-12-03',
    '2025-12-03T14:15:00Z', '2025-12-03T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    0, 0, 0, 0, 0,
    'Wednesday standup, mostly status updates on current designs.', '["meeting", "standup", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 76
);

-- E82: Focused work
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0082', 'day_2025-12-03',
    '2025-12-03T14:45:00Z', '2025-12-03T17:30:00Z',
    'Focused work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Worked on interaction prototypes for the settings flow.', '["design", "figma", "deep-work", "focus"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 70
);

-- E83: Lunch with Maya at Tatsu-ya
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0083', 'day_2025-12-03',
    '2025-12-03T17:30:00Z', '2025-12-03T18:30:00Z',
    'Lunch at Ramen Tatsu-ya', 'Ramen Tatsu-ya', '["location_visit"]',
    0, 0, 0, 0, 0,
    'Wednesday ramen with Maya at Tatsu-ya, talked about holiday plans.', '["food", "social", "ramen"]', '["person_demo_maya", "place_demo_ramen"]',
    NULL, 'NEW', 70
);

-- E84: Afternoon work
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0084', 'day_2025-12-03',
    '2025-12-03T18:30:00Z', '2025-12-03T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Polished the prototype and shared it with the engineering team.', '["work", "design", "figma"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 72
);

-- E85: Bike commute home
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0085', 'day_2025-12-03',
    '2025-12-03T22:30:00Z', '2025-12-03T23:00:00Z',
    'Bike commute', NULL, '["location_visit"]',
    0, 1, 0, 0, 0,
    'Biked home, cold but clear evening.', '["commute", "cycling"]', '[]',
    NULL, 'NEW', 134
);

-- E86: Evening at home
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0086', 'day_2025-12-03',
    '2025-12-03T23:00:00Z', '2025-12-04T04:00:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Made a big salad for dinner, watched a documentary about architecture.', '["food", "leisure", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 66
);

-- ── Thursday, December 4, 2025 (WFH afternoon) ────────────────────────────

-- E87: Sleep
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0087', 'day_2025-12-04',
    '2025-12-04T04:00:00Z', '2025-12-04T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Solid 6.5 hours of sleep.', '["sleep"]', '[]',
    NULL, 'NEW', 55
);

-- E88: Morning routine
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0088', 'day_2025-12-04',
    '2025-12-04T12:30:00Z', '2025-12-04T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Morning coffee, checked the weather and Slack.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
    NULL, 'NEW', 66
);

-- E89: Bike commute
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0089', 'day_2025-12-04',
    '2025-12-04T13:15:00Z', '2025-12-04T13:45:00Z',
    'Bike commute', NULL, '["location_visit"]',
    0, 1, 0, 0, 0,
    'Biked to the office for the morning.', '["commute", "cycling", "morning"]', '[]',
    NULL, 'NEW', 114
);

-- E90: Coffee and Slack
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0090', 'day_2025-12-04',
    '2025-12-04T13:45:00Z', '2025-12-04T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    0, 0, 0, 0, 0,
    'Office coffee, reading through design feedback threads.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 68
);

-- E91: Standup
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0091', 'day_2025-12-04',
    '2025-12-04T14:15:00Z', '2025-12-04T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    0, 0, 0, 0, 0,
    'Thursday standup, quick sync on handoff items for engineering.', '["meeting", "standup", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 72
);

-- E92: Morning work at office
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0092', 'day_2025-12-04',
    '2025-12-04T14:45:00Z', '2025-12-04T17:30:00Z',
    'Morning work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Worked on handoff docs for the settings page redesign.', '["work", "design", "figma"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 66
);

-- E93: Lunch (solo at office)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0093', 'day_2025-12-04',
    '2025-12-04T17:30:00Z', '2025-12-04T18:00:00Z',
    'Lunch', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Quick lunch then headed home to WFH for the afternoon.', '["food", "lunch"]', '["place_demo_office"]',
    NULL, 'NEW', 71
);

-- E94: Bike commute home (midday)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0094', 'day_2025-12-04',
    '2025-12-04T18:00:00Z', '2025-12-04T18:30:00Z',
    'Bike commute', NULL, '["location_visit"]',
    0, 1, 0, 0, 0,
    'Biked home midday to work from home the rest of the day.', '["commute", "cycling"]', '[]',
    NULL, 'NEW', 135
);

-- E95: WFH afternoon
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0095', 'day_2025-12-04',
    '2025-12-04T18:30:00Z', '2025-12-04T22:00:00Z',
    'WFH afternoon', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Worked from home on the couch, finished up some Figma annotations.', '["work", "focus", "figma", "deep-work"]', '["place_demo_home", "org_demo_employer"]',
    NULL, 'NEW', 68
);

-- E96: Evening walk
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0096', 'day_2025-12-04',
    '2025-12-04T22:00:00Z', '2025-12-04T22:45:00Z',
    'Evening walk', 'Mueller Trails', '["steps"]',
    0, 0, 0, 0, 0,
    'Went for a walk around Mueller to get some fresh air after WFH.', '["exercise", "outdoors"]', '["place_demo_mueller_trails"]',
    NULL, 'NEW', 146
);

-- E97: Evening
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0097', 'day_2025-12-04',
    '2025-12-04T22:45:00Z', '2025-12-05T04:00:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Cooked stir-fry, caught up on some browsing, bed around 10.', '["food", "leisure", "browsing", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 64
);

-- ── Friday, December 5, 2025 (NO game night this week, quieter Friday) ────

-- E98: Sleep
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0098', 'day_2025-12-05',
    '2025-12-05T04:00:00Z', '2025-12-05T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Slept 6.5 hours.', '["sleep"]', '[]',
    NULL, 'NEW', 58
);

-- E99: Morning routine
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0099', 'day_2025-12-05',
    '2025-12-05T12:30:00Z', '2025-12-05T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'TGIF morning, coffee and messages.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
    NULL, 'NEW', 64
);

-- E100: Bike commute
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0100', 'day_2025-12-05',
    '2025-12-05T13:15:00Z', '2025-12-05T13:45:00Z',
    'Bike commute', NULL, '["location_visit"]',
    0, 1, 0, 0, 0,
    'Biked to the office.', '["commute", "cycling", "morning"]', '[]',
    NULL, 'NEW', 111
);

-- E101: Coffee and Slack
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0101', 'day_2025-12-05',
    '2025-12-05T13:45:00Z', '2025-12-05T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    0, 0, 0, 0, 0,
    'Friday coffee, lighter Slack traffic.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 66
);

-- E102: Standup
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0102', 'day_2025-12-05',
    '2025-12-05T14:15:00Z', '2025-12-05T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    0, 0, 0, 0, 0,
    'Friday standup, short and sweet, wrapping up the week.', '["meeting", "standup"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 71
);

-- E103: Focused work
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0103', 'day_2025-12-05',
    '2025-12-05T14:45:00Z', '2025-12-05T17:30:00Z',
    'Focused work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Wrapped up the week''s design tasks and organized files.', '["work", "focus", "design"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 71
);

-- E104: Lunch (solo)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0104', 'day_2025-12-05',
    '2025-12-05T17:30:00Z', '2025-12-05T18:15:00Z',
    'Lunch', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Grabbed a sandwich from the deli next door.', '["food", "lunch"]', '["place_demo_office"]',
    NULL, 'NEW', 66
);

-- E105: Short afternoon
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0105', 'day_2025-12-05',
    '2025-12-05T18:15:00Z', '2025-12-05T21:00:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Tied up loose ends and left a bit early for the weekend.', '["work", "design"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 67
);

-- E106: Bike commute home (early)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0106', 'day_2025-12-05',
    '2025-12-05T21:00:00Z', '2025-12-05T21:30:00Z',
    'Bike commute', NULL, '["location_visit"]',
    0, 1, 0, 0, 0,
    'Biked home early, nice to have the extra evening time.', '["commute", "cycling"]', '[]',
    NULL, 'NEW', 131
);

-- E107: Quiet Friday evening (no game night)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0107', 'day_2025-12-05',
    '2025-12-05T21:30:00Z', '2025-12-06T04:30:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Quiet Friday night in — cooked a proper dinner, watched a movie, texted Jess about next week.', '["food", "leisure", "messaging", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 60
);

-- ── Saturday, December 6, 2025 ─────────────────────────────────────────────

-- E108: Sleep
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0108', 'day_2025-12-06',
    '2025-12-06T04:30:00Z', '2025-12-06T14:00:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Slept in until about 8am, felt rested.', '["sleep"]', '[]',
    NULL, 'NEW', 55
);

-- E109: Slow morning
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0109', 'day_2025-12-06',
    '2025-12-06T14:00:00Z', '2025-12-06T15:30:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Saturday morning, made pancakes and read.', '["routine", "morning", "coffee", "food", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 66
);

-- E110: Lady Bird Lake walk
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0110', 'day_2025-12-06',
    '2025-12-06T15:30:00Z', '2025-12-06T17:00:00Z',
    'Walk at Lady Bird Lake', 'Lady Bird Lake', '["steps", "location_visit"]',
    0, 0, 0, 0, 0,
    'Long walk around Lady Bird Lake, gorgeous December morning.', '["exercise", "outdoors"]', '["place_demo_ladybird"]',
    NULL, 'NEW', 88
);

-- E111: Errands
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0111', 'day_2025-12-06',
    '2025-12-06T17:00:00Z', '2025-12-06T19:00:00Z',
    'Errands', NULL, '["location_visit"]',
    0, 0, 0, 0, 0,
    'Grocery shopping and picked up a few things at Target.', '["food", "errands"]', '[]',
    NULL, 'NEW', 81
);

-- E112: Afternoon at home
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0112', 'day_2025-12-06',
    '2025-12-06T19:00:00Z', '2025-12-06T23:00:00Z',
    'Afternoon at home', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Relaxed afternoon, did some reading and online browsing.', '["leisure", "browsing", "reading"]', '["place_demo_home"]',
    NULL, 'NEW', 64
);

-- E113: Evening (movie night solo)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0113', 'day_2025-12-06',
    '2025-12-06T23:00:00Z', '2025-12-07T04:30:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Made dinner, watched a movie, quiet Saturday night.', '["food", "leisure", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 67
);

-- ── Sunday, December 7, 2025 (Mom call this weekend) ───────────────────────

-- E114: Sleep
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0114', 'day_2025-12-07',
    '2025-12-07T04:30:00Z', '2025-12-07T14:00:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Slept in on Sunday, about 7.5 hours.', '["sleep"]', '[]',
    NULL, 'NEW', 60
);

-- E115: Slow morning
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0115', 'day_2025-12-07',
    '2025-12-07T14:00:00Z', '2025-12-07T15:30:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Lazy Sunday morning with coffee and the paper.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
    NULL, 'NEW', 68
);

-- E116: Mueller run
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0116', 'day_2025-12-07',
    '2025-12-07T15:30:00Z', '2025-12-07T16:15:00Z',
    'Morning run', 'Mueller Trails', '["steps", "workout"]',
    0, 0, 0, 0, 0,
    'Sunday run on Mueller trails, 3 miles, good pace.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
    NULL, 'NEW', 65
);

-- E117: Afternoon — reading and cooking
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0117', 'day_2025-12-07',
    '2025-12-07T16:15:00Z', '2025-12-07T20:00:00Z',
    'Afternoon at home', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Spent the afternoon reading and doing meal prep for the week.', '["leisure", "food", "reading", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 65
);

-- E118: Mom call
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0118', 'day_2025-12-07',
    '2025-12-07T20:00:00Z', '2025-12-07T20:40:00Z',
    'Phone call with Mom', 'Home', '["transcription"]',
    0, 0, 0, 0, 0,
    'Weekly call with Mom, talked about her week and Christmas plans.', '["family", "phone-call", "reflection"]', '["person_demo_mom", "place_demo_home"]',
    NULL, 'NEW', 71
);

-- E119: Evening
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0119', 'day_2025-12-07',
    '2025-12-07T20:40:00Z', '2025-12-08T04:00:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Prepped bag for tomorrow, early night.', '["routine", "leisure", "reflection"]', '["place_demo_home"]',
    NULL, 'NEW', 64
);

-- ═══════════════════════════════════════════════════════════════════════════
-- WEEK 3: Dec 8 (Mon) – Dec 14 (Sun)
-- ═══════════════════════════════════════════════════════════════════════════

-- ── Monday, December 8, 2025 ───────────────────────────────────────────────

-- E120: Sleep
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0120', 'day_2025-12-08',
    '2025-12-08T04:00:00Z', '2025-12-08T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Slept 6.5 hours, alarm at 6:30.', '["sleep"]', '[]',
    NULL, 'NEW', 55
);

-- E121: Morning routine
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0121', 'day_2025-12-08',
    '2025-12-08T12:30:00Z', '2025-12-08T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Monday morning, coffee and catching up on weekend Slack.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
    NULL, 'NEW', 68
);

-- E122: Bike commute
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0122', 'day_2025-12-08',
    '2025-12-08T13:15:00Z', '2025-12-08T13:45:00Z',
    'Bike commute', NULL, '["location_visit"]',
    0, 1, 0, 0, 0,
    'Biked to the office, foggy morning.', '["commute", "cycling", "morning"]', '[]',
    NULL, 'NEW', 118
);

-- E123: Coffee and Slack
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0123', 'day_2025-12-08',
    '2025-12-08T13:45:00Z', '2025-12-08T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    0, 0, 0, 0, 0,
    'Monday coffee at the office, reading through weekend messages.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 71
);

-- E124: Standup
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0124', 'day_2025-12-08',
    '2025-12-08T14:15:00Z', '2025-12-08T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    0, 0, 0, 0, 0,
    'Monday standup with Maya and David, planning the sprint.', '["meeting", "standup", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 77
);

-- E125: Focused work (long block)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0125', 'day_2025-12-08',
    '2025-12-08T14:45:00Z', '2025-12-08T17:30:00Z',
    'Focused design work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Long focus block on the dashboard data viz redesign.', '["design", "figma", "deep-work", "focus"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 64
);

-- E126: Lunch (solo)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0126', 'day_2025-12-08',
    '2025-12-08T17:30:00Z', '2025-12-08T18:15:00Z',
    'Lunch', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Ate my meal prep at the office, chatted with a couple coworkers.', '["food", "lunch", "social"]', '["place_demo_office"]',
    NULL, 'NEW', 69
);

-- E127: Afternoon work
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0127', 'day_2025-12-08',
    '2025-12-08T18:15:00Z', '2025-12-08T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage", "message"]',
    0, 0, 0, 0, 0,
    'Afternoon meetings and async design feedback, worked with David on the chart components.', '["work", "design", "figma"]', '["person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 68
);

-- E128: Bike commute home
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0128', 'day_2025-12-08',
    '2025-12-08T22:30:00Z', '2025-12-08T23:00:00Z',
    'Bike commute', NULL, '["location_visit"]',
    0, 1, 0, 0, 0,
    'Biked home, bundled up.', '["commute", "cycling"]', '[]',
    NULL, 'NEW', 122
);

-- E129: Evening
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0129', 'day_2025-12-08',
    '2025-12-08T23:00:00Z', '2025-12-09T04:00:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Made soup for dinner, read a chapter of my book, early night.', '["food", "leisure", "cooking", "reading"]', '["place_demo_home"]',
    NULL, 'NEW', 62
);

-- ── Tuesday, December 9, 2025 ──────────────────────────────────────────────

-- E130: Sleep
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0130', 'day_2025-12-09',
    '2025-12-09T04:00:00Z', '2025-12-09T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Slept well, about 6.5 hours.', '["sleep"]', '[]',
    NULL, 'NEW', 58
);

-- E131: Morning routine
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0131', 'day_2025-12-09',
    '2025-12-09T12:30:00Z', '2025-12-09T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Morning coffee and quick browse through email.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
    NULL, 'NEW', 65
);

-- E132: Bike commute
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0132', 'day_2025-12-09',
    '2025-12-09T13:15:00Z', '2025-12-09T13:45:00Z',
    'Bike commute', NULL, '["location_visit"]',
    0, 1, 0, 0, 0,
    'Biked to the office, clear and cold.', '["commute", "cycling", "morning"]', '[]',
    NULL, 'NEW', 113
);

-- E133: Coffee and Slack
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0133', 'day_2025-12-09',
    '2025-12-09T13:45:00Z', '2025-12-09T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    0, 0, 0, 0, 0,
    'Got coffee and checked in on Slack, a few bugs reported in the latest build.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 68
);

-- E134: Standup
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0134', 'day_2025-12-09',
    '2025-12-09T14:15:00Z', '2025-12-09T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    0, 0, 0, 0, 0,
    'Standup with Maya and David, discussed the data viz sprint.', '["meeting", "standup", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 77
);

-- E135: Design review with David
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0135', 'day_2025-12-09',
    '2025-12-09T15:00:00Z', '2025-12-09T16:00:00Z',
    'Design review', 'Office', '["calendar", "message"]',
    0, 0, 0, 0, 0,
    'Reviewed the chart component specs with David, discussed edge cases.', '["meeting", "design-review", "design"]', '["person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 76
);

-- E136: Focused work
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0136', 'day_2025-12-09',
    '2025-12-09T16:00:00Z', '2025-12-09T17:30:00Z',
    'Focused work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Iterated on the chart designs based on David''s feedback.', '["design", "figma", "focus"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 69
);

-- E137: Lunch (solo)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0137', 'day_2025-12-09',
    '2025-12-09T17:30:00Z', '2025-12-09T18:15:00Z',
    'Lunch', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Lunch at desk, scrolled through design Twitter.', '["food", "lunch", "browsing"]', '["place_demo_office"]',
    NULL, 'NEW', 66
);

-- E138: Afternoon work
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0138', 'day_2025-12-09',
    '2025-12-09T18:15:00Z', '2025-12-09T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Kept working on the charts, got into a good flow state.', '["design", "figma", "deep-work", "focus"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 68
);

-- E139: Bike commute home
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0139', 'day_2025-12-09',
    '2025-12-09T22:30:00Z', '2025-12-09T23:00:00Z',
    'Bike commute', NULL, '["location_visit"]',
    0, 1, 0, 0, 0,
    'Biked home.', '["commute", "cycling"]', '[]',
    NULL, 'NEW', 126
);

-- E140: Evening run (Tuesday)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0140', 'day_2025-12-09',
    '2025-12-09T23:15:00Z', '2025-12-10T00:00:00Z',
    'Evening run', 'Mueller Trails', '["steps", "workout"]',
    0, 0, 0, 0, 0,
    'Ran 4 miles on Mueller trails, pushed a little harder this week.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
    NULL, 'NEW', 146
);

-- E141: Evening at home
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0141', 'day_2025-12-09',
    '2025-12-10T00:00:00Z', '2025-12-10T04:00:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Quick dinner, shower, read for a bit.', '["food", "leisure", "reading"]', '["place_demo_home"]',
    NULL, 'NEW', 61
);

-- ── Wednesday, December 10, 2025 ───────────────────────────────────────────

-- E142: Sleep
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0142', 'day_2025-12-10',
    '2025-12-10T04:00:00Z', '2025-12-10T12:45:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'About 6.75 hours sleep.', '["sleep"]', '[]',
    NULL, 'NEW', 60
);

-- E143: Morning routine
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0143', 'day_2025-12-10',
    '2025-12-10T12:45:00Z', '2025-12-10T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Coffee and morning routine.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
    NULL, 'NEW', 65
);

-- E144: Bike commute
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0144', 'day_2025-12-10',
    '2025-12-10T13:15:00Z', '2025-12-10T13:45:00Z',
    'Bike commute', NULL, '["location_visit"]',
    0, 1, 0, 0, 0,
    'Biked to the office, misty morning.', '["commute", "cycling", "morning"]', '[]',
    NULL, 'NEW', 123
);

-- E145: Coffee and Slack
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0145', 'day_2025-12-10',
    '2025-12-10T13:45:00Z', '2025-12-10T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    0, 0, 0, 0, 0,
    'Office coffee, lots of Slack threads to catch up on.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 68
);

-- E146: Standup
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0146', 'day_2025-12-10',
    '2025-12-10T14:15:00Z', '2025-12-10T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    0, 0, 0, 0, 0,
    'Standup with Maya and David, midweek check-in.', '["meeting", "standup", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 72
);

-- E147: Focused work
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0147', 'day_2025-12-10',
    '2025-12-10T14:45:00Z', '2025-12-10T17:30:00Z',
    'Focused work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Heads down on the dashboard charts, working through the responsive breakpoints.', '["design", "figma", "deep-work", "focus"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 70
);

-- E148: Lunch with Maya at Tatsu-ya
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0148', 'day_2025-12-10',
    '2025-12-10T17:30:00Z', '2025-12-10T18:30:00Z',
    'Lunch at Ramen Tatsu-ya', 'Ramen Tatsu-ya', '["location_visit"]',
    0, 0, 0, 0, 0,
    'Wednesday ramen with Maya, she was excited about a new hire starting next month.', '["food", "social", "ramen"]', '["person_demo_maya", "place_demo_ramen"]',
    NULL, 'NEW', 71
);

-- E149: Afternoon work
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0149', 'day_2025-12-10',
    '2025-12-10T18:30:00Z', '2025-12-10T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Finished the responsive chart mockups and shared for review.', '["work", "design", "figma"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 70
);

-- E150: Bike commute home
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0150', 'day_2025-12-10',
    '2025-12-10T22:30:00Z', '2025-12-10T23:00:00Z',
    'Bike commute', NULL, '["location_visit"]',
    0, 1, 0, 0, 0,
    'Biked home.', '["commute", "cycling"]', '[]',
    NULL, 'NEW', 113
);

-- E151: Evening at home
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0151', 'day_2025-12-10',
    '2025-12-10T23:00:00Z', '2025-12-11T04:00:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Made a curry for dinner, watched TV, browsed apartment decor ideas.', '["food", "leisure", "browsing", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 61
);

-- ── Thursday, December 11, 2025 ────────────────────────────────────────────

-- E152: Sleep
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0152', 'day_2025-12-11',
    '2025-12-11T04:00:00Z', '2025-12-11T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Slept 6.5 hours.', '["sleep"]', '[]',
    NULL, 'NEW', 58
);

-- E153: Morning routine
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0153', 'day_2025-12-11',
    '2025-12-11T12:30:00Z', '2025-12-11T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Coffee and messages, chilly morning.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
    NULL, 'NEW', 64
);

-- E154: Bike commute
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0154', 'day_2025-12-11',
    '2025-12-11T13:15:00Z', '2025-12-11T13:45:00Z',
    'Bike commute', NULL, '["location_visit"]',
    0, 1, 0, 0, 0,
    'Biked to the office.', '["commute", "cycling", "morning"]', '[]',
    NULL, 'NEW', 129
);

-- E155: Coffee and Slack
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0155', 'day_2025-12-11',
    '2025-12-11T13:45:00Z', '2025-12-11T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    0, 0, 0, 0, 0,
    'Coffee and checking Slack, David shared some chart edge cases.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 65
);

-- E156: Standup
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0156', 'day_2025-12-11',
    '2025-12-11T14:15:00Z', '2025-12-11T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    0, 0, 0, 0, 0,
    'Thursday standup with Maya and David.', '["meeting", "standup", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 75
);

-- E157: Focused work
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0157', 'day_2025-12-11',
    '2025-12-11T14:45:00Z', '2025-12-11T17:30:00Z',
    'Focused work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Worked on the edge case charts David flagged, tricky empty-state designs.', '["design", "figma", "deep-work", "focus"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 67
);

-- E158: Lunch (solo)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0158', 'day_2025-12-11',
    '2025-12-11T17:30:00Z', '2025-12-11T18:15:00Z',
    'Lunch', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Grabbed a burrito from the taco truck outside.', '["food", "lunch"]', '["place_demo_office"]',
    NULL, 'NEW', 69
);

-- E159: Afternoon work
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0159', 'day_2025-12-11',
    '2025-12-11T18:15:00Z', '2025-12-11T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Finished the empty-state designs and started on the loading skeleton patterns.', '["work", "design", "figma"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 66
);

-- E160: Bike commute home
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0160', 'day_2025-12-11',
    '2025-12-11T22:30:00Z', '2025-12-11T23:00:00Z',
    'Bike commute', NULL, '["location_visit"]',
    0, 1, 0, 0, 0,
    'Biked home.', '["commute", "cycling"]', '[]',
    NULL, 'NEW', 111
);

-- E161: Evening walk and dinner
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0161', 'day_2025-12-11',
    '2025-12-11T23:00:00Z', '2025-12-12T00:00:00Z',
    'Evening walk', 'Mueller Trails', '["steps"]',
    0, 0, 0, 0, 0,
    'Walked around Mueller, the holiday lights were going up on the houses.', '["exercise", "outdoors"]', '["place_demo_mueller_trails"]',
    NULL, 'NEW', 155
);

-- E162: Evening
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0162', 'day_2025-12-11',
    '2025-12-12T00:00:00Z', '2025-12-12T04:00:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Dinner and some reading, thinking about Christmas gifts.', '["food", "leisure", "reading", "reflection"]', '["place_demo_home"]',
    NULL, 'NEW', 65
);

-- ── Friday, December 12, 2025 (Game night at Jess's) ──────────────────────

-- E163: Sleep
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0163', 'day_2025-12-12',
    '2025-12-12T04:00:00Z', '2025-12-12T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'About 6.5 hours sleep.', '["sleep"]', '[]',
    NULL, 'NEW', 55
);

-- E164: Morning routine
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0164', 'day_2025-12-12',
    '2025-12-12T12:30:00Z', '2025-12-12T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Friday morning, coffee and checking Slack, excited for game night tonight.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
    NULL, 'NEW', 66
);

-- E165: Bike commute
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0165', 'day_2025-12-12',
    '2025-12-12T13:15:00Z', '2025-12-12T13:45:00Z',
    'Bike commute', NULL, '["location_visit"]',
    0, 1, 0, 0, 0,
    'Biked to the office.', '["commute", "cycling", "morning"]', '[]',
    NULL, 'NEW', 111
);

-- E166: Coffee and Slack
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0166', 'day_2025-12-12',
    '2025-12-12T13:45:00Z', '2025-12-12T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    0, 0, 0, 0, 0,
    'Friday coffee, looking forward to the weekend.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 72
);

-- E167: Standup
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0167', 'day_2025-12-12',
    '2025-12-12T14:15:00Z', '2025-12-12T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    0, 0, 0, 0, 0,
    'Friday standup, wrapped up the week''s work items.', '["meeting", "standup"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 73
);

-- E168: Focused work
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0168', 'day_2025-12-12',
    '2025-12-12T14:45:00Z', '2025-12-12T17:30:00Z',
    'Focused work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Cleaned up the Figma files and organized the design system components.', '["work", "design", "figma"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 67
);

-- E169: Lunch (with Maya, casual)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0169', 'day_2025-12-12',
    '2025-12-12T17:30:00Z', '2025-12-12T18:15:00Z',
    'Lunch', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Ate lunch with Maya in the break room, chatted about weekend plans.', '["food", "social", "lunch"]', '["person_demo_maya", "place_demo_office"]',
    NULL, 'NEW', 71
);

-- E170: Short afternoon
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0170', 'day_2025-12-12',
    '2025-12-12T18:15:00Z', '2025-12-12T21:00:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Quick afternoon wrapping things up, left early for game night.', '["work", "design"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 71
);

-- E171: Bike commute home
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0171', 'day_2025-12-12',
    '2025-12-12T21:00:00Z', '2025-12-12T21:30:00Z',
    'Bike commute', NULL, '["location_visit"]',
    0, 1, 0, 0, 0,
    'Biked home to change before heading to Jess''s.', '["commute", "cycling"]', '[]',
    NULL, 'NEW', 128
);

-- E172: Quick break at home
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0172', 'day_2025-12-12',
    '2025-12-12T21:30:00Z', '2025-12-13T00:00:00Z',
    'Home break', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Changed clothes, grabbed some snacks to bring to game night.', '["routine"]', '["place_demo_home"]',
    NULL, 'NEW', 71
);

-- E173: Game night at Jess's
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0173', 'day_2025-12-12',
    '2025-12-13T00:00:00Z', '2025-12-13T04:30:00Z',
    'Game night', 'Jess''s Place', '["location_visit"]',
    0, 0, 0, 0, 0,
    'Game night at Jess''s with Priya — played Ticket to Ride and Wavelength, great time.', '["social", "games"]', '["person_demo_jess", "person_demo_priya", "place_demo_jess"]',
    NULL, 'NEW', 74
);

-- ── Saturday, December 13, 2025 ────────────────────────────────────────────

-- E174: Sleep
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0174', 'day_2025-12-13',
    '2025-12-13T04:30:00Z', '2025-12-13T14:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Slept in after game night, about 8 hours.', '["sleep"]', '[]',
    NULL, 'NEW', 56
);

-- E175: Slow morning
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0175', 'day_2025-12-13',
    '2025-12-13T14:30:00Z', '2025-12-13T16:00:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Slow Saturday morning, made waffles, scrolled through the internet.', '["routine", "morning", "coffee", "food", "browsing", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 66
);

-- E176: Lady Bird Lake walk
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0176', 'day_2025-12-13',
    '2025-12-13T16:00:00Z', '2025-12-13T17:30:00Z',
    'Walk at Lady Bird Lake', 'Lady Bird Lake', '["steps", "location_visit"]',
    0, 0, 0, 0, 0,
    'Walked the Lady Bird Lake trail, the water was really still today.', '["exercise", "outdoors"]', '["place_demo_ladybird"]',
    NULL, 'NEW', 93
);

-- E177: Jo's Coffee
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0177', 'day_2025-12-13',
    '2025-12-13T17:30:00Z', '2025-12-13T19:00:00Z',
    'Coffee at Jo''s', 'Jo''s Coffee', '["location_visit"]',
    0, 0, 0, 0, 0,
    'Stopped at Jo''s on South Congress for a latte, read for a while.', '["coffee", "leisure"]', '["place_demo_jos"]',
    NULL, 'NEW', 72
);

-- E178: Afternoon at home
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0178', 'day_2025-12-13',
    '2025-12-13T19:00:00Z', '2025-12-13T23:00:00Z',
    'Afternoon at home', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Relaxed at home, did some online Christmas shopping.', '["leisure", "browsing", "shopping"]', '["place_demo_home"]',
    NULL, 'NEW', 71
);

-- E179: Evening
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0179', 'day_2025-12-13',
    '2025-12-13T23:00:00Z', '2025-12-14T04:30:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Cooked pasta, watched a holiday movie, early-ish night.', '["food", "leisure", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 67
);

-- ── Sunday, December 14, 2025 ──────────────────────────────────────────────

-- E180: Sleep
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0180', 'day_2025-12-14',
    '2025-12-14T04:30:00Z', '2025-12-14T14:00:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0, 1, 0, 0,
    'Slept well, about 7.5 hours.', '["sleep"]', '[]',
    NULL, 'NEW', 62
);

-- E181: Slow morning
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0181', 'day_2025-12-14',
    '2025-12-14T14:00:00Z', '2025-12-14T15:30:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Sunday morning, big brunch with eggs and toast, read the news.', '["routine", "morning", "coffee", "food", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 66
);

-- E182: Mueller run
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0182', 'day_2025-12-14',
    '2025-12-14T15:30:00Z', '2025-12-14T16:15:00Z',
    'Morning run', 'Mueller Trails', '["steps", "workout"]',
    0, 0, 0, 0, 0,
    'Sunday run on Mueller, 3 miles at an easy pace.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
    NULL, 'NEW', 65
);

-- E183: Afternoon — meal prep and reading
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0183', 'day_2025-12-14',
    '2025-12-14T16:15:00Z', '2025-12-14T21:00:00Z',
    'Afternoon at home', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Meal prepped for the week, did laundry, read a couple chapters.', '["food", "leisure", "cooking", "reading"]', '["place_demo_home"]',
    NULL, 'NEW', 70
);

-- E184: Evening
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0184', 'day_2025-12-14',
    '2025-12-14T21:00:00Z', '2025-12-15T04:00:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    0, 0, 0, 0, 0,
    'Prepped bag for the week, caught up on a podcast, early night.', '["routine", "leisure", "podcast"]', '["place_demo_home"]',
    NULL, 'NEW', 65
);
