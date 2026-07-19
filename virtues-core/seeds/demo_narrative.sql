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
-- Usage: psql "$DATABASE_URL" -f core/seeds/demo_narrative.sql
-- =============================================================================

-- ─────────────────────────────────────────────────────────────────────────────
-- 0. IDEMPOTENT CLEANUP
-- ─────────────────────────────────────────────────────────────────────────────
DELETE FROM wiki_events WHERE id LIKE 'ev_b0%' AND CAST(SUBSTR(id, 5) AS INTEGER) BETWEEN 1 AND 210;

-- ─────────────────────────────────────────────────────────────────────────────
-- 1. WIKI DAYS
-- ─────────────────────────────────────────────────────────────────────────────
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-11-24', '2025-11-24', 'America/Chicago', 0.48) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-11-25', '2025-11-25', 'America/Chicago', 0.52) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-11-26', '2025-11-26', 'America/Chicago', 0.50) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-11-27', '2025-11-27', 'America/Chicago', 0.55) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-11-28', '2025-11-28', 'America/Chicago', 0.45) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-11-29', '2025-11-29', 'America/Chicago', 0.50) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-11-30', '2025-11-30', 'America/Chicago', 0.47) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-12-01', '2025-12-01', 'America/Chicago', 0.50) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-12-02', '2025-12-02', 'America/Chicago', 0.53) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-12-03', '2025-12-03', 'America/Chicago', 0.48) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-12-04', '2025-12-04', 'America/Chicago', 0.51) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-12-05', '2025-12-05', 'America/Chicago', 0.46) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-12-06', '2025-12-06', 'America/Chicago', 0.54) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-12-07', '2025-12-07', 'America/Chicago', 0.49) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-12-08', '2025-12-08', 'America/Chicago', 0.52) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-12-09', '2025-12-09', 'America/Chicago', 0.50) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-12-10', '2025-12-10', 'America/Chicago', 0.44) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-12-11', '2025-12-11', 'America/Chicago', 0.55) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-12-12', '2025-12-12', 'America/Chicago', 0.48) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-12-13', '2025-12-13', 'America/Chicago', 0.52) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-12-14', '2025-12-14', 'America/Chicago', 0.50) ON CONFLICT DO NOTHING;

-- ─────────────────────────────────────────────────────────────────────────────
-- 2. WIKI EVENTS
-- ─────────────────────────────────────────────────────────────────────────────

-- ═══════════════════════════════════════════════════════════════════════════
-- WEEK 1: Nov 24 (Mon) – Nov 30 (Sun)
-- ═══════════════════════════════════════════════════════════════════════════

-- ── Monday, November 24, 2025 ──────────────────────────────────────────────

-- E01: Sleep (midnight-6:30 CST = 06:00-12:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0001', 'day_2025-11-24',
    '2025-11-24T06:00:00Z', '2025-11-24T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Slept about 6.5 hours, woke up at 6:30am.', '["sleep"]', '[]',
    NULL, 'NEW', 62
) ON CONFLICT DO NOTHING;

-- E02: Morning routine (06:30-07:15 CST = 12:30-13:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0002', 'day_2025-11-24',
    '2025-11-24T12:30:00Z', '2025-11-24T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Coffee and scrolling through messages at home.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
    NULL, 'NEW', 68
) ON CONFLICT DO NOTHING;

-- E03: Bike commute (07:15-07:45 CST = 13:15-13:45 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0003', 'day_2025-11-24',
    '2025-11-24T13:15:00Z', '2025-11-24T13:45:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked to the office, chilly morning.', '["commute", "cycling", "morning"]', '[]',
    NULL, 'NEW', 130
) ON CONFLICT DO NOTHING;

-- E04: Coffee and Slack (07:45-08:15 CST = 13:45-14:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0004', 'day_2025-11-24',
    '2025-11-24T13:45:00Z', '2025-11-24T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Settled in at the office with coffee, catching up on Slack.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 64
) ON CONFLICT DO NOTHING;

-- E05: Design standup (08:15-08:45 CST = 14:15-14:45 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0005', 'day_2025-11-24',
    '2025-11-24T14:15:00Z', '2025-11-24T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Monday standup with Maya and David, planning the week.', '["meeting", "standup", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 77
) ON CONFLICT DO NOTHING;

-- E06: Focused design work (08:45-11:30 CST = 14:45-17:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0006', 'day_2025-11-24',
    '2025-11-24T14:45:00Z', '2025-11-24T17:30:00Z',
    'Focused design work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Deep work in Figma on the settings page redesign.', '["design", "figma", "deep-work", "focus"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 64
) ON CONFLICT DO NOTHING;

-- E07: Solo lunch (11:30-12:15 CST = 17:30-18:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0007', 'day_2025-11-24',
    '2025-11-24T17:30:00Z', '2025-11-24T18:15:00Z',
    'Lunch', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Quick solo lunch at the office, ate at desk.', '["food", "lunch"]', '["place_demo_office"]',
    NULL, 'NEW', 70
) ON CONFLICT DO NOTHING;

-- E08: Afternoon work (12:15-16:30 CST = 18:15-22:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0008', 'day_2025-11-24',
    '2025-11-24T18:15:00Z', '2025-11-24T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Worked on wireframes and responded to design feedback on Slack.', '["work", "design", "figma"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 72
) ON CONFLICT DO NOTHING;

-- E09: Bike commute home (16:30-17:00 CST = 22:30-23:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0009', 'day_2025-11-24',
    '2025-11-24T22:30:00Z', '2025-11-24T23:00:00Z',
    'Bike commute', NULL, '["location_visit"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked home from the office.', '["commute", "cycling"]', '[]',
    NULL, 'NEW', 119
) ON CONFLICT DO NOTHING;

-- E10: Evening at home (17:00-22:00 CST = 23:00-04:00+1 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0010', 'day_2025-11-24',
    '2025-11-24T23:00:00Z', '2025-11-25T04:00:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Made pasta for dinner, watched a couple episodes of a show, read before bed.', '["food", "leisure", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 68
) ON CONFLICT DO NOTHING;

-- ── Tuesday, November 25, 2025 ─────────────────────────────────────────────

-- E11: Sleep
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0011', 'day_2025-11-25',
    '2025-11-25T04:00:00Z', '2025-11-25T12:45:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Slept about 6.75 hours, woke a bit before the alarm.', '["sleep"]', '[]',
    NULL, 'NEW', 59
) ON CONFLICT DO NOTHING;

-- E12: Morning routine
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0012', 'day_2025-11-25',
    '2025-11-25T12:45:00Z', '2025-11-25T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Quick morning routine, checked email and weather.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
    NULL, 'NEW', 67
) ON CONFLICT DO NOTHING;

-- E13: Bike commute
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0013', 'day_2025-11-25',
    '2025-11-25T13:15:00Z', '2025-11-25T13:45:00Z',
    'Bike commute', NULL, '["location_visit"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked to the office, cool and overcast.', '["commute", "cycling", "morning"]', '[]',
    NULL, 'NEW', 126
) ON CONFLICT DO NOTHING;

-- E14: Coffee and Slack
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0014', 'day_2025-11-25',
    '2025-11-25T13:45:00Z', '2025-11-25T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Coffee at the office, catching up on overnight Slack threads.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 67
) ON CONFLICT DO NOTHING;

-- E15: Standup
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0015', 'day_2025-11-25',
    '2025-11-25T14:15:00Z', '2025-11-25T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Standup with Maya and David, talked about the short week ahead of Thanksgiving.', '["meeting", "standup", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 75
) ON CONFLICT DO NOTHING;

-- E16: Design review with David (Tuesday special)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0016', 'day_2025-11-25',
    '2025-11-25T15:00:00Z', '2025-11-25T16:00:00Z',
    'Design review', 'Office', '["calendar", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Design review with David going over component library updates.', '["meeting", "design-review", "design"]', '["person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 77
) ON CONFLICT DO NOTHING;

-- E17: Focused work
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0017', 'day_2025-11-25',
    '2025-11-25T16:00:00Z', '2025-11-25T17:30:00Z',
    'Focused work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Heads-down time in Figma iterating on dashboard layouts.', '["design", "figma", "deep-work", "focus"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 68
) ON CONFLICT DO NOTHING;

-- E18: Lunch (solo)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0018', 'day_2025-11-25',
    '2025-11-25T17:30:00Z', '2025-11-25T18:15:00Z',
    'Lunch', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Grabbed a salad from the place downstairs, ate in the break room.', '["food", "lunch"]', '["place_demo_office"]',
    NULL, 'NEW', 69
) ON CONFLICT DO NOTHING;

-- E19: Afternoon work
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0019', 'day_2025-11-25',
    '2025-11-25T18:15:00Z', '2025-11-25T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Finished up the settings page mockups and pushed to the design repo.', '["work", "design", "figma"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 68
) ON CONFLICT DO NOTHING;

-- E20: Bike commute home
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0020', 'day_2025-11-25',
    '2025-11-25T22:30:00Z', '2025-11-25T23:00:00Z',
    'Bike commute', NULL, '["location_visit"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked home, sun setting early now.', '["commute", "cycling"]', '[]',
    NULL, 'NEW', 119
) ON CONFLICT DO NOTHING;

-- E21: Evening run (Tuesday = Mueller trails)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0021', 'day_2025-11-25',
    '2025-11-25T23:15:00Z', '2025-11-26T00:00:00Z',
    'Evening run', 'Mueller Trails', '["steps", "workout"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Quick 3-mile run on Mueller trails before it got dark.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
    NULL, 'NEW', 150
) ON CONFLICT DO NOTHING;

-- E22: Evening at home
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0022', 'day_2025-11-25',
    '2025-11-26T00:00:00Z', '2025-11-26T04:00:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Showered, made stir-fry, browsed the internet for a while.', '["food", "leisure", "browsing", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 63
) ON CONFLICT DO NOTHING;

-- ── Wednesday, November 26, 2025 (day before Thanksgiving) ─────────────────

-- E23: Sleep
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0023', 'day_2025-11-26',
    '2025-11-26T04:00:00Z', '2025-11-26T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Slept about 6.5 hours.', '["sleep"]', '[]',
    NULL, 'NEW', 59
) ON CONFLICT DO NOTHING;

-- E24: Morning routine
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0024', 'day_2025-11-26',
    '2025-11-26T12:30:00Z', '2025-11-26T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Morning coffee and checking texts, people already off for the holiday.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
    NULL, 'NEW', 67
) ON CONFLICT DO NOTHING;

-- E25: Bike commute
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0025', 'day_2025-11-26',
    '2025-11-26T13:15:00Z', '2025-11-26T13:45:00Z',
    'Bike commute', NULL, '["location_visit"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked to the office, half the office already on holiday.', '["commute", "cycling", "morning"]', '[]',
    NULL, 'NEW', 135
) ON CONFLICT DO NOTHING;

-- E26: Coffee and Slack
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0026', 'day_2025-11-26',
    '2025-11-26T13:45:00Z', '2025-11-26T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Quiet office, half the team out. Quick coffee.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 67
) ON CONFLICT DO NOTHING;

-- E27: Standup (short, pre-holiday)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0027', 'day_2025-11-26',
    '2025-11-26T14:15:00Z', '2025-11-26T14:30:00Z',
    'Design standup', 'Office', '["calendar"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Quick pre-holiday standup, just Maya on the call.', '["meeting", "standup"]', '["person_demo_maya", "place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 76
) ON CONFLICT DO NOTHING;

-- E28: Focused work (wrapping up before holiday)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0028', 'day_2025-11-26',
    '2025-11-26T14:30:00Z', '2025-11-26T17:30:00Z',
    'Focused work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Wrapped up loose ends before the holiday break, updated Jira tickets.', '["work", "focus", "deep-work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 71
) ON CONFLICT DO NOTHING;

-- E29: Lunch with Maya at Tatsu-ya (Wednesday = Tatsu-ya day)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0029', 'day_2025-11-26',
    '2025-11-26T17:30:00Z', '2025-11-26T18:30:00Z',
    'Lunch at Ramen Tatsu-ya', 'Ramen Tatsu-ya', '["location_visit"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Pre-Thanksgiving ramen with Maya at Tatsu-ya.', '["food", "social", "ramen"]', '["person_demo_maya", "place_demo_ramen"]',
    NULL, 'NEW', 73
) ON CONFLICT DO NOTHING;

-- E30: Short afternoon (left early)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0030', 'day_2025-11-26',
    '2025-11-26T18:30:00Z', '2025-11-26T21:00:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Came back to the office for a couple hours then headed out early.', '["work", "design"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 69
) ON CONFLICT DO NOTHING;

-- E31: Bike commute home (early)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0031', 'day_2025-11-26',
    '2025-11-26T21:00:00Z', '2025-11-26T21:30:00Z',
    'Bike commute', NULL, '["location_visit"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked home early, nice to leave in daylight for once.', '["commute", "cycling"]', '[]',
    NULL, 'NEW', 126
) ON CONFLICT DO NOTHING;

-- E32: Evening — groceries and cooking
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0032', 'day_2025-11-26',
    '2025-11-26T21:30:00Z', '2025-11-27T04:00:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Stopped for groceries, prepped some food for tomorrow, watched a movie.', '["food", "leisure", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 62
) ON CONFLICT DO NOTHING;

-- ── Thursday, November 27, 2025 (Thanksgiving) ─────────────────────────────

-- E33: Sleep (slept in)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0033', 'day_2025-11-27',
    '2025-11-27T04:00:00Z', '2025-11-27T14:00:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Slept in on Thanksgiving morning, about 8 hours.', '["sleep"]', '[]',
    NULL, 'NEW', 61
) ON CONFLICT DO NOTHING;

-- E34: Slow morning
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0034', 'day_2025-11-27',
    '2025-11-27T14:00:00Z', '2025-11-27T15:30:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Lazy Thanksgiving morning with coffee and texts from family.', '["routine", "morning", "coffee", "family"]', '["place_demo_home"]',
    NULL, 'NEW', 63
) ON CONFLICT DO NOTHING;

-- E35: Morning walk
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0035', 'day_2025-11-27',
    '2025-11-27T15:30:00Z', '2025-11-27T16:30:00Z',
    'Morning walk', 'Mueller Trails', '["steps"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Went for a walk around Mueller to clear my head, the neighborhood was quiet.', '["exercise", "outdoors"]', '["place_demo_mueller_trails"]',
    NULL, 'NEW', 67
) ON CONFLICT DO NOTHING;

-- E36: Cooking Thanksgiving meal
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0036', 'day_2025-11-27',
    '2025-11-27T16:30:00Z', '2025-11-27T20:00:00Z',
    'Cooking', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Made a small Thanksgiving dinner for one — roasted chicken, mashed potatoes, pie from the store.', '["food", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 75
) ON CONFLICT DO NOTHING;

-- E37: Phone call with Mom
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0037', 'day_2025-11-27',
    '2025-11-27T20:00:00Z', '2025-11-27T20:45:00Z',
    'Phone call with Mom', 'Home', '["transcription"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Called Mom for Thanksgiving, she was at her sister''s. Talked about her garden and holiday plans.', '["family", "phone-call"]', '["person_demo_mom", "place_demo_home"]',
    NULL, 'NEW', 66
) ON CONFLICT DO NOTHING;

-- E38: Thanksgiving evening
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0038', 'day_2025-11-27',
    '2025-11-27T20:45:00Z', '2025-11-28T04:00:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Ate Thanksgiving dinner, watched a movie, texted Jess about plans for tomorrow.', '["food", "leisure", "messaging", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 63
) ON CONFLICT DO NOTHING;

-- ── Friday, November 28, 2025 (Black Friday — game night at Jess's) ────────

-- E39: Sleep
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0039', 'day_2025-11-28',
    '2025-11-28T04:00:00Z', '2025-11-28T14:00:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Slept in, no work today.', '["sleep"]', '[]',
    NULL, 'NEW', 62
) ON CONFLICT DO NOTHING;

-- E40: Slow morning
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0040', 'day_2025-11-28',
    '2025-11-28T14:00:00Z', '2025-11-28T15:30:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Slow Black Friday morning, coffee and reading online.', '["routine", "morning", "coffee", "browsing"]', '["place_demo_home"]',
    NULL, 'NEW', 66
) ON CONFLICT DO NOTHING;

-- E41: Lady Bird Lake walk
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0041', 'day_2025-11-28',
    '2025-11-28T16:00:00Z', '2025-11-28T17:30:00Z',
    'Walk at Lady Bird Lake', 'Lady Bird Lake', '["steps", "location_visit"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Walked around Lady Bird Lake, the trail was busy with holiday runners.', '["exercise", "outdoors"]', '["place_demo_ladybird"]',
    NULL, 'NEW', 92
) ON CONFLICT DO NOTHING;

-- E42: Afternoon at home
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0042', 'day_2025-11-28',
    '2025-11-28T17:30:00Z', '2025-11-28T23:00:00Z',
    'Afternoon at home', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Leftover Thanksgiving food for lunch, read a book, did some online browsing.', '["food", "leisure", "browsing", "reading"]', '["place_demo_home"]',
    NULL, 'NEW', 67
) ON CONFLICT DO NOTHING;

-- E43: Game night at Jess's
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0043', 'day_2025-11-28',
    '2025-11-29T00:00:00Z', '2025-11-29T04:00:00Z',
    'Game night', 'Jess''s Place', '["location_visit"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Game night at Jess''s with Priya — played Catan and Codenames, ate leftover pie.', '["social", "games"]', '["person_demo_jess", "person_demo_priya", "place_demo_jess"]',
    NULL, 'NEW', 71
) ON CONFLICT DO NOTHING;

-- ── Saturday, November 29, 2025 ────────────────────────────────────────────

-- E44: Sleep
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0044', 'day_2025-11-29',
    '2025-11-29T04:00:00Z', '2025-11-29T13:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Slept in until about 7:30am after game night.', '["sleep"]', '[]',
    NULL, 'NEW', 56
) ON CONFLICT DO NOTHING;

-- E45: Slow morning
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0045', 'day_2025-11-29',
    '2025-11-29T13:30:00Z', '2025-11-29T15:00:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Slow Saturday morning, coffee and podcasts.', '["routine", "morning", "coffee", "podcast"]', '["place_demo_home"]',
    NULL, 'NEW', 65
) ON CONFLICT DO NOTHING;

-- E46: Lady Bird Lake walk
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0046', 'day_2025-11-29',
    '2025-11-29T15:00:00Z', '2025-11-29T16:30:00Z',
    'Walk at Lady Bird Lake', 'Lady Bird Lake', '["steps", "location_visit"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Nice walk around the lake, warm for late November.', '["exercise", "outdoors"]', '["place_demo_ladybird"]',
    NULL, 'NEW', 88
) ON CONFLICT DO NOTHING;

-- E47: Errands and afternoon
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0047', 'day_2025-11-29',
    '2025-11-29T16:30:00Z', '2025-11-29T21:00:00Z',
    'Errands and reading', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Ran some errands, then spent the afternoon reading at home.', '["leisure", "reading"]', '["place_demo_home"]',
    NULL, 'NEW', 65
) ON CONFLICT DO NOTHING;

-- E48: Mom call (Saturday this week since Thanksgiving was Thursday)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0048', 'day_2025-11-29',
    '2025-11-29T21:00:00Z', '2025-11-29T21:30:00Z',
    'Phone call with Mom', 'Home', '["transcription"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Quick follow-up call with Mom, she wanted the pie recipe I used.', '["family", "phone-call"]', '["person_demo_mom", "place_demo_home"]',
    NULL, 'NEW', 65
) ON CONFLICT DO NOTHING;

-- E49: Evening
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0049', 'day_2025-11-29',
    '2025-11-29T21:30:00Z', '2025-11-30T04:30:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Cooked dinner, watched a documentary, early night.', '["food", "leisure", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 67
) ON CONFLICT DO NOTHING;

-- ── Sunday, November 30, 2025 ──────────────────────────────────────────────

-- E50: Sleep
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0050', 'day_2025-11-30',
    '2025-11-30T04:30:00Z', '2025-11-30T14:00:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Slept well, about 7.5 hours.', '["sleep"]', '[]',
    NULL, 'NEW', 60
) ON CONFLICT DO NOTHING;

-- E51: Slow morning
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0051', 'day_2025-11-30',
    '2025-11-30T14:00:00Z', '2025-11-30T15:30:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Sunday morning, made a big breakfast and read the news.', '["routine", "morning", "coffee", "food"]', '["place_demo_home"]',
    NULL, 'NEW', 63
) ON CONFLICT DO NOTHING;

-- E52: Mueller run
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0052', 'day_2025-11-30',
    '2025-11-30T15:30:00Z', '2025-11-30T16:15:00Z',
    'Morning run', 'Mueller Trails', '["steps", "workout"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Short run around Mueller, legs were a bit tired.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
    NULL, 'NEW', 67
) ON CONFLICT DO NOTHING;

-- E53: Afternoon reading and meal prep
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0053', 'day_2025-11-30',
    '2025-11-30T16:15:00Z', '2025-11-30T21:00:00Z',
    'Afternoon at home', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Meal prepped for the week, did some reading, reorganized the bookshelf.', '["food", "leisure", "reading", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 64
) ON CONFLICT DO NOTHING;

-- E54: Evening
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0054', 'day_2025-11-30',
    '2025-11-30T21:00:00Z', '2025-12-01T04:00:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Quiet Sunday evening, prepped bag for tomorrow, caught up on a podcast.', '["leisure", "routine", "podcast"]', '["place_demo_home"]',
    NULL, 'NEW', 64
) ON CONFLICT DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════════════
-- WEEK 2: Dec 1 (Mon) – Dec 7 (Sun)
-- ═══════════════════════════════════════════════════════════════════════════

-- ── Monday, December 1, 2025 ───────────────────────────────────────────────

-- E55: Sleep
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0055', 'day_2025-12-01',
    '2025-12-01T04:00:00Z', '2025-12-01T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'About 6.5 hours sleep, alarm went off at 6:30.', '["sleep"]', '[]',
    NULL, 'NEW', 59
) ON CONFLICT DO NOTHING;

-- E56: Morning routine
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0056', 'day_2025-12-01',
    '2025-12-01T12:30:00Z', '2025-12-01T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Back to the grind after the long weekend, coffee and Slack.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
    NULL, 'NEW', 63
) ON CONFLICT DO NOTHING;

-- E57: Bike commute
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0057', 'day_2025-12-01',
    '2025-12-01T13:15:00Z', '2025-12-01T13:45:00Z',
    'Bike commute', NULL, '["location_visit"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked to the office, cold December morning.', '["commute", "cycling", "morning"]', '[]',
    NULL, 'NEW', 133
) ON CONFLICT DO NOTHING;

-- E58: Coffee and Slack
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0058', 'day_2025-12-01',
    '2025-12-01T13:45:00Z', '2025-12-01T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Office coffee, catching up on the backlog from the holiday break.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 69
) ON CONFLICT DO NOTHING;

-- E59: Standup
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0059', 'day_2025-12-01',
    '2025-12-01T14:15:00Z', '2025-12-01T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Monday standup with Maya and David, recapping what got done before the break.', '["meeting", "standup", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 78
) ON CONFLICT DO NOTHING;

-- E60: Focused work (long morning block)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0060', 'day_2025-12-01',
    '2025-12-01T14:45:00Z', '2025-12-01T17:30:00Z',
    'Focused design work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Long focus block in Figma working on the notifications page.', '["design", "figma", "deep-work", "focus"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 64
) ON CONFLICT DO NOTHING;

-- E61: Lunch (solo)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0061', 'day_2025-12-01',
    '2025-12-01T17:30:00Z', '2025-12-01T18:15:00Z',
    'Lunch', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Brought leftovers from the weekend meal prep.', '["food", "lunch"]', '["place_demo_office"]',
    NULL, 'NEW', 67
) ON CONFLICT DO NOTHING;

-- E62: Afternoon meetings + work
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0062', 'day_2025-12-01',
    '2025-12-01T18:15:00Z', '2025-12-01T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Afternoon of Slack conversations and wrapping up the notifications page.', '["work", "messaging", "design"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 71
) ON CONFLICT DO NOTHING;

-- E63: Bike commute home
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0063', 'day_2025-12-01',
    '2025-12-01T22:30:00Z', '2025-12-01T23:00:00Z',
    'Bike commute', NULL, '["location_visit"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked home, dark already at 5pm.', '["commute", "cycling"]', '[]',
    NULL, 'NEW', 134
) ON CONFLICT DO NOTHING;

-- E64: Evening
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0064', 'day_2025-12-01',
    '2025-12-01T23:00:00Z', '2025-12-02T04:00:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Made tacos for dinner, watched some TV, early bedtime.', '["food", "leisure", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 61
) ON CONFLICT DO NOTHING;

-- ── Tuesday, December 2, 2025 ──────────────────────────────────────────────

-- E65: Sleep
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0065', 'day_2025-12-02',
    '2025-12-02T04:00:00Z', '2025-12-02T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Slept okay, woke up once around 4am.', '["sleep"]', '[]',
    NULL, 'NEW', 60
) ON CONFLICT DO NOTHING;

-- E66: Morning routine
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0066', 'day_2025-12-02',
    '2025-12-02T12:30:00Z', '2025-12-02T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Coffee and morning routine, checked messages.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
    NULL, 'NEW', 64
) ON CONFLICT DO NOTHING;

-- E67: Bike commute
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0067', 'day_2025-12-02',
    '2025-12-02T13:15:00Z', '2025-12-02T13:45:00Z',
    'Bike commute', NULL, '["location_visit"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked to the office, windy morning.', '["commute", "cycling", "morning"]', '[]',
    NULL, 'NEW', 130
) ON CONFLICT DO NOTHING;

-- E68: Coffee and Slack
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0068', 'day_2025-12-02',
    '2025-12-02T13:45:00Z', '2025-12-02T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Got settled in with coffee, reviewed PRs on Slack.', '["messaging", "work", "coffee", "code-review"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 72
) ON CONFLICT DO NOTHING;

-- E69: Standup
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0069', 'day_2025-12-02',
    '2025-12-02T14:15:00Z', '2025-12-02T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Standup with Maya and David, discussed upcoming sprint goals.', '["meeting", "standup", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 70
) ON CONFLICT DO NOTHING;

-- E70: Design review with David
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0070', 'day_2025-12-02',
    '2025-12-02T15:00:00Z', '2025-12-02T16:00:00Z',
    'Design review', 'Office', '["calendar", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Design review with David, going through the notification flow iterations.', '["meeting", "design-review", "design"]', '["person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 75
) ON CONFLICT DO NOTHING;

-- E71: Focused work
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0071', 'day_2025-12-02',
    '2025-12-02T16:00:00Z', '2025-12-02T17:30:00Z',
    'Focused work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Applied review feedback to the notification designs.', '["design", "figma", "focus"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 65
) ON CONFLICT DO NOTHING;

-- E72: Lunch (solo)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0072', 'day_2025-12-02',
    '2025-12-02T17:30:00Z', '2025-12-02T18:15:00Z',
    'Lunch', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Lunch at desk, leftovers again.', '["food", "lunch"]', '["place_demo_office"]',
    NULL, 'NEW', 71
) ON CONFLICT DO NOTHING;

-- E73: Afternoon work
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0073', 'day_2025-12-02',
    '2025-12-02T18:15:00Z', '2025-12-02T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Finished the notifications mockups, shared in Slack for async feedback.', '["work", "design", "figma"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 72
) ON CONFLICT DO NOTHING;

-- E74: Bike commute home
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0074', 'day_2025-12-02',
    '2025-12-02T22:30:00Z', '2025-12-02T23:00:00Z',
    'Bike commute', NULL, '["location_visit"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked home in the dark.', '["commute", "cycling"]', '[]',
    NULL, 'NEW', 116
) ON CONFLICT DO NOTHING;

-- E75: Evening run (Tuesday = Mueller)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0075', 'day_2025-12-02',
    '2025-12-02T23:15:00Z', '2025-12-03T00:00:00Z',
    'Evening run', 'Mueller Trails', '["steps", "workout"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Ran 3.5 miles on Mueller trails, felt good despite the cold.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
    NULL, 'NEW', 156
) ON CONFLICT DO NOTHING;

-- E76: Evening at home
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0076', 'day_2025-12-02',
    '2025-12-03T00:00:00Z', '2025-12-03T04:30:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Shower, quick dinner, read before bed.', '["food", "leisure", "reading"]', '["place_demo_home"]',
    NULL, 'NEW', 62
) ON CONFLICT DO NOTHING;

-- ── Wednesday, December 3, 2025 ────────────────────────────────────────────

-- E77: Sleep
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0077', 'day_2025-12-03',
    '2025-12-03T04:30:00Z', '2025-12-03T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'About 6 hours sleep, stayed up a bit late reading.', '["sleep"]', '[]',
    NULL, 'NEW', 62
) ON CONFLICT DO NOTHING;

-- E78: Morning routine
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0078', 'day_2025-12-03',
    '2025-12-03T12:30:00Z', '2025-12-03T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Groggy morning, extra coffee needed.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
    NULL, 'NEW', 63
) ON CONFLICT DO NOTHING;

-- E79: Bike commute
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0079', 'day_2025-12-03',
    '2025-12-03T13:15:00Z', '2025-12-03T13:45:00Z',
    'Bike commute', NULL, '["location_visit"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked to the office, wore an extra layer today.', '["commute", "cycling", "morning"]', '[]',
    NULL, 'NEW', 132
) ON CONFLICT DO NOTHING;

-- E80: Coffee and Slack
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0080', 'day_2025-12-03',
    '2025-12-03T13:45:00Z', '2025-12-03T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Coffee and Slack, the usual.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 72
) ON CONFLICT DO NOTHING;

-- E81: Standup
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0081', 'day_2025-12-03',
    '2025-12-03T14:15:00Z', '2025-12-03T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Wednesday standup, mostly status updates on current designs.', '["meeting", "standup", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 76
) ON CONFLICT DO NOTHING;

-- E82: Focused work
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0082', 'day_2025-12-03',
    '2025-12-03T14:45:00Z', '2025-12-03T17:30:00Z',
    'Focused work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Worked on interaction prototypes for the settings flow.', '["design", "figma", "deep-work", "focus"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 70
) ON CONFLICT DO NOTHING;

-- E83: Lunch with Maya at Tatsu-ya
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0083', 'day_2025-12-03',
    '2025-12-03T17:30:00Z', '2025-12-03T18:30:00Z',
    'Lunch at Ramen Tatsu-ya', 'Ramen Tatsu-ya', '["location_visit"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Wednesday ramen with Maya at Tatsu-ya, talked about holiday plans.', '["food", "social", "ramen"]', '["person_demo_maya", "place_demo_ramen"]',
    NULL, 'NEW', 70
) ON CONFLICT DO NOTHING;

-- E84: Afternoon work
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0084', 'day_2025-12-03',
    '2025-12-03T18:30:00Z', '2025-12-03T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Polished the prototype and shared it with the engineering team.', '["work", "design", "figma"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 72
) ON CONFLICT DO NOTHING;

-- E85: Bike commute home
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0085', 'day_2025-12-03',
    '2025-12-03T22:30:00Z', '2025-12-03T23:00:00Z',
    'Bike commute', NULL, '["location_visit"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked home, cold but clear evening.', '["commute", "cycling"]', '[]',
    NULL, 'NEW', 134
) ON CONFLICT DO NOTHING;

-- E86: Evening at home
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0086', 'day_2025-12-03',
    '2025-12-03T23:00:00Z', '2025-12-04T04:00:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Made a big salad for dinner, watched a documentary about architecture.', '["food", "leisure", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 66
) ON CONFLICT DO NOTHING;

-- ── Thursday, December 4, 2025 (WFH afternoon) ────────────────────────────

-- E87: Sleep
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0087', 'day_2025-12-04',
    '2025-12-04T04:00:00Z', '2025-12-04T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Solid 6.5 hours of sleep.', '["sleep"]', '[]',
    NULL, 'NEW', 55
) ON CONFLICT DO NOTHING;

-- E88: Morning routine
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0088', 'day_2025-12-04',
    '2025-12-04T12:30:00Z', '2025-12-04T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Morning coffee, checked the weather and Slack.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
    NULL, 'NEW', 66
) ON CONFLICT DO NOTHING;

-- E89: Bike commute
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0089', 'day_2025-12-04',
    '2025-12-04T13:15:00Z', '2025-12-04T13:45:00Z',
    'Bike commute', NULL, '["location_visit"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked to the office for the morning.', '["commute", "cycling", "morning"]', '[]',
    NULL, 'NEW', 114
) ON CONFLICT DO NOTHING;

-- E90: Coffee and Slack
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0090', 'day_2025-12-04',
    '2025-12-04T13:45:00Z', '2025-12-04T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Office coffee, reading through design feedback threads.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 68
) ON CONFLICT DO NOTHING;

-- E91: Standup
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0091', 'day_2025-12-04',
    '2025-12-04T14:15:00Z', '2025-12-04T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Thursday standup, quick sync on handoff items for engineering.', '["meeting", "standup", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 72
) ON CONFLICT DO NOTHING;

-- E92: Morning work at office
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0092', 'day_2025-12-04',
    '2025-12-04T14:45:00Z', '2025-12-04T17:30:00Z',
    'Morning work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Worked on handoff docs for the settings page redesign.', '["work", "design", "figma"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 66
) ON CONFLICT DO NOTHING;

-- E93: Lunch (solo at office)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0093', 'day_2025-12-04',
    '2025-12-04T17:30:00Z', '2025-12-04T18:00:00Z',
    'Lunch', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Quick lunch then headed home to WFH for the afternoon.', '["food", "lunch"]', '["place_demo_office"]',
    NULL, 'NEW', 71
) ON CONFLICT DO NOTHING;

-- E94: Bike commute home (midday)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0094', 'day_2025-12-04',
    '2025-12-04T18:00:00Z', '2025-12-04T18:30:00Z',
    'Bike commute', NULL, '["location_visit"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked home midday to work from home the rest of the day.', '["commute", "cycling"]', '[]',
    NULL, 'NEW', 135
) ON CONFLICT DO NOTHING;

-- E95: WFH afternoon
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0095', 'day_2025-12-04',
    '2025-12-04T18:30:00Z', '2025-12-04T22:00:00Z',
    'WFH afternoon', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Worked from home on the couch, finished up some Figma annotations.', '["work", "focus", "figma", "deep-work"]', '["place_demo_home", "org_demo_employer"]',
    NULL, 'NEW', 68
) ON CONFLICT DO NOTHING;

-- E96: Evening walk
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0096', 'day_2025-12-04',
    '2025-12-04T22:00:00Z', '2025-12-04T22:45:00Z',
    'Evening walk', 'Mueller Trails', '["steps"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Went for a walk around Mueller to get some fresh air after WFH.', '["exercise", "outdoors"]', '["place_demo_mueller_trails"]',
    NULL, 'NEW', 146
) ON CONFLICT DO NOTHING;

-- E97: Evening
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0097', 'day_2025-12-04',
    '2025-12-04T22:45:00Z', '2025-12-05T04:00:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Cooked stir-fry, caught up on some browsing, bed around 10.', '["food", "leisure", "browsing", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 64
) ON CONFLICT DO NOTHING;

-- ── Friday, December 5, 2025 (NO game night this week, quieter Friday) ────

-- E98: Sleep
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0098', 'day_2025-12-05',
    '2025-12-05T04:00:00Z', '2025-12-05T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Slept 6.5 hours.', '["sleep"]', '[]',
    NULL, 'NEW', 58
) ON CONFLICT DO NOTHING;

-- E99: Morning routine
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0099', 'day_2025-12-05',
    '2025-12-05T12:30:00Z', '2025-12-05T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'TGIF morning, coffee and messages.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
    NULL, 'NEW', 64
) ON CONFLICT DO NOTHING;

-- E100: Bike commute
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0100', 'day_2025-12-05',
    '2025-12-05T13:15:00Z', '2025-12-05T13:45:00Z',
    'Bike commute', NULL, '["location_visit"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked to the office.', '["commute", "cycling", "morning"]', '[]',
    NULL, 'NEW', 111
) ON CONFLICT DO NOTHING;

-- E101: Coffee and Slack
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0101', 'day_2025-12-05',
    '2025-12-05T13:45:00Z', '2025-12-05T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Friday coffee, lighter Slack traffic.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 66
) ON CONFLICT DO NOTHING;

-- E102: Standup
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0102', 'day_2025-12-05',
    '2025-12-05T14:15:00Z', '2025-12-05T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Friday standup, short and sweet, wrapping up the week.', '["meeting", "standup"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 71
) ON CONFLICT DO NOTHING;

-- E103: Focused work
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0103', 'day_2025-12-05',
    '2025-12-05T14:45:00Z', '2025-12-05T17:30:00Z',
    'Focused work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Wrapped up the week''s design tasks and organized files.', '["work", "focus", "design"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 71
) ON CONFLICT DO NOTHING;

-- E104: Lunch (solo)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0104', 'day_2025-12-05',
    '2025-12-05T17:30:00Z', '2025-12-05T18:15:00Z',
    'Lunch', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Grabbed a sandwich from the deli next door.', '["food", "lunch"]', '["place_demo_office"]',
    NULL, 'NEW', 66
) ON CONFLICT DO NOTHING;

-- E105: Short afternoon
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0105', 'day_2025-12-05',
    '2025-12-05T18:15:00Z', '2025-12-05T21:00:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Tied up loose ends and left a bit early for the weekend.', '["work", "design"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 67
) ON CONFLICT DO NOTHING;

-- E106: Bike commute home (early)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0106', 'day_2025-12-05',
    '2025-12-05T21:00:00Z', '2025-12-05T21:30:00Z',
    'Bike commute', NULL, '["location_visit"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked home early, nice to have the extra evening time.', '["commute", "cycling"]', '[]',
    NULL, 'NEW', 131
) ON CONFLICT DO NOTHING;

-- E107: Quiet Friday evening (no game night)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0107', 'day_2025-12-05',
    '2025-12-05T21:30:00Z', '2025-12-06T04:30:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Quiet Friday night in — cooked a proper dinner, watched a movie, texted Jess about next week.', '["food", "leisure", "messaging", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 60
) ON CONFLICT DO NOTHING;

-- ── Saturday, December 6, 2025 ─────────────────────────────────────────────

-- E108: Sleep
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0108', 'day_2025-12-06',
    '2025-12-06T04:30:00Z', '2025-12-06T14:00:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Slept in until about 8am, felt rested.', '["sleep"]', '[]',
    NULL, 'NEW', 55
) ON CONFLICT DO NOTHING;

-- E109: Slow morning
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0109', 'day_2025-12-06',
    '2025-12-06T14:00:00Z', '2025-12-06T15:30:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Saturday morning, made pancakes and read.', '["routine", "morning", "coffee", "food", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 66
) ON CONFLICT DO NOTHING;

-- E110: Lady Bird Lake walk
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0110', 'day_2025-12-06',
    '2025-12-06T15:30:00Z', '2025-12-06T17:00:00Z',
    'Walk at Lady Bird Lake', 'Lady Bird Lake', '["steps", "location_visit"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Long walk around Lady Bird Lake, gorgeous December morning.', '["exercise", "outdoors"]', '["place_demo_ladybird"]',
    NULL, 'NEW', 88
) ON CONFLICT DO NOTHING;

-- E111: Errands
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0111', 'day_2025-12-06',
    '2025-12-06T17:00:00Z', '2025-12-06T19:00:00Z',
    'Errands', NULL, '["location_visit"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Grocery shopping and picked up a few things at Target.', '["food", "errands"]', '[]',
    NULL, 'NEW', 81
) ON CONFLICT DO NOTHING;

-- E112: Afternoon at home
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0112', 'day_2025-12-06',
    '2025-12-06T19:00:00Z', '2025-12-06T23:00:00Z',
    'Afternoon at home', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Relaxed afternoon, did some reading and online browsing.', '["leisure", "browsing", "reading"]', '["place_demo_home"]',
    NULL, 'NEW', 64
) ON CONFLICT DO NOTHING;

-- E113: Evening (movie night solo)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0113', 'day_2025-12-06',
    '2025-12-06T23:00:00Z', '2025-12-07T04:30:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Made dinner, watched a movie, quiet Saturday night.', '["food", "leisure", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 67
) ON CONFLICT DO NOTHING;

-- ── Sunday, December 7, 2025 (Mom call this weekend) ───────────────────────

-- E114: Sleep
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0114', 'day_2025-12-07',
    '2025-12-07T04:30:00Z', '2025-12-07T14:00:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Slept in on Sunday, about 7.5 hours.', '["sleep"]', '[]',
    NULL, 'NEW', 60
) ON CONFLICT DO NOTHING;

-- E115: Slow morning
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0115', 'day_2025-12-07',
    '2025-12-07T14:00:00Z', '2025-12-07T15:30:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Lazy Sunday morning with coffee and the paper.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
    NULL, 'NEW', 68
) ON CONFLICT DO NOTHING;

-- E116: Mueller run
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0116', 'day_2025-12-07',
    '2025-12-07T15:30:00Z', '2025-12-07T16:15:00Z',
    'Morning run', 'Mueller Trails', '["steps", "workout"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Sunday run on Mueller trails, 3 miles, good pace.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
    NULL, 'NEW', 65
) ON CONFLICT DO NOTHING;

-- E117: Afternoon — reading and cooking
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0117', 'day_2025-12-07',
    '2025-12-07T16:15:00Z', '2025-12-07T20:00:00Z',
    'Afternoon at home', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Spent the afternoon reading and doing meal prep for the week.', '["leisure", "food", "reading", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 65
) ON CONFLICT DO NOTHING;

-- E118: Mom call
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0118', 'day_2025-12-07',
    '2025-12-07T20:00:00Z', '2025-12-07T20:40:00Z',
    'Phone call with Mom', 'Home', '["transcription"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Weekly call with Mom, talked about her week and Christmas plans.', '["family", "phone-call", "reflection"]', '["person_demo_mom", "place_demo_home"]',
    NULL, 'NEW', 71
) ON CONFLICT DO NOTHING;

-- E119: Evening
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0119', 'day_2025-12-07',
    '2025-12-07T20:40:00Z', '2025-12-08T04:00:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Prepped bag for tomorrow, early night.', '["routine", "leisure", "reflection"]', '["place_demo_home"]',
    NULL, 'NEW', 64
) ON CONFLICT DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════════════
-- WEEK 3: Dec 8 (Mon) – Dec 14 (Sun)
-- ═══════════════════════════════════════════════════════════════════════════

-- ── Monday, December 8, 2025 ───────────────────────────────────────────────

-- E120: Sleep
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0120', 'day_2025-12-08',
    '2025-12-08T04:00:00Z', '2025-12-08T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Slept 6.5 hours, alarm at 6:30.', '["sleep"]', '[]',
    NULL, 'NEW', 55
) ON CONFLICT DO NOTHING;

-- E121: Morning routine
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0121', 'day_2025-12-08',
    '2025-12-08T12:30:00Z', '2025-12-08T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Monday morning, coffee and catching up on weekend Slack.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
    NULL, 'NEW', 68
) ON CONFLICT DO NOTHING;

-- E122: Bike commute
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0122', 'day_2025-12-08',
    '2025-12-08T13:15:00Z', '2025-12-08T13:45:00Z',
    'Bike commute', NULL, '["location_visit"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked to the office, foggy morning.', '["commute", "cycling", "morning"]', '[]',
    NULL, 'NEW', 118
) ON CONFLICT DO NOTHING;

-- E123: Coffee and Slack
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0123', 'day_2025-12-08',
    '2025-12-08T13:45:00Z', '2025-12-08T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Monday coffee at the office, reading through weekend messages.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 71
) ON CONFLICT DO NOTHING;

-- E124: Standup
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0124', 'day_2025-12-08',
    '2025-12-08T14:15:00Z', '2025-12-08T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Monday standup with Maya and David, planning the sprint.', '["meeting", "standup", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 77
) ON CONFLICT DO NOTHING;

-- E125: Focused work (long block)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0125', 'day_2025-12-08',
    '2025-12-08T14:45:00Z', '2025-12-08T17:30:00Z',
    'Focused design work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Long focus block on the dashboard data viz redesign.', '["design", "figma", "deep-work", "focus"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 64
) ON CONFLICT DO NOTHING;

-- E126: Lunch (solo)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0126', 'day_2025-12-08',
    '2025-12-08T17:30:00Z', '2025-12-08T18:15:00Z',
    'Lunch', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Ate my meal prep at the office, chatted with a couple coworkers.', '["food", "lunch", "social"]', '["place_demo_office"]',
    NULL, 'NEW', 69
) ON CONFLICT DO NOTHING;

-- E127: Afternoon work
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0127', 'day_2025-12-08',
    '2025-12-08T18:15:00Z', '2025-12-08T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Afternoon meetings and async design feedback, worked with David on the chart components.', '["work", "design", "figma"]', '["person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 68
) ON CONFLICT DO NOTHING;

-- E128: Bike commute home
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0128', 'day_2025-12-08',
    '2025-12-08T22:30:00Z', '2025-12-08T23:00:00Z',
    'Bike commute', NULL, '["location_visit"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked home, bundled up.', '["commute", "cycling"]', '[]',
    NULL, 'NEW', 122
) ON CONFLICT DO NOTHING;

-- E129: Evening
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0129', 'day_2025-12-08',
    '2025-12-08T23:00:00Z', '2025-12-09T04:00:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Made soup for dinner, read a chapter of my book, early night.', '["food", "leisure", "cooking", "reading"]', '["place_demo_home"]',
    NULL, 'NEW', 62
) ON CONFLICT DO NOTHING;

-- ── Tuesday, December 9, 2025 ──────────────────────────────────────────────

-- E130: Sleep
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0130', 'day_2025-12-09',
    '2025-12-09T04:00:00Z', '2025-12-09T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Slept well, about 6.5 hours.', '["sleep"]', '[]',
    NULL, 'NEW', 58
) ON CONFLICT DO NOTHING;

-- E131: Morning routine
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0131', 'day_2025-12-09',
    '2025-12-09T12:30:00Z', '2025-12-09T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Morning coffee and quick browse through email.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
    NULL, 'NEW', 65
) ON CONFLICT DO NOTHING;

-- E132: Bike commute
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0132', 'day_2025-12-09',
    '2025-12-09T13:15:00Z', '2025-12-09T13:45:00Z',
    'Bike commute', NULL, '["location_visit"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked to the office, clear and cold.', '["commute", "cycling", "morning"]', '[]',
    NULL, 'NEW', 113
) ON CONFLICT DO NOTHING;

-- E133: Coffee and Slack
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0133', 'day_2025-12-09',
    '2025-12-09T13:45:00Z', '2025-12-09T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Got coffee and checked in on Slack, a few bugs reported in the latest build.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 68
) ON CONFLICT DO NOTHING;

-- E134: Standup
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0134', 'day_2025-12-09',
    '2025-12-09T14:15:00Z', '2025-12-09T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Standup with Maya and David, discussed the data viz sprint.', '["meeting", "standup", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 77
) ON CONFLICT DO NOTHING;

-- E135: Design review with David
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0135', 'day_2025-12-09',
    '2025-12-09T15:00:00Z', '2025-12-09T16:00:00Z',
    'Design review', 'Office', '["calendar", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Reviewed the chart component specs with David, discussed edge cases.', '["meeting", "design-review", "design"]', '["person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 76
) ON CONFLICT DO NOTHING;

-- E136: Focused work
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0136', 'day_2025-12-09',
    '2025-12-09T16:00:00Z', '2025-12-09T17:30:00Z',
    'Focused work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Iterated on the chart designs based on David''s feedback.', '["design", "figma", "focus"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 69
) ON CONFLICT DO NOTHING;

-- E137: Lunch (solo)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0137', 'day_2025-12-09',
    '2025-12-09T17:30:00Z', '2025-12-09T18:15:00Z',
    'Lunch', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Lunch at desk, scrolled through design Twitter.', '["food", "lunch", "browsing"]', '["place_demo_office"]',
    NULL, 'NEW', 66
) ON CONFLICT DO NOTHING;

-- E138: Afternoon work
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0138', 'day_2025-12-09',
    '2025-12-09T18:15:00Z', '2025-12-09T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Kept working on the charts, got into a good flow state.', '["design", "figma", "deep-work", "focus"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 68
) ON CONFLICT DO NOTHING;

-- E139: Bike commute home
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0139', 'day_2025-12-09',
    '2025-12-09T22:30:00Z', '2025-12-09T23:00:00Z',
    'Bike commute', NULL, '["location_visit"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked home.', '["commute", "cycling"]', '[]',
    NULL, 'NEW', 126
) ON CONFLICT DO NOTHING;

-- E140: Evening run (Tuesday)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0140', 'day_2025-12-09',
    '2025-12-09T23:15:00Z', '2025-12-10T00:00:00Z',
    'Evening run', 'Mueller Trails', '["steps", "workout"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Ran 4 miles on Mueller trails, pushed a little harder this week.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
    NULL, 'NEW', 146
) ON CONFLICT DO NOTHING;

-- E141: Evening at home
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0141', 'day_2025-12-09',
    '2025-12-10T00:00:00Z', '2025-12-10T04:00:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Quick dinner, shower, read for a bit.', '["food", "leisure", "reading"]', '["place_demo_home"]',
    NULL, 'NEW', 61
) ON CONFLICT DO NOTHING;

-- ── Wednesday, December 10, 2025 ───────────────────────────────────────────

-- E142: Sleep
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0142', 'day_2025-12-10',
    '2025-12-10T04:00:00Z', '2025-12-10T12:45:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'About 6.75 hours sleep.', '["sleep"]', '[]',
    NULL, 'NEW', 60
) ON CONFLICT DO NOTHING;

-- E143: Morning routine
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0143', 'day_2025-12-10',
    '2025-12-10T12:45:00Z', '2025-12-10T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Coffee and morning routine.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
    NULL, 'NEW', 65
) ON CONFLICT DO NOTHING;

-- E144: Bike commute
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0144', 'day_2025-12-10',
    '2025-12-10T13:15:00Z', '2025-12-10T13:45:00Z',
    'Bike commute', NULL, '["location_visit"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked to the office, misty morning.', '["commute", "cycling", "morning"]', '[]',
    NULL, 'NEW', 123
) ON CONFLICT DO NOTHING;

-- E145: Coffee and Slack
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0145', 'day_2025-12-10',
    '2025-12-10T13:45:00Z', '2025-12-10T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Office coffee, lots of Slack threads to catch up on.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 68
) ON CONFLICT DO NOTHING;

-- E146: Standup
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0146', 'day_2025-12-10',
    '2025-12-10T14:15:00Z', '2025-12-10T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Standup with Maya and David, midweek check-in.', '["meeting", "standup", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 72
) ON CONFLICT DO NOTHING;

-- E147: Focused work
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0147', 'day_2025-12-10',
    '2025-12-10T14:45:00Z', '2025-12-10T17:30:00Z',
    'Focused work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Heads down on the dashboard charts, working through the responsive breakpoints.', '["design", "figma", "deep-work", "focus"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 70
) ON CONFLICT DO NOTHING;

-- E148: Lunch with Maya at Tatsu-ya
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0148', 'day_2025-12-10',
    '2025-12-10T17:30:00Z', '2025-12-10T18:30:00Z',
    'Lunch at Ramen Tatsu-ya', 'Ramen Tatsu-ya', '["location_visit"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Wednesday ramen with Maya, she was excited about a new hire starting next month.', '["food", "social", "ramen"]', '["person_demo_maya", "place_demo_ramen"]',
    NULL, 'NEW', 71
) ON CONFLICT DO NOTHING;

-- E149: Afternoon work
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0149', 'day_2025-12-10',
    '2025-12-10T18:30:00Z', '2025-12-10T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Finished the responsive chart mockups and shared for review.', '["work", "design", "figma"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 70
) ON CONFLICT DO NOTHING;

-- E150: Bike commute home
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0150', 'day_2025-12-10',
    '2025-12-10T22:30:00Z', '2025-12-10T23:00:00Z',
    'Bike commute', NULL, '["location_visit"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked home.', '["commute", "cycling"]', '[]',
    NULL, 'NEW', 113
) ON CONFLICT DO NOTHING;

-- E151: Evening at home
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0151', 'day_2025-12-10',
    '2025-12-10T23:00:00Z', '2025-12-11T04:00:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Made a curry for dinner, watched TV, browsed apartment decor ideas.', '["food", "leisure", "browsing", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 61
) ON CONFLICT DO NOTHING;

-- ── Thursday, December 11, 2025 ────────────────────────────────────────────

-- E152: Sleep
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0152', 'day_2025-12-11',
    '2025-12-11T04:00:00Z', '2025-12-11T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Slept 6.5 hours.', '["sleep"]', '[]',
    NULL, 'NEW', 58
) ON CONFLICT DO NOTHING;

-- E153: Morning routine
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0153', 'day_2025-12-11',
    '2025-12-11T12:30:00Z', '2025-12-11T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Coffee and messages, chilly morning.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
    NULL, 'NEW', 64
) ON CONFLICT DO NOTHING;

-- E154: Bike commute
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0154', 'day_2025-12-11',
    '2025-12-11T13:15:00Z', '2025-12-11T13:45:00Z',
    'Bike commute', NULL, '["location_visit"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked to the office.', '["commute", "cycling", "morning"]', '[]',
    NULL, 'NEW', 129
) ON CONFLICT DO NOTHING;

-- E155: Coffee and Slack
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0155', 'day_2025-12-11',
    '2025-12-11T13:45:00Z', '2025-12-11T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Coffee and checking Slack, David shared some chart edge cases.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 65
) ON CONFLICT DO NOTHING;

-- E156: Standup
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0156', 'day_2025-12-11',
    '2025-12-11T14:15:00Z', '2025-12-11T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Thursday standup with Maya and David.', '["meeting", "standup", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 75
) ON CONFLICT DO NOTHING;

-- E157: Focused work
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0157', 'day_2025-12-11',
    '2025-12-11T14:45:00Z', '2025-12-11T17:30:00Z',
    'Focused work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Worked on the edge case charts David flagged, tricky empty-state designs.', '["design", "figma", "deep-work", "focus"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 67
) ON CONFLICT DO NOTHING;

-- E158: Lunch (solo)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0158', 'day_2025-12-11',
    '2025-12-11T17:30:00Z', '2025-12-11T18:15:00Z',
    'Lunch', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Grabbed a burrito from the taco truck outside.', '["food", "lunch"]', '["place_demo_office"]',
    NULL, 'NEW', 69
) ON CONFLICT DO NOTHING;

-- E159: Afternoon work
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0159', 'day_2025-12-11',
    '2025-12-11T18:15:00Z', '2025-12-11T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Finished the empty-state designs and started on the loading skeleton patterns.', '["work", "design", "figma"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 66
) ON CONFLICT DO NOTHING;

-- E160: Bike commute home
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0160', 'day_2025-12-11',
    '2025-12-11T22:30:00Z', '2025-12-11T23:00:00Z',
    'Bike commute', NULL, '["location_visit"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked home.', '["commute", "cycling"]', '[]',
    NULL, 'NEW', 111
) ON CONFLICT DO NOTHING;

-- E161: Evening walk and dinner
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0161', 'day_2025-12-11',
    '2025-12-11T23:00:00Z', '2025-12-12T00:00:00Z',
    'Evening walk', 'Mueller Trails', '["steps"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Walked around Mueller, the holiday lights were going up on the houses.', '["exercise", "outdoors"]', '["place_demo_mueller_trails"]',
    NULL, 'NEW', 155
) ON CONFLICT DO NOTHING;

-- E162: Evening
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0162', 'day_2025-12-11',
    '2025-12-12T00:00:00Z', '2025-12-12T04:00:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Dinner and some reading, thinking about Christmas gifts.', '["food", "leisure", "reading", "reflection"]', '["place_demo_home"]',
    NULL, 'NEW', 65
) ON CONFLICT DO NOTHING;

-- ── Friday, December 12, 2025 (Game night at Jess's) ──────────────────────

-- E163: Sleep
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0163', 'day_2025-12-12',
    '2025-12-12T04:00:00Z', '2025-12-12T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'About 6.5 hours sleep.', '["sleep"]', '[]',
    NULL, 'NEW', 55
) ON CONFLICT DO NOTHING;

-- E164: Morning routine
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0164', 'day_2025-12-12',
    '2025-12-12T12:30:00Z', '2025-12-12T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Friday morning, coffee and checking Slack, excited for game night tonight.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
    NULL, 'NEW', 66
) ON CONFLICT DO NOTHING;

-- E165: Bike commute
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0165', 'day_2025-12-12',
    '2025-12-12T13:15:00Z', '2025-12-12T13:45:00Z',
    'Bike commute', NULL, '["location_visit"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked to the office.', '["commute", "cycling", "morning"]', '[]',
    NULL, 'NEW', 111
) ON CONFLICT DO NOTHING;

-- E166: Coffee and Slack
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0166', 'day_2025-12-12',
    '2025-12-12T13:45:00Z', '2025-12-12T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Friday coffee, looking forward to the weekend.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 72
) ON CONFLICT DO NOTHING;

-- E167: Standup
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0167', 'day_2025-12-12',
    '2025-12-12T14:15:00Z', '2025-12-12T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Friday standup, wrapped up the week''s work items.', '["meeting", "standup"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 73
) ON CONFLICT DO NOTHING;

-- E168: Focused work
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0168', 'day_2025-12-12',
    '2025-12-12T14:45:00Z', '2025-12-12T17:30:00Z',
    'Focused work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Cleaned up the Figma files and organized the design system components.', '["work", "design", "figma"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 67
) ON CONFLICT DO NOTHING;

-- E169: Lunch (with Maya, casual)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0169', 'day_2025-12-12',
    '2025-12-12T17:30:00Z', '2025-12-12T18:15:00Z',
    'Lunch', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Ate lunch with Maya in the break room, chatted about weekend plans.', '["food", "social", "lunch"]', '["person_demo_maya", "place_demo_office"]',
    NULL, 'NEW', 71
) ON CONFLICT DO NOTHING;

-- E170: Short afternoon
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0170', 'day_2025-12-12',
    '2025-12-12T18:15:00Z', '2025-12-12T21:00:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Quick afternoon wrapping things up, left early for game night.', '["work", "design"]', '["place_demo_office", "org_demo_employer"]',
    NULL, 'NEW', 71
) ON CONFLICT DO NOTHING;

-- E171: Bike commute home
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0171', 'day_2025-12-12',
    '2025-12-12T21:00:00Z', '2025-12-12T21:30:00Z',
    'Bike commute', NULL, '["location_visit"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked home to change before heading to Jess''s.', '["commute", "cycling"]', '[]',
    NULL, 'NEW', 128
) ON CONFLICT DO NOTHING;

-- E172: Quick break at home
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0172', 'day_2025-12-12',
    '2025-12-12T21:30:00Z', '2025-12-13T00:00:00Z',
    'Home break', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Changed clothes, grabbed some snacks to bring to game night.', '["routine"]', '["place_demo_home"]',
    NULL, 'NEW', 71
) ON CONFLICT DO NOTHING;

-- E173: Game night at Jess's
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0173', 'day_2025-12-12',
    '2025-12-13T00:00:00Z', '2025-12-13T04:30:00Z',
    'Game night', 'Jess''s Place', '["location_visit"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Game night at Jess''s with Priya — played Ticket to Ride and Wavelength, great time.', '["social", "games"]', '["person_demo_jess", "person_demo_priya", "place_demo_jess"]',
    NULL, 'NEW', 74
) ON CONFLICT DO NOTHING;

-- ── Saturday, December 13, 2025 ────────────────────────────────────────────

-- E174: Sleep
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0174', 'day_2025-12-13',
    '2025-12-13T04:30:00Z', '2025-12-13T14:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Slept in after game night, about 8 hours.', '["sleep"]', '[]',
    NULL, 'NEW', 56
) ON CONFLICT DO NOTHING;

-- E175: Slow morning
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0175', 'day_2025-12-13',
    '2025-12-13T14:30:00Z', '2025-12-13T16:00:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Slow Saturday morning, made waffles, scrolled through the internet.', '["routine", "morning", "coffee", "food", "browsing", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 66
) ON CONFLICT DO NOTHING;

-- E176: Lady Bird Lake walk
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0176', 'day_2025-12-13',
    '2025-12-13T16:00:00Z', '2025-12-13T17:30:00Z',
    'Walk at Lady Bird Lake', 'Lady Bird Lake', '["steps", "location_visit"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Walked the Lady Bird Lake trail, the water was really still today.', '["exercise", "outdoors"]', '["place_demo_ladybird"]',
    NULL, 'NEW', 93
) ON CONFLICT DO NOTHING;

-- E177: Jo's Coffee
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0177', 'day_2025-12-13',
    '2025-12-13T17:30:00Z', '2025-12-13T19:00:00Z',
    'Coffee at Jo''s', 'Jo''s Coffee', '["location_visit"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Stopped at Jo''s on South Congress for a latte, read for a while.', '["coffee", "leisure"]', '["place_demo_jos"]',
    NULL, 'NEW', 72
) ON CONFLICT DO NOTHING;

-- E178: Afternoon at home
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0178', 'day_2025-12-13',
    '2025-12-13T19:00:00Z', '2025-12-13T23:00:00Z',
    'Afternoon at home', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Relaxed at home, did some online Christmas shopping.', '["leisure", "browsing", "shopping"]', '["place_demo_home"]',
    NULL, 'NEW', 71
) ON CONFLICT DO NOTHING;

-- E179: Evening
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0179', 'day_2025-12-13',
    '2025-12-13T23:00:00Z', '2025-12-14T04:30:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Cooked pasta, watched a holiday movie, early-ish night.', '["food", "leisure", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 67
) ON CONFLICT DO NOTHING;

-- ── Sunday, December 14, 2025 ──────────────────────────────────────────────

-- E180: Sleep
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0180', 'day_2025-12-14',
    '2025-12-14T04:30:00Z', '2025-12-14T14:00:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Slept well, about 7.5 hours.', '["sleep"]', '[]',
    NULL, 'NEW', 62
) ON CONFLICT DO NOTHING;

-- E181: Slow morning
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0181', 'day_2025-12-14',
    '2025-12-14T14:00:00Z', '2025-12-14T15:30:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Sunday morning, big brunch with eggs and toast, read the news.', '["routine", "morning", "coffee", "food", "cooking"]', '["place_demo_home"]',
    NULL, 'NEW', 66
) ON CONFLICT DO NOTHING;

-- E182: Mueller run
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0182', 'day_2025-12-14',
    '2025-12-14T15:30:00Z', '2025-12-14T16:15:00Z',
    'Morning run', 'Mueller Trails', '["steps", "workout"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Sunday run on Mueller, 3 miles at an easy pace.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
    NULL, 'NEW', 65
) ON CONFLICT DO NOTHING;

-- E183: Afternoon — meal prep and reading
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0183', 'day_2025-12-14',
    '2025-12-14T16:15:00Z', '2025-12-14T21:00:00Z',
    'Afternoon at home', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Meal prepped for the week, did laundry, read a couple chapters.', '["food", "leisure", "cooking", "reading"]', '["place_demo_home"]',
    NULL, 'NEW', 70
) ON CONFLICT DO NOTHING;

-- E184: Evening
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action, avg_hr
) VALUES (
    'ev_b0184', 'day_2025-12-14',
    '2025-12-14T21:00:00Z', '2025-12-15T04:00:00Z',
    'Evening at home', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Prepped bag for the week, caught up on a podcast, early night.', '["routine", "leisure", "podcast"]', '["place_demo_home"]',
    NULL, 'NEW', 65
) ON CONFLICT DO NOTHING;
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
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-12-15', '2025-12-15', 'America/Chicago', 0.48) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-12-16', '2025-12-16', 'America/Chicago', 0.52) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-12-17', '2025-12-17', 'America/Chicago', 0.50) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-12-18', '2025-12-18', 'America/Chicago', 0.45) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-12-19', '2025-12-19', 'America/Chicago', 0.53) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-12-20', '2025-12-20', 'America/Chicago', 0.55) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-12-21', '2025-12-21', 'America/Chicago', 0.50) ON CONFLICT DO NOTHING;

-- Week 5: Dec 22-28 (Christmas week)
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-12-22', '2025-12-22', 'America/Chicago', 0.47) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-12-23', '2025-12-23', 'America/Chicago', 0.50) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-12-24', '2025-12-24', 'America/Chicago', 0.55) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-12-25', '2025-12-25', 'America/Chicago', 0.58) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-12-26', '2025-12-26', 'America/Chicago', 0.52) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-12-27', '2025-12-27', 'America/Chicago', 0.50) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-12-28', '2025-12-28', 'America/Chicago', 0.48) ON CONFLICT DO NOTHING;

-- Week 6: Dec 29 - Jan 4 (New Year's week)
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-12-29', '2025-12-29', 'America/Chicago', 0.46) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-12-30', '2025-12-30', 'America/Chicago', 0.50) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2025-12-31', '2025-12-31', 'America/Chicago', 0.52) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2026-01-01', '2026-01-01', 'America/Chicago', 0.42) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2026-01-02', '2026-01-02', 'America/Chicago', 0.45) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2026-01-03', '2026-01-03', 'America/Chicago', 0.50) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2026-01-04', '2026-01-04', 'America/Chicago', 0.48) ON CONFLICT DO NOTHING;


-- ─────────────────────────────────────────────────────────────────────────────
-- WIKI EVENTS
-- ─────────────────────────────────────────────────────────────────────────────
-- All times UTC (CST + 6). December/January is CST (UTC-6).
-- Midnight CST = 06:00 UTC, 6:30am CST = 12:30 UTC, etc.

-- =============================================================================
-- MONDAY December 15, 2025 — Normal weekday
-- =============================================================================

-- Sleep (00:00-06:30 CST = 06:00-12:30 UTC)
INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0211', 'day_2025-12-15', '2025-12-15T06:00:00Z', '2025-12-15T12:30:00Z', 'Sleep', 'Home', '["sleep"]', FALSE, FALSE, TRUE, FALSE, FALSE, 'Sleep from midnight to 6:30am, 6.5 hours.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 56) ON CONFLICT DO NOTHING;

-- Morning routine (06:30-07:15 CST = 12:30-13:15 UTC)
INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0212', 'day_2025-12-15', '2025-12-15T12:30:00Z', '2025-12-15T13:15:00Z', 'Morning routine', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Coffee and checking messages before heading out.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 63) ON CONFLICT DO NOTHING;

-- Bike commute (07:15-07:45 CST = 13:15-13:45 UTC)
INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0213', 'day_2025-12-15', '2025-12-15T13:15:00Z', '2025-12-15T13:45:00Z', 'Bike commute', NULL, '["location_visit", "steps"]', FALSE, TRUE, FALSE, FALSE, FALSE, 'Bike commute from Mueller to downtown office, chilly morning.', '["commute", "cycling", "morning"]', '[]', NULL, NULL, NULL, 'NEW', 133) ON CONFLICT DO NOTHING;

-- Coffee and Slack (07:45-08:15 CST = 13:45-14:15 UTC)
INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0214', 'day_2025-12-15', '2025-12-15T13:45:00Z', '2025-12-15T14:15:00Z', 'Coffee and Slack', 'Office', '["app_usage", "message"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Office coffee and catching up on Slack before standup.', '["messaging", "work"]', '["place_demo_office", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 69) ON CONFLICT DO NOTHING;

-- Design standup (08:15-08:45 CST = 14:15-14:45 UTC)
INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0215', 'day_2025-12-15', '2025-12-15T14:15:00Z', '2025-12-15T14:45:00Z', 'Design standup', 'Office', '["calendar", "transcription"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Monday standup with Maya and David, reviewing sprint priorities.', '["meeting", "standup", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 73) ON CONFLICT DO NOTHING;

-- Focused design work (09:00-11:30 CST = 15:00-17:30 UTC)
INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0216', 'day_2025-12-15', '2025-12-15T15:00:00Z', '2025-12-15T17:30:00Z', 'Focused design work', 'Office', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Deep work in Figma on the settings page redesign.', '["design", "figma", "focus", "deep-work"]', '["place_demo_office", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 63) ON CONFLICT DO NOTHING;

-- Lunch solo (11:30-12:15 CST = 17:30-18:15 UTC)
INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0217', 'day_2025-12-15', '2025-12-15T17:30:00Z', '2025-12-15T18:15:00Z', 'Lunch', 'Office', '["location_visit"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Solo lunch at the office, ate leftover soup at her desk.', '["food"]', '["place_demo_office"]', NULL, NULL, NULL, 'NEW', 72) ON CONFLICT DO NOTHING;

-- Afternoon work (12:30-16:30 CST = 18:30-22:30 UTC)
INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0218', 'day_2025-12-15', '2025-12-15T18:30:00Z', '2025-12-15T22:30:00Z', 'Afternoon work', 'Office', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Worked on component library updates and responded to design review comments.', '["work", "design", "figma", "code-review"]', '["place_demo_office", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 65) ON CONFLICT DO NOTHING;

-- Bike commute home (16:30-17:00 CST = 22:30-23:00 UTC)
INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0219', 'day_2025-12-15', '2025-12-15T22:30:00Z', '2025-12-15T23:00:00Z', 'Bike commute', NULL, '["location_visit", "steps"]', FALSE, TRUE, FALSE, FALSE, FALSE, 'Bike ride home from the office.', '["commute", "cycling"]', '[]', NULL, NULL, NULL, 'NEW', 131) ON CONFLICT DO NOTHING;

-- Evening at home (17:30-21:30 CST = 23:30-03:30+1 UTC)
INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0220', 'day_2025-12-15', '2025-12-15T23:30:00Z', '2025-12-16T03:30:00Z', 'Evening at home', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Made stir-fry for dinner, then read on the couch for a couple hours.', '["food", "leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 68) ON CONFLICT DO NOTHING;

-- Wind down (21:30-00:00 CST = 03:30-06:00+1 UTC)
INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0221', 'day_2025-12-15', '2025-12-16T03:30:00Z', '2025-12-16T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Browsed Reddit and watched a YouTube video before bed.', '["leisure", "browsing"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 58) ON CONFLICT DO NOTHING;

-- =============================================================================
-- TUESDAY December 16, 2025 — Normal weekday, evening run
-- =============================================================================

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0222', 'day_2025-12-16', '2025-12-16T06:00:00Z', '2025-12-16T12:30:00Z', 'Sleep', 'Home', '["sleep"]', FALSE, FALSE, TRUE, FALSE, FALSE, 'Sleep from midnight to 6:30am, 6.5 hours.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 61) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0223', 'day_2025-12-16', '2025-12-16T12:30:00Z', '2025-12-16T13:15:00Z', 'Morning routine', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Morning coffee and scrolling through news.', '["routine", "morning", "coffee", "browsing"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 63) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0224', 'day_2025-12-16', '2025-12-16T13:15:00Z', '2025-12-16T13:45:00Z', 'Bike commute', NULL, '["location_visit", "steps"]', FALSE, TRUE, FALSE, FALSE, FALSE, 'Bike commute to the office, cold but clear.', '["commute", "cycling", "morning"]', '[]', NULL, NULL, NULL, 'NEW', 110) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0225', 'day_2025-12-16', '2025-12-16T13:45:00Z', '2025-12-16T14:15:00Z', 'Coffee and Slack', 'Office', '["app_usage", "message"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Checked Slack and email over coffee at the office.', '["messaging", "work"]', '["place_demo_office", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 66) ON CONFLICT DO NOTHING;

-- Standup + design review with David (Tuesday pattern)
INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0226', 'day_2025-12-16', '2025-12-16T14:15:00Z', '2025-12-16T15:15:00Z', 'Standup and design review', 'Office', '["calendar", "transcription"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Standup followed by design review with David on the dashboard components.', '["meeting", "standup", "design", "design-review"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 73) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0227', 'day_2025-12-16', '2025-12-16T15:15:00Z', '2025-12-16T17:30:00Z', 'Focused design work', 'Office', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Iterated on dashboard wireframes in Figma after review feedback.', '["design", "figma", "focus", "deep-work"]', '["place_demo_office", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 63) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0228', 'day_2025-12-16', '2025-12-16T17:30:00Z', '2025-12-16T18:15:00Z', 'Lunch', 'Office', '["location_visit"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Grabbed a sandwich from the deli downstairs.', '["food"]', '["place_demo_office"]', NULL, NULL, NULL, 'NEW', 78) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0229', 'day_2025-12-16', '2025-12-16T18:30:00Z', '2025-12-16T22:30:00Z', 'Afternoon work', 'Office', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Continued dashboard iteration and prepped assets for handoff.', '["work", "design", "figma"]', '["place_demo_office", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 64) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0230', 'day_2025-12-16', '2025-12-16T22:30:00Z', '2025-12-16T23:00:00Z', 'Bike commute', NULL, '["location_visit", "steps"]', FALSE, TRUE, FALSE, FALSE, FALSE, 'Bike ride home from the office.', '["commute", "cycling"]', '[]', NULL, NULL, NULL, 'NEW', 127) ON CONFLICT DO NOTHING;

-- Evening run on Mueller trails (Tuesday pattern)
INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0231', 'day_2025-12-16', '2025-12-16T23:30:00Z', '2025-12-17T00:15:00Z', 'Evening run', 'Mueller Trails', '["steps", "workout"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Evening run on the Mueller trails, 3.2 miles in the cold.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]', NULL, NULL, NULL, 'NEW', 63) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0232', 'day_2025-12-16', '2025-12-17T00:30:00Z', '2025-12-17T03:30:00Z', 'Evening at home', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Showered, heated up leftovers, watched an episode of a show.', '["food", "leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 68) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0233', 'day_2025-12-16', '2025-12-17T03:30:00Z', '2025-12-17T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Scrolled through Instagram and texted Jess about Friday plans.', '["leisure", "messaging"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 61) ON CONFLICT DO NOTHING;

-- =============================================================================
-- WEDNESDAY December 17, 2025 — Lunch at Tatsu-ya with Maya
-- =============================================================================

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0234', 'day_2025-12-17', '2025-12-17T06:00:00Z', '2025-12-17T12:30:00Z', 'Sleep', 'Home', '["sleep"]', FALSE, FALSE, TRUE, FALSE, FALSE, 'Sleep from midnight to 6:30am, 6.5 hours.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 58) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0235', 'day_2025-12-17', '2025-12-17T12:30:00Z', '2025-12-17T13:15:00Z', 'Morning routine', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Coffee and catching up on Slack messages before heading out.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 66) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0236', 'day_2025-12-17', '2025-12-17T13:15:00Z', '2025-12-17T13:45:00Z', 'Bike commute', NULL, '["location_visit", "steps"]', FALSE, TRUE, FALSE, FALSE, FALSE, 'Bike commute to the office.', '["commute", "cycling", "morning"]', '[]', NULL, NULL, NULL, 'NEW', 128) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0237', 'day_2025-12-17', '2025-12-17T13:45:00Z', '2025-12-17T14:15:00Z', 'Coffee and Slack', 'Office', '["app_usage", "message"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Morning coffee and reviewing design feedback in Slack.', '["messaging", "work", "code-review"]', '["place_demo_office", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 69) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0238', 'day_2025-12-17', '2025-12-17T14:15:00Z', '2025-12-17T14:45:00Z', 'Design standup', 'Office', '["calendar", "transcription"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Wednesday standup, quick sync on year-end tasks.', '["meeting", "standup", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 70) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0239', 'day_2025-12-17', '2025-12-17T15:00:00Z', '2025-12-17T17:30:00Z', 'Focused work', 'Office', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Deep work session on the settings page flow.', '["design", "figma", "focus", "deep-work"]', '["place_demo_office", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 68) ON CONFLICT DO NOTHING;

-- Wednesday: Lunch at Tatsu-ya with Maya
INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0240', 'day_2025-12-17', '2025-12-17T17:30:00Z', '2025-12-17T18:30:00Z', 'Lunch with Maya', 'Ramen Tatsu-ya', '["location_visit", "transcription"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Weekly lunch at Tatsu-ya with Maya, talked about holiday plans.', '["food", "social", "ramen"]', '["person_demo_maya", "place_demo_ramen"]', NULL, NULL, NULL, 'NEW', 72) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0241', 'day_2025-12-17', '2025-12-17T18:30:00Z', '2025-12-17T22:30:00Z', 'Afternoon work', 'Office', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Finalized settings page mockups and shared with the team.', '["work", "design", "figma"]', '["place_demo_office", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 70) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0242', 'day_2025-12-17', '2025-12-17T22:30:00Z', '2025-12-17T23:00:00Z', 'Bike commute', NULL, '["location_visit", "steps"]', FALSE, TRUE, FALSE, FALSE, FALSE, 'Bike ride home from the office.', '["commute", "cycling"]', '[]', NULL, NULL, NULL, 'NEW', 120) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0243', 'day_2025-12-17', '2025-12-17T23:30:00Z', '2025-12-18T04:00:00Z', 'Evening at home', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Made pasta for dinner, then watched a documentary about architecture.', '["food", "leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 64) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0244', 'day_2025-12-17', '2025-12-18T04:00:00Z', '2025-12-18T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Reading in bed before falling asleep.', '["leisure", "reflection"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 59) ON CONFLICT DO NOTHING;

-- =============================================================================
-- THURSDAY December 18, 2025 — WFH afternoon, walk
-- =============================================================================

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0245', 'day_2025-12-18', '2025-12-18T06:00:00Z', '2025-12-18T12:15:00Z', 'Sleep', 'Home', '["sleep"]', FALSE, FALSE, TRUE, FALSE, FALSE, 'Sleep from midnight to 6:15am, 6.25 hours.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 58) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0246', 'day_2025-12-18', '2025-12-18T12:15:00Z', '2025-12-18T13:15:00Z', 'Morning routine', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Slow morning, coffee and checking messages.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 65) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0247', 'day_2025-12-18', '2025-12-18T13:15:00Z', '2025-12-18T13:45:00Z', 'Bike commute', NULL, '["location_visit", "steps"]', FALSE, TRUE, FALSE, FALSE, FALSE, 'Bike commute to the office.', '["commute", "cycling", "morning"]', '[]', NULL, NULL, NULL, 'NEW', 113) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0248', 'day_2025-12-18', '2025-12-18T14:15:00Z', '2025-12-18T14:45:00Z', 'Design standup', 'Office', '["calendar", "transcription"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Thursday standup, most of the team already wrapping up before the holidays.', '["meeting", "standup", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 71) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0249', 'day_2025-12-18', '2025-12-18T15:00:00Z', '2025-12-18T17:30:00Z', 'Focused work', 'Office', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Morning design work on year-end polish items.', '["design", "figma", "focus", "deep-work"]', '["place_demo_office", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 65) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0250', 'day_2025-12-18', '2025-12-18T17:30:00Z', '2025-12-18T18:15:00Z', 'Lunch', 'Office', '["location_visit"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Quick lunch at the office before heading home.', '["food"]', '["place_demo_office"]', NULL, NULL, NULL, 'NEW', 71) ON CONFLICT DO NOTHING;

-- WFH afternoon
INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0251', 'day_2025-12-18', '2025-12-18T18:30:00Z', '2025-12-18T19:00:00Z', 'Bike commute', NULL, '["location_visit", "steps"]', FALSE, TRUE, FALSE, FALSE, FALSE, 'Rode home early to work from home for the afternoon.', '["commute", "cycling"]', '[]', NULL, NULL, NULL, 'NEW', 121) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0252', 'day_2025-12-18', '2025-12-18T19:00:00Z', '2025-12-18T22:00:00Z', 'WFH afternoon', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Worked from home on Slack messages and final design tweaks before the holiday break.', '["work", "messaging", "design"]', '["place_demo_home", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 67) ON CONFLICT DO NOTHING;

-- Walk in the afternoon
INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0253', 'day_2025-12-18', '2025-12-18T22:30:00Z', '2025-12-18T23:15:00Z', 'Walk', 'Mueller Trails', '["steps"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Late afternoon walk around Mueller trails to clear her head.', '["exercise", "outdoors"]', '["place_demo_mueller_trails"]', NULL, NULL, NULL, 'NEW', 93) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0254', 'day_2025-12-18', '2025-12-18T23:30:00Z', '2025-12-19T04:00:00Z', 'Evening at home', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Cooked a big batch of chili for the week, listened to a podcast.', '["food", "leisure", "podcast"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 60) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0255', 'day_2025-12-18', '2025-12-19T04:00:00Z', '2025-12-19T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Browsed holiday gift ideas online.', '["leisure", "browsing", "errands"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 63) ON CONFLICT DO NOTHING;

-- =============================================================================
-- FRIDAY December 19, 2025 — Shorter day, game night at Jess's, Mom call
-- =============================================================================

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0256', 'day_2025-12-19', '2025-12-19T06:00:00Z', '2025-12-19T12:30:00Z', 'Sleep', 'Home', '["sleep"]', FALSE, FALSE, TRUE, FALSE, FALSE, 'Sleep from midnight to 6:30am, 6.5 hours.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 62) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0257', 'day_2025-12-19', '2025-12-19T12:30:00Z', '2025-12-19T13:15:00Z', 'Morning routine', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Coffee and quick morning browse.', '["routine", "morning", "coffee"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 67) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0258', 'day_2025-12-19', '2025-12-19T13:15:00Z', '2025-12-19T13:45:00Z', 'Bike commute', NULL, '["location_visit", "steps"]', FALSE, TRUE, FALSE, FALSE, FALSE, 'Bike commute to the office.', '["commute", "cycling", "morning"]', '[]', NULL, NULL, NULL, 'NEW', 113) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0259', 'day_2025-12-19', '2025-12-19T14:15:00Z', '2025-12-19T14:45:00Z', 'Design standup', 'Office', '["calendar", "transcription"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Friday standup, last one before the holiday break.', '["meeting", "standup", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 76) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0260', 'day_2025-12-19', '2025-12-19T15:00:00Z', '2025-12-19T17:30:00Z', 'Focused work', 'Office', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Wrapping up loose ends before the holiday break.', '["work", "design", "figma"]', '["place_demo_office", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 62) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0261', 'day_2025-12-19', '2025-12-19T17:30:00Z', '2025-12-19T18:15:00Z', 'Lunch', 'Office', '["location_visit"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Lunch in the break room, the office already felt half-empty.', '["food"]', '["place_demo_office"]', NULL, NULL, NULL, 'NEW', 78) ON CONFLICT DO NOTHING;

-- Left early on Friday
INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0262', 'day_2025-12-19', '2025-12-19T20:00:00Z', '2025-12-19T20:30:00Z', 'Bike commute', NULL, '["location_visit", "steps"]', FALSE, TRUE, FALSE, FALSE, FALSE, 'Left the office early, biked home.', '["commute", "cycling"]', '[]', NULL, NULL, NULL, 'NEW', 119) ON CONFLICT DO NOTHING;

-- Mom call (Friday evening)
INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0263', 'day_2025-12-19', '2025-12-19T23:00:00Z', '2025-12-19T23:45:00Z', 'Phone call with Mom', 'Home', '["transcription"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Weekly call with Mom, talked about Christmas plans and what to bring.', '["family", "phone-call"]', '["person_demo_mom", "place_demo_home"]', NULL, NULL, NULL, 'NEW', 70) ON CONFLICT DO NOTHING;

-- Game night at Jess's
INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0264', 'day_2025-12-19', '2025-12-20T01:00:00Z', '2025-12-20T05:00:00Z', 'Game night', 'Jess''s Place', '["location_visit", "transcription"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Game night at Jess''s with Priya, played Catan and drank mulled wine.', '["social", "games"]', '["person_demo_jess", "person_demo_priya", "place_demo_jess"]', NULL, NULL, NULL, 'NEW', 71) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0265', 'day_2025-12-19', '2025-12-20T05:00:00Z', '2025-12-20T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Got home late from game night, quick scroll before bed.', '["leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 63) ON CONFLICT DO NOTHING;

-- =============================================================================
-- SATURDAY December 20, 2025 — Lady Bird Lake walk, errands
-- =============================================================================

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0266', 'day_2025-12-20', '2025-12-20T06:00:00Z', '2025-12-20T13:30:00Z', 'Sleep', 'Home', '["sleep"]', FALSE, FALSE, TRUE, FALSE, FALSE, 'Slept in after game night, midnight to 7:30am.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 56) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0267', 'day_2025-12-20', '2025-12-20T13:30:00Z', '2025-12-20T15:00:00Z', 'Slow morning', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Lazy Saturday morning, coffee and browsing.', '["routine", "morning", "coffee", "leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 63) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0268', 'day_2025-12-20', '2025-12-20T15:00:00Z', '2025-12-20T16:30:00Z', 'Walk at Lady Bird Lake', 'Lady Bird Lake', '["steps", "location_visit"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Morning walk along Lady Bird Lake, crisp winter air.', '["exercise", "outdoors"]', '["place_demo_ladybird"]', NULL, NULL, NULL, 'NEW', 92) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0269', 'day_2025-12-20', '2025-12-20T17:00:00Z', '2025-12-20T19:00:00Z', 'Holiday errands', NULL, '["location_visit"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Holiday shopping and picking up last-minute gifts.', '["leisure", "errands"]', '[]', NULL, NULL, NULL, 'NEW', 76) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0270', 'day_2025-12-20', '2025-12-20T19:30:00Z', '2025-12-20T23:00:00Z', 'Afternoon at home', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Wrapped presents and watched holiday baking shows.', '["leisure", "family"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 59) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0271', 'day_2025-12-20', '2025-12-20T23:00:00Z', '2025-12-21T01:00:00Z', 'Dinner', 'Home', '["location_visit"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Made tacos for dinner and ate on the couch.', '["food"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 68) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0272', 'day_2025-12-20', '2025-12-21T01:00:00Z', '2025-12-21T06:00:00Z', 'Evening and wind down', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Watched a movie and then read before bed.', '["leisure", "reflection"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 64) ON CONFLICT DO NOTHING;

-- =============================================================================
-- SUNDAY December 21, 2025 — Slow day, cooking, reading
-- =============================================================================

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0273', 'day_2025-12-21', '2025-12-21T06:00:00Z', '2025-12-21T14:00:00Z', 'Sleep', 'Home', '["sleep"]', FALSE, FALSE, TRUE, FALSE, FALSE, 'Sleep from midnight to 8am, nice long sleep.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 56) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0274', 'day_2025-12-21', '2025-12-21T14:00:00Z', '2025-12-21T15:30:00Z', 'Slow morning', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Lazy Sunday morning, coffee and reading.', '["routine", "morning", "coffee", "leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 66) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0275', 'day_2025-12-21', '2025-12-21T16:00:00Z', '2025-12-21T17:00:00Z', 'Run', 'Mueller Trails', '["steps", "workout"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Easy Sunday run on the Mueller trails, 2.5 miles.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]', NULL, NULL, NULL, 'NEW', 149) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0276', 'day_2025-12-21', '2025-12-21T17:30:00Z', '2025-12-21T20:00:00Z', 'Cooking', 'Home', '["location_visit"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Batch cooking for the week — soup and roasted vegetables.', '["food", "cooking"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 75) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0277', 'day_2025-12-21', '2025-12-21T20:00:00Z', '2025-12-22T01:00:00Z', 'Afternoon reading', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Read for a few hours and did some light tidying up.', '["leisure", "reflection"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 63) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0278', 'day_2025-12-21', '2025-12-22T01:00:00Z', '2025-12-22T04:00:00Z', 'Evening', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Watched a holiday movie and packed for Christmas travel.', '["leisure", "family"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 62) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0279', 'day_2025-12-21', '2025-12-22T04:00:00Z', '2025-12-22T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Pre-sleep browsing and setting alarms for tomorrow.', '["leisure", "browsing"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 60) ON CONFLICT DO NOTHING;

-- =============================================================================
-- MONDAY December 22, 2025 — Light WFH day (holiday week starts)
-- =============================================================================

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0280', 'day_2025-12-22', '2025-12-22T06:00:00Z', '2025-12-22T12:30:00Z', 'Sleep', 'Home', '["sleep"]', FALSE, FALSE, TRUE, FALSE, FALSE, 'Sleep from midnight to 6:30am, 6.5 hours.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 60) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0281', 'day_2025-12-22', '2025-12-22T12:30:00Z', '2025-12-22T13:30:00Z', 'Morning routine', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Coffee and checking Slack, most channels pretty quiet.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 64) ON CONFLICT DO NOTHING;

-- WFH for the day
INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0282', 'day_2025-12-22', '2025-12-22T14:00:00Z', '2025-12-22T17:00:00Z', 'WFH morning', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Light work from home, tying up loose ends and writing documentation.', '["work"]', '["place_demo_home", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 68) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0283', 'day_2025-12-22', '2025-12-22T17:00:00Z', '2025-12-22T18:00:00Z', 'Lunch', 'Home', '["location_visit"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Leftover chili for lunch.', '["food"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 74) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0284', 'day_2025-12-22', '2025-12-22T18:00:00Z', '2025-12-22T20:00:00Z', 'WFH afternoon', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'A bit more work, then signed off early for the holidays.', '["work"]', '["place_demo_home", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 63) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0285', 'day_2025-12-22', '2025-12-22T21:00:00Z', '2025-12-22T22:00:00Z', 'Walk', 'Mueller Trails', '["steps"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Afternoon walk through the neighborhood.', '["exercise", "outdoors"]', '["place_demo_mueller_trails"]', NULL, NULL, NULL, 'NEW', 90) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0286', 'day_2025-12-22', '2025-12-22T23:00:00Z', '2025-12-23T03:00:00Z', 'Evening at home', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Baked cookies for Christmas, listened to holiday music.', '["food", "leisure", "cooking", "family"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 68) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0287', 'day_2025-12-22', '2025-12-23T03:00:00Z', '2025-12-23T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Watched a few YouTube videos before bed.', '["leisure", "browsing"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 63) ON CONFLICT DO NOTHING;

-- =============================================================================
-- TUESDAY December 23, 2025 — Light WFH, holiday prep
-- =============================================================================

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0288', 'day_2025-12-23', '2025-12-23T06:00:00Z', '2025-12-23T13:00:00Z', 'Sleep', 'Home', '["sleep"]', FALSE, FALSE, TRUE, FALSE, FALSE, 'Sleep from midnight to 7am.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 58) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0289', 'day_2025-12-23', '2025-12-23T13:00:00Z', '2025-12-23T14:00:00Z', 'Morning routine', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Slow morning coffee, reading holiday recipes.', '["routine", "morning", "coffee"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 64) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0290', 'day_2025-12-23', '2025-12-23T14:00:00Z', '2025-12-23T16:00:00Z', 'WFH morning', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Checked in on a few things, mostly quiet — half the team already off.', '["work", "messaging"]', '["place_demo_home", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 66) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0291', 'day_2025-12-23', '2025-12-23T17:00:00Z', '2025-12-23T19:00:00Z', 'Holiday errands', NULL, '["location_visit"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Ran out to pick up groceries and a last-minute gift.', '["leisure", "errands"]', '[]', NULL, NULL, NULL, 'NEW', 78) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0292', 'day_2025-12-23', '2025-12-23T19:30:00Z', '2025-12-23T22:00:00Z', 'Cooking', 'Home', '["location_visit"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Prepped food for Christmas Eve dinner.', '["food", "family"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 72) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0293', 'day_2025-12-23', '2025-12-23T22:00:00Z', '2025-12-24T01:00:00Z', 'Evening', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Wrapped the last presents and watched a holiday movie.', '["leisure", "family"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 68) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0294', 'day_2025-12-23', '2025-12-24T01:00:00Z', '2025-12-24T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Read in bed and fell asleep early.', '["leisure", "reflection"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 59) ON CONFLICT DO NOTHING;

-- =============================================================================
-- WEDNESDAY December 24, 2025 — Christmas Eve (off work, cozy day)
-- =============================================================================

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0295', 'day_2025-12-24', '2025-12-24T06:00:00Z', '2025-12-24T13:30:00Z', 'Sleep', 'Home', '["sleep"]', FALSE, FALSE, TRUE, FALSE, FALSE, 'Sleep from midnight to 7:30am.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 60) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0296', 'day_2025-12-24', '2025-12-24T13:30:00Z', '2025-12-24T15:00:00Z', 'Slow morning', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Christmas Eve morning, coffee and cinnamon rolls.', '["routine", "morning", "coffee", "food"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 63) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0297', 'day_2025-12-24', '2025-12-24T15:00:00Z', '2025-12-24T16:00:00Z', 'Walk', 'Mueller Trails', '["steps"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Short walk through the Mueller neighborhood, holiday lights everywhere.', '["exercise", "outdoors"]', '["place_demo_mueller_trails"]', NULL, NULL, NULL, 'NEW', 92) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0298', 'day_2025-12-24', '2025-12-24T17:00:00Z', '2025-12-24T20:00:00Z', 'Christmas Eve cooking', 'Home', '["location_visit"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Spent the afternoon cooking Christmas Eve dinner.', '["food", "family"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 68) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0299', 'day_2025-12-24', '2025-12-25T00:00:00Z', '2025-12-25T00:45:00Z', 'Phone call with Mom', 'Home', '["transcription"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'FaceTime with Mom on Christmas Eve, she showed off the tree.', '["family", "phone-call"]', '["person_demo_mom", "place_demo_home"]', NULL, NULL, NULL, 'NEW', 70) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0300', 'day_2025-12-24', '2025-12-25T01:00:00Z', '2025-12-25T04:00:00Z', 'Christmas Eve evening', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Quiet evening at home, watched It''s a Wonderful Life.', '["leisure", "family"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 66) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0301', 'day_2025-12-24', '2025-12-25T04:00:00Z', '2025-12-25T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Texted friends Merry Christmas and fell asleep.', '["messaging", "leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 60) ON CONFLICT DO NOTHING;

-- =============================================================================
-- THURSDAY December 25, 2025 — Christmas Day (quiet, home)
-- =============================================================================

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0302', 'day_2025-12-25', '2025-12-25T06:00:00Z', '2025-12-25T14:00:00Z', 'Sleep', 'Home', '["sleep"]', FALSE, FALSE, TRUE, FALSE, FALSE, 'Slept in on Christmas morning, midnight to 8am.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 56) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0303', 'day_2025-12-25', '2025-12-25T14:00:00Z', '2025-12-25T16:00:00Z', 'Christmas morning', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Slow Christmas morning, opened a gift from Mom that arrived in the mail.', '["routine", "morning", "coffee", "family"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 64) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0304', 'day_2025-12-25', '2025-12-25T16:00:00Z', '2025-12-25T17:00:00Z', 'Phone call with Mom', 'Home', '["transcription"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Long Christmas morning call with Mom, caught up on family news.', '["family", "phone-call"]', '["person_demo_mom", "place_demo_home"]', NULL, NULL, NULL, 'NEW', 70) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0305', 'day_2025-12-25', '2025-12-25T17:30:00Z', '2025-12-25T19:00:00Z', 'Walk', 'Mueller Trails', '["steps"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Christmas walk around Mueller, streets were quiet.', '["exercise", "outdoors"]', '["place_demo_mueller_trails"]', NULL, NULL, NULL, 'NEW', 91) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0306', 'day_2025-12-25', '2025-12-25T19:30:00Z', '2025-12-25T22:00:00Z', 'Christmas cooking', 'Home', '["location_visit"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Made a proper Christmas dinner for herself — roasted chicken and potatoes.', '["food", "family"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 75) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0307', 'day_2025-12-25', '2025-12-25T22:00:00Z', '2025-12-26T04:00:00Z', 'Christmas evening', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Curled up with a new book she got as a gift, then watched a movie.', '["leisure", "family"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 66) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0308', 'day_2025-12-25', '2025-12-26T04:00:00Z', '2025-12-26T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Light browsing before bed, peaceful Christmas night.', '["leisure", "browsing", "family"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 63) ON CONFLICT DO NOTHING;

-- =============================================================================
-- FRIDAY December 26, 2025 — Day off, quiet recovery (no game night — Christmas)
-- =============================================================================

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0309', 'day_2025-12-26', '2025-12-26T06:00:00Z', '2025-12-26T14:00:00Z', 'Sleep', 'Home', '["sleep"]', FALSE, FALSE, TRUE, FALSE, FALSE, 'Slept in, midnight to 8am.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 62) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0310', 'day_2025-12-26', '2025-12-26T14:00:00Z', '2025-12-26T15:30:00Z', 'Slow morning', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Leisurely morning with coffee and leftover Christmas food.', '["routine", "morning", "coffee", "food"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 64) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0311', 'day_2025-12-26', '2025-12-26T16:00:00Z', '2025-12-26T17:30:00Z', 'Walk at Lady Bird Lake', 'Lady Bird Lake', '["steps", "location_visit"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Post-Christmas walk along Lady Bird Lake, beautiful winter day.', '["exercise", "outdoors"]', '["place_demo_ladybird"]', NULL, NULL, NULL, 'NEW', 93) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0312', 'day_2025-12-26', '2025-12-26T18:00:00Z', '2025-12-26T22:00:00Z', 'Afternoon at home', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Read her new book for most of the afternoon.', '["leisure", "reflection"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 60) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0313', 'day_2025-12-26', '2025-12-26T22:00:00Z', '2025-12-27T00:00:00Z', 'Dinner', 'Home', '["location_visit"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Used up Christmas leftovers for a simple dinner.', '["food"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 68) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0314', 'day_2025-12-26', '2025-12-27T00:00:00Z', '2025-12-27T04:00:00Z', 'Evening', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Watched a couple episodes of a show and messaged Jess about New Year''s plans.', '["leisure", "messaging"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 68) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0315', 'day_2025-12-26', '2025-12-27T04:00:00Z', '2025-12-27T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Browsed online sales before bed.', '["leisure", "browsing", "errands"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 62) ON CONFLICT DO NOTHING;

-- =============================================================================
-- SATURDAY December 27, 2025 — Errands, Mom call, quiet
-- =============================================================================

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0316', 'day_2025-12-27', '2025-12-27T06:00:00Z', '2025-12-27T13:30:00Z', 'Sleep', 'Home', '["sleep"]', FALSE, FALSE, TRUE, FALSE, FALSE, 'Sleep from midnight to 7:30am.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 59) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0317', 'day_2025-12-27', '2025-12-27T13:30:00Z', '2025-12-27T15:00:00Z', 'Slow morning', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Coffee and catching up on messages from the holidays.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 68) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0318', 'day_2025-12-27', '2025-12-27T16:00:00Z', '2025-12-27T17:00:00Z', 'Run', 'Mueller Trails', '["steps", "workout"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Saturday morning run on Mueller trails, working off Christmas food.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]', NULL, NULL, NULL, 'NEW', 154) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0319', 'day_2025-12-27', '2025-12-27T18:00:00Z', '2025-12-27T20:00:00Z', 'Errands', NULL, '["location_visit"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Grocery run and returned a couple of things at the store.', '["leisure", "errands"]', '[]', NULL, NULL, NULL, 'NEW', 78) ON CONFLICT DO NOTHING;

-- Mom call (Saturday)
INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0320', 'day_2025-12-27', '2025-12-27T22:00:00Z', '2025-12-27T22:40:00Z', 'Phone call with Mom', 'Home', '["transcription"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Called Mom to recap Christmas and talk about New Year''s plans.', '["family", "phone-call"]', '["person_demo_mom", "place_demo_home"]', NULL, NULL, NULL, 'NEW', 71) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0321', 'day_2025-12-27', '2025-12-27T23:00:00Z', '2025-12-28T03:00:00Z', 'Evening at home', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Cooked a simple stir-fry and watched a movie.', '["food", "leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 65) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0322', 'day_2025-12-27', '2025-12-28T03:00:00Z', '2025-12-28T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Read before bed.', '["leisure", "reflection"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 59) ON CONFLICT DO NOTHING;

-- =============================================================================
-- SUNDAY December 28, 2025 — Quiet Sunday, reading, cooking
-- =============================================================================

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0323', 'day_2025-12-28', '2025-12-28T06:00:00Z', '2025-12-28T14:00:00Z', 'Sleep', 'Home', '["sleep"]', FALSE, FALSE, TRUE, FALSE, FALSE, 'Sleep from midnight to 8am.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 57) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0324', 'day_2025-12-28', '2025-12-28T14:00:00Z', '2025-12-28T15:30:00Z', 'Slow morning', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Lazy Sunday, coffee and reading.', '["routine", "morning", "coffee", "leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 67) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0325', 'day_2025-12-28', '2025-12-28T16:00:00Z', '2025-12-28T17:30:00Z', 'Walk at Lady Bird Lake', 'Lady Bird Lake', '["steps", "location_visit"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Sunday walk along Lady Bird Lake.', '["exercise", "outdoors"]', '["place_demo_ladybird"]', NULL, NULL, NULL, 'NEW', 100) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0326', 'day_2025-12-28', '2025-12-28T18:00:00Z', '2025-12-28T20:00:00Z', 'Cooking', 'Home', '["location_visit"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Made a big pot of lentil soup for the week.', '["food", "cooking"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 69) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0327', 'day_2025-12-28', '2025-12-28T20:00:00Z', '2025-12-29T01:00:00Z', 'Afternoon at home', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Read and did some light journaling about the year.', '["leisure", "reflection"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 58) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0328', 'day_2025-12-28', '2025-12-29T01:00:00Z', '2025-12-29T04:00:00Z', 'Evening', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Watched a documentary and had soup for dinner.', '["leisure", "food"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 61) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0329', 'day_2025-12-28', '2025-12-29T04:00:00Z', '2025-12-29T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Browsing and reading before bed.', '["leisure", "browsing", "reflection"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 59) ON CONFLICT DO NOTHING;

-- =============================================================================
-- MONDAY December 29, 2025 — Light WFH day (holiday week)
-- =============================================================================

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0330', 'day_2025-12-29', '2025-12-29T06:00:00Z', '2025-12-29T13:00:00Z', 'Sleep', 'Home', '["sleep"]', FALSE, FALSE, TRUE, FALSE, FALSE, 'Sleep from midnight to 7am.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 57) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0331', 'day_2025-12-29', '2025-12-29T13:00:00Z', '2025-12-29T14:00:00Z', 'Morning routine', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Coffee and checking in on Slack, still pretty quiet.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 68) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0332', 'day_2025-12-29', '2025-12-29T14:00:00Z', '2025-12-29T17:00:00Z', 'WFH morning', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Light work from home, cleaning up Jira tickets and design files.', '["work", "design"]', '["place_demo_home", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 66) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0333', 'day_2025-12-29', '2025-12-29T17:00:00Z', '2025-12-29T18:00:00Z', 'Lunch', 'Home', '["location_visit"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Lentil soup from yesterday for lunch.', '["food"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 71) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0334', 'day_2025-12-29', '2025-12-29T18:00:00Z', '2025-12-29T20:00:00Z', 'WFH afternoon', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Wrapped up a few things and signed off for the day.', '["work"]', '["place_demo_home", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 68) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0335', 'day_2025-12-29', '2025-12-29T21:00:00Z', '2025-12-29T22:00:00Z', 'Run', 'Mueller Trails', '["steps", "workout"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Afternoon run on Mueller trails, 3 miles.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]', NULL, NULL, NULL, 'NEW', 151) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0336', 'day_2025-12-29', '2025-12-29T23:00:00Z', '2025-12-30T03:00:00Z', 'Evening at home', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Made a simple dinner and started a new show.', '["food", "leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 67) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0337', 'day_2025-12-29', '2025-12-30T03:00:00Z', '2025-12-30T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Browsed year-end lists and articles before bed.', '["leisure", "browsing", "reflection"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 62) ON CONFLICT DO NOTHING;

-- =============================================================================
-- TUESDAY December 30, 2025 — Light WFH day
-- =============================================================================

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0338', 'day_2025-12-30', '2025-12-30T06:00:00Z', '2025-12-30T12:30:00Z', 'Sleep', 'Home', '["sleep"]', FALSE, FALSE, TRUE, FALSE, FALSE, 'Sleep from midnight to 6:30am.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 59) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0339', 'day_2025-12-30', '2025-12-30T12:30:00Z', '2025-12-30T13:30:00Z', 'Morning routine', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Coffee and catching up on messages.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 67) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0340', 'day_2025-12-30', '2025-12-30T14:00:00Z', '2025-12-30T17:00:00Z', 'WFH morning', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Worked on organizing design files and reviewing Q1 roadmap drafts.', '["work", "design", "figma"]', '["place_demo_home", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 63) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0341', 'day_2025-12-30', '2025-12-30T17:00:00Z', '2025-12-30T18:00:00Z', 'Lunch', 'Home', '["location_visit"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Soup and bread for lunch.', '["food"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 71) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0342', 'day_2025-12-30', '2025-12-30T18:00:00Z', '2025-12-30T20:00:00Z', 'WFH afternoon', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Quick video call with Maya to sync on January plans, then signed off.', '["work", "meeting"]', '["person_demo_maya", "place_demo_home", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 70) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0343', 'day_2025-12-30', '2025-12-30T21:00:00Z', '2025-12-30T22:00:00Z', 'Walk', 'Mueller Trails', '["steps"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Afternoon walk through the neighborhood.', '["exercise", "outdoors"]', '["place_demo_mueller_trails"]', NULL, NULL, NULL, 'NEW', 93) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0344', 'day_2025-12-30', '2025-12-30T23:00:00Z', '2025-12-31T03:00:00Z', 'Evening at home', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Made rice and vegetables for dinner, then binge-watched a show.', '["food", "leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 65) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0345', 'day_2025-12-30', '2025-12-31T03:00:00Z', '2025-12-31T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Read a bit before bed, thinking about New Year''s resolutions.', '["leisure", "reflection"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 58) ON CONFLICT DO NOTHING;

-- =============================================================================
-- WEDNESDAY December 31, 2025 — New Year's Eve (social evening with Jess & Priya)
-- =============================================================================

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0346', 'day_2025-12-31', '2025-12-31T06:00:00Z', '2025-12-31T13:00:00Z', 'Sleep', 'Home', '["sleep"]', FALSE, FALSE, TRUE, FALSE, FALSE, 'Sleep from midnight to 7am.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 59) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0347', 'day_2025-12-31', '2025-12-31T13:00:00Z', '2025-12-31T14:30:00Z', 'Slow morning', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Slow morning on New Year''s Eve, coffee and planning the day.', '["routine", "morning", "coffee"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 66) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0348', 'day_2025-12-31', '2025-12-31T15:00:00Z', '2025-12-31T16:30:00Z', 'Walk at Lady Bird Lake', 'Lady Bird Lake', '["steps", "location_visit"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Late morning walk along Lady Bird Lake, reflecting on the year.', '["exercise", "outdoors", "reflection"]', '["place_demo_ladybird"]', NULL, NULL, NULL, 'NEW', 90) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0349', 'day_2025-12-31', '2025-12-31T17:00:00Z', '2025-12-31T19:00:00Z', 'Afternoon at home', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Tidied up the apartment and did some year-end journaling.', '["leisure", "reflection"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 77) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0350', 'day_2025-12-31', '2025-12-31T22:00:00Z', '2025-12-31T23:00:00Z', 'Getting ready', 'Home', '["location_visit"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Got ready and made appetizers to bring to Jess''s NYE party.', '["routine", "food"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 68) ON CONFLICT DO NOTHING;

-- NYE party at Jess's
INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0351', 'day_2025-12-31', '2026-01-01T00:00:00Z', '2026-01-01T06:30:00Z', 'New Year''s Eve at Jess''s', 'Jess''s Place', '["location_visit", "transcription"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'New Year''s Eve party at Jess''s with Priya and a few others, champagne at midnight.', '["social", "games"]', '["person_demo_jess", "person_demo_priya", "place_demo_jess"]', NULL, NULL, NULL, 'NEW', 74) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0352', 'day_2025-12-31', '2026-01-01T06:30:00Z', '2026-01-01T07:00:00Z', 'Wind down', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Got home around 12:30am and fell straight into bed.', '["leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 62) ON CONFLICT DO NOTHING;

-- =============================================================================
-- THURSDAY January 1, 2026 — New Year's Day (quiet recovery)
-- =============================================================================

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0353', 'day_2026-01-01', '2026-01-01T07:00:00Z', '2026-01-01T15:00:00Z', 'Sleep', 'Home', '["sleep"]', FALSE, FALSE, TRUE, FALSE, FALSE, 'Slept in after New Year''s Eve, 1am to 9am.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 57) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0354', 'day_2026-01-01', '2026-01-01T15:00:00Z', '2026-01-01T16:30:00Z', 'Slow morning', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Very slow New Year''s morning, coffee and toast.', '["routine", "morning", "coffee", "food"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 67) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0355', 'day_2026-01-01', '2026-01-01T17:00:00Z', '2026-01-01T18:00:00Z', 'Walk', 'Mueller Trails', '["steps"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Short New Year''s Day walk to get some fresh air.', '["exercise", "outdoors"]', '["place_demo_mueller_trails"]', NULL, NULL, NULL, 'NEW', 88) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0356', 'day_2026-01-01', '2026-01-01T18:30:00Z', '2026-01-01T22:00:00Z', 'Afternoon at home', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Spent the afternoon on the couch reading and writing New Year''s goals.', '["leisure", "reflection"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 62) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0357', 'day_2026-01-01', '2026-01-01T22:00:00Z', '2026-01-02T01:00:00Z', 'Dinner', 'Home', '["location_visit"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Made a simple dinner and watched the first episode of a new show.', '["food", "leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 68) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0358', 'day_2026-01-01', '2026-01-02T01:00:00Z', '2026-01-02T05:00:00Z', 'Evening', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Quiet evening, journaling about plans for the new year.', '["leisure", "reflection"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 62) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0359', 'day_2026-01-01', '2026-01-02T05:00:00Z', '2026-01-02T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Early to bed, ready to get back to normal.', '["leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 60) ON CONFLICT DO NOTHING;

-- =============================================================================
-- FRIDAY January 2, 2026 — Light WFH, game night at Jess's
-- =============================================================================

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0360', 'day_2026-01-02', '2026-01-02T06:00:00Z', '2026-01-02T12:30:00Z', 'Sleep', 'Home', '["sleep"]', FALSE, FALSE, TRUE, FALSE, FALSE, 'Sleep from midnight to 6:30am.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 57) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0361', 'day_2026-01-02', '2026-01-02T12:30:00Z', '2026-01-02T13:30:00Z', 'Morning routine', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Coffee and Slack, team starting to come back online.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 67) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0362', 'day_2026-01-02', '2026-01-02T14:00:00Z', '2026-01-02T17:00:00Z', 'WFH morning', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Eased back into work from home, reviewed Q1 priorities and cleared out email.', '["work", "onboarding"]', '["place_demo_home", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 67) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0363', 'day_2026-01-02', '2026-01-02T17:00:00Z', '2026-01-02T18:00:00Z', 'Lunch', 'Home', '["location_visit"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Leftover soup for lunch.', '["food"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 70) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0364', 'day_2026-01-02', '2026-01-02T18:00:00Z', '2026-01-02T20:00:00Z', 'WFH afternoon', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Caught up with David on Slack about a design spec, signed off early.', '["work", "messaging", "design-review"]', '["person_demo_david", "place_demo_home", "org_demo_employer"]', NULL, NULL, NULL, 'NEW', 67) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0365', 'day_2026-01-02', '2026-01-02T21:00:00Z', '2026-01-02T21:45:00Z', 'Phone call with Mom', 'Home', '["transcription"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Quick call with Mom, talked about how the holidays went.', '["family", "phone-call"]', '["person_demo_mom", "place_demo_home"]', NULL, NULL, NULL, 'NEW', 72) ON CONFLICT DO NOTHING;

-- Game night at Jess's
INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0366', 'day_2026-01-02', '2026-01-03T01:00:00Z', '2026-01-03T05:00:00Z', 'Game night', 'Jess''s Place', '["location_visit", "transcription"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'First game night of the new year at Jess''s, played Ticket to Ride with Priya.', '["social", "games"]', '["person_demo_jess", "person_demo_priya", "place_demo_jess"]', NULL, NULL, NULL, 'NEW', 68) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0367', 'day_2026-01-02', '2026-01-03T05:00:00Z', '2026-01-03T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Got home and went straight to bed.', '["leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 58) ON CONFLICT DO NOTHING;

-- =============================================================================
-- SATURDAY January 3, 2026 — Lady Bird Lake, errands, Mom call
-- =============================================================================

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0368', 'day_2026-01-03', '2026-01-03T06:00:00Z', '2026-01-03T13:30:00Z', 'Sleep', 'Home', '["sleep"]', FALSE, FALSE, TRUE, FALSE, FALSE, 'Slept in after game night, midnight to 7:30am.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 60) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0369', 'day_2026-01-03', '2026-01-03T13:30:00Z', '2026-01-03T15:00:00Z', 'Slow morning', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Saturday morning, coffee and reading the news.', '["routine", "morning", "coffee", "browsing"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 65) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0370', 'day_2026-01-03', '2026-01-03T15:30:00Z', '2026-01-03T17:00:00Z', 'Walk at Lady Bird Lake', 'Lady Bird Lake', '["steps", "location_visit"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Saturday walk at Lady Bird Lake, cool January morning.', '["exercise", "outdoors"]', '["place_demo_ladybird"]', NULL, NULL, NULL, 'NEW', 92) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0371', 'day_2026-01-03', '2026-01-03T17:30:00Z', '2026-01-03T19:30:00Z', 'Errands', NULL, '["location_visit"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Grocery shopping and picking up a few things for the new year.', '["leisure", "errands"]', '[]', NULL, NULL, NULL, 'NEW', 72) ON CONFLICT DO NOTHING;

-- Mom call (Saturday)
INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0372', 'day_2026-01-03', '2026-01-03T22:00:00Z', '2026-01-03T22:45:00Z', 'Phone call with Mom', 'Home', '["transcription"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Weekly call with Mom, she asked about New Year''s resolutions.', '["family", "phone-call"]', '["person_demo_mom", "place_demo_home"]', NULL, NULL, NULL, 'NEW', 68) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0373', 'day_2026-01-03', '2026-01-03T23:00:00Z', '2026-01-04T02:00:00Z', 'Evening at home', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Made a stir-fry for dinner and started a new book.', '["food", "leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 61) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0374', 'day_2026-01-03', '2026-01-04T02:00:00Z', '2026-01-04T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Watched a show and fell asleep on the couch.', '["leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 58) ON CONFLICT DO NOTHING;

-- =============================================================================
-- SUNDAY January 4, 2026 — Slow day, prep for back to normal
-- =============================================================================

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0375', 'day_2026-01-04', '2026-01-04T06:00:00Z', '2026-01-04T14:00:00Z', 'Sleep', 'Home', '["sleep"]', FALSE, FALSE, TRUE, FALSE, FALSE, 'Sleep from midnight to 8am.', '["sleep"]', '[]', NULL, NULL, NULL, 'NEW', 62) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0376', 'day_2026-01-04', '2026-01-04T14:00:00Z', '2026-01-04T15:30:00Z', 'Slow morning', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Lazy Sunday morning, coffee and reading.', '["routine", "morning", "coffee", "leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 63) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0377', 'day_2026-01-04', '2026-01-04T16:00:00Z', '2026-01-04T17:00:00Z', 'Run', 'Mueller Trails', '["steps", "workout"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Sunday morning run on Mueller trails, 3 miles to start the year right.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]', NULL, NULL, NULL, 'NEW', 157) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0378', 'day_2026-01-04', '2026-01-04T17:30:00Z', '2026-01-04T20:00:00Z', 'Cooking and meal prep', 'Home', '["location_visit"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Big Sunday meal prep — made soup, roasted vegetables, and prepped lunches.', '["food", "cooking"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 70) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0379', 'day_2026-01-04', '2026-01-04T20:00:00Z', '2026-01-05T00:00:00Z', 'Afternoon at home', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Read for a while and organized her desk for the week ahead.', '["leisure", "reflection"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 60) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0380', 'day_2026-01-04', '2026-01-05T00:00:00Z', '2026-01-05T03:00:00Z', 'Evening', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Made pasta for dinner and watched a movie, early night to get back on schedule.', '["food", "leisure"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 67) ON CONFLICT DO NOTHING;

INSERT INTO wiki_events (id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_unknown, is_transit, is_sleep, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr) VALUES
('ev_b0381', 'day_2026-01-04', '2026-01-05T03:00:00Z', '2026-01-05T06:00:00Z', 'Wind down', 'Home', '["app_usage"]', FALSE, FALSE, FALSE, FALSE, FALSE, 'Set alarms for Monday, read a few pages, and fell asleep.', '["leisure", "reflection"]', '["place_demo_home"]', NULL, NULL, NULL, 'NEW', 62) ON CONFLICT DO NOTHING;
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
-- (see header)
-- =============================================================================

-- ─────────────────────────────────────────────────────────────────────────────
-- CLEANUP
-- ─────────────────────────────────────────────────────────────────────────────
DELETE FROM wiki_events WHERE id LIKE 'ev_b0%' AND CAST(SUBSTR(id, 5) AS INTEGER) BETWEEN 421 AND 630;

-- ─────────────────────────────────────────────────────────────────────────────
-- WIKI DAYS
-- ─────────────────────────────────────────────────────────────────────────────
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2026-01-05', '2026-01-05', 'America/Chicago', 0.52) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2026-01-06', '2026-01-06', 'America/Chicago', 0.48) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2026-01-07', '2026-01-07', 'America/Chicago', 0.50) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2026-01-08', '2026-01-08', 'America/Chicago', 0.45) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2026-01-09', '2026-01-09', 'America/Chicago', 0.53) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2026-01-10', '2026-01-10', 'America/Chicago', 0.55) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2026-01-11', '2026-01-11', 'America/Chicago', 0.47) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2026-01-12', '2026-01-12', 'America/Chicago', 0.50) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2026-01-13', '2026-01-13', 'America/Chicago', 0.51) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2026-01-14', '2026-01-14', 'America/Chicago', 0.49) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2026-01-15', '2026-01-15', 'America/Chicago', 0.54) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2026-01-16', '2026-01-16', 'America/Chicago', 0.46) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2026-01-17', '2026-01-17', 'America/Chicago', 0.52) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2026-01-18', '2026-01-18', 'America/Chicago', 0.58) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2026-01-19', '2026-01-19', 'America/Chicago', 0.44) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2026-01-20', '2026-01-20', 'America/Chicago', 0.50) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2026-01-21', '2026-01-21', 'America/Chicago', 0.48) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2026-01-22', '2026-01-22', 'America/Chicago', 0.53) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2026-01-23', '2026-01-23', 'America/Chicago', 0.42) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2026-01-24', '2026-01-24', 'America/Chicago', 0.56) ON CONFLICT DO NOTHING;
INSERT INTO wiki_days (id, date, start_timezone, morning_baseline) VALUES ('day_2026-01-25', '2026-01-25', 'America/Chicago', 0.49) ON CONFLICT DO NOTHING;

-- ─────────────────────────────────────────────────────────────────────────────
-- WIKI EVENTS
-- ─────────────────────────────────────────────────────────────────────────────

-- =============================================================================
-- WEEK 7: January 5 (Mon) - January 11 (Sun)
-- =============================================================================

-- ── Monday, January 5, 2026 ─────────────────────────────────────────────────

-- Sleep (00:00-06:30 CST = 06:00-12:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0421', 'day_2026-01-05',
    '2026-01-05T06:00:00Z', '2026-01-05T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Sleep from midnight to 6:30am, about 6.5 hours.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 56
) ON CONFLICT DO NOTHING;

-- Morning routine (06:30-07:15 CST = 12:30-13:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0422', 'day_2026-01-05',
    '2026-01-05T12:30:00Z', '2026-01-05T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Coffee, checked Slack and email to catch up after the weekend.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 67
) ON CONFLICT DO NOTHING;

-- Bike commute (07:15-07:45 CST = 13:15-13:45 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0423', 'day_2026-01-05',
    '2026-01-05T13:15:00Z', '2026-01-05T13:45:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Bike commute to the office, cold morning but sunny.', '["commute", "cycling", "podcast"]', '[]',
    NULL, NULL, NULL, 'NEW', 128
) ON CONFLICT DO NOTHING;

-- Coffee and Slack (07:45-08:15 CST = 13:45-14:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0424', 'day_2026-01-05',
    '2026-01-05T13:45:00Z', '2026-01-05T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Grabbed coffee at the office and caught up on Slack threads from the holiday break.', '["messaging", "work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 72
) ON CONFLICT DO NOTHING;

-- Standup (08:15-09:00 CST = 14:15-15:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0425', 'day_2026-01-05',
    '2026-01-05T14:15:00Z', '2026-01-05T15:00:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'First standup of the new year with Maya and David, reviewed Q1 priorities.', '["meeting", "standup", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 77
) ON CONFLICT DO NOTHING;

-- Focused design work (09:00-11:30 CST = 15:00-17:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0426', 'day_2026-01-05',
    '2026-01-05T15:00:00Z', '2026-01-05T17:30:00Z',
    'Focused design work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Long Figma session working on the settings page redesign.', '["design", "figma", "focus", "deep-work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 69
) ON CONFLICT DO NOTHING;

-- Lunch solo (11:30-12:15 CST = 17:30-18:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0427', 'day_2026-01-05',
    '2026-01-05T17:30:00Z', '2026-01-05T18:15:00Z',
    'Lunch', 'Office', '["location_visit"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Quick solo lunch at desk, leftover soup from the weekend.', '["food"]', '["place_demo_office"]',
    NULL, NULL, NULL, 'NEW', 66
) ON CONFLICT DO NOTHING;

-- Afternoon work block (12:15-16:30 CST = 18:15-22:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0428', 'day_2026-01-05',
    '2026-01-05T18:15:00Z', '2026-01-05T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Continued on settings page wireframes, responded to Slack threads about Q1 roadmap.', '["design", "figma", "work", "messaging"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 69
) ON CONFLICT DO NOTHING;

-- Bike commute home (16:30-17:00 CST = 22:30-23:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0429', 'day_2026-01-05',
    '2026-01-05T22:30:00Z', '2026-01-05T23:00:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Bike ride home from the office.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 130
) ON CONFLICT DO NOTHING;

-- Evening run (17:30-18:15 CST = 23:30-00:15+1 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0430', 'day_2026-01-05',
    '2026-01-05T23:30:00Z', '2026-01-06T00:15:00Z',
    'Evening run', 'Mueller Trails', '["steps", "workout"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Short 3-mile run on Mueller trails to shake off the Monday sluggishness.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
    NULL, NULL, NULL, 'NEW', 157
) ON CONFLICT DO NOTHING;

-- Dinner and reading (18:30-22:00 CST = 00:30-04:00+1 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0431', 'day_2026-01-05',
    '2026-01-06T00:30:00Z', '2026-01-06T04:00:00Z',
    'Dinner and reading', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Made stir fry for dinner then read on the couch for a couple hours.', '["food", "leisure"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 59
) ON CONFLICT DO NOTHING;

-- ── Tuesday, January 6, 2026 ────────────────────────────────────────────────

-- Sleep (00:00-06:15 CST = 06:00-12:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0432', 'day_2026-01-06',
    '2026-01-06T06:00:00Z', '2026-01-06T12:15:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Sleep from midnight to about 6:15am.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 58
) ON CONFLICT DO NOTHING;

-- Morning routine (06:15-07:10 CST = 12:15-13:10 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0433', 'day_2026-01-06',
    '2026-01-06T12:15:00Z', '2026-01-06T13:10:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Morning coffee, scrolled through texts, got ready for the day.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 64
) ON CONFLICT DO NOTHING;

-- Bike commute (07:10-07:40 CST = 13:10-13:40 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0434', 'day_2026-01-06',
    '2026-01-06T13:10:00Z', '2026-01-06T13:40:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Bike commute to office, chilly but clear.', '["commute", "cycling", "podcast"]', '[]',
    NULL, NULL, NULL, 'NEW', 118
) ON CONFLICT DO NOTHING;

-- Coffee and Slack (07:40-08:15 CST = 13:40-14:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0435', 'day_2026-01-06',
    '2026-01-06T13:40:00Z', '2026-01-06T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Coffee at the office, caught up on overnight Slack messages.', '["messaging", "work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 64
) ON CONFLICT DO NOTHING;

-- Standup + design review with David (08:15-09:30 CST = 14:15-15:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0436', 'day_2026-01-06',
    '2026-01-06T14:15:00Z', '2026-01-06T15:30:00Z',
    'Standup and design review', 'Office', '["calendar", "message", "transcription"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Standup followed by design review with David on the settings page component library.', '["meeting", "standup", "design", "design-review"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 77
) ON CONFLICT DO NOTHING;

-- Focused work (09:30-11:30 CST = 15:30-17:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0437', 'day_2026-01-06',
    '2026-01-06T15:30:00Z', '2026-01-06T17:30:00Z',
    'Focused design work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Deep work on settings page Figma prototypes, refining interaction flows.', '["design", "figma", "focus", "deep-work", "navigation"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 66
) ON CONFLICT DO NOTHING;

-- Lunch solo (11:30-12:15 CST = 17:30-18:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0438', 'day_2026-01-06',
    '2026-01-06T17:30:00Z', '2026-01-06T18:15:00Z',
    'Lunch', 'Office', '["location_visit"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Solo lunch at the office, sandwich from the deli downstairs.', '["food"]', '["place_demo_office"]',
    NULL, NULL, NULL, 'NEW', 70
) ON CONFLICT DO NOTHING;

-- Afternoon work (12:15-16:30 CST = 18:15-22:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0439', 'day_2026-01-06',
    '2026-01-06T18:15:00Z', '2026-01-06T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Worked on component specs and documentation for the settings redesign.', '["design", "figma", "work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 67
) ON CONFLICT DO NOTHING;

-- Bike commute home (16:30-17:00 CST = 22:30-23:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0440', 'day_2026-01-06',
    '2026-01-06T22:30:00Z', '2026-01-06T23:00:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked home from the office.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 130
) ON CONFLICT DO NOTHING;

-- Evening run on Mueller trails (17:30-18:20 CST = 23:30-00:20+1 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0441', 'day_2026-01-06',
    '2026-01-06T23:30:00Z', '2026-01-07T00:20:00Z',
    'Evening run', 'Mueller Trails', '["steps", "workout"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Tuesday evening run on Mueller trails, 3.5 miles.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
    NULL, NULL, NULL, 'NEW', 151
) ON CONFLICT DO NOTHING;

-- Dinner and TV (19:00-22:00 CST = 01:00-04:00+1 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0442', 'day_2026-01-06',
    '2026-01-07T01:00:00Z', '2026-01-07T04:00:00Z',
    'Dinner and TV', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Cooked pasta for dinner and watched a couple episodes of a documentary series.', '["food", "leisure"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 66
) ON CONFLICT DO NOTHING;

-- ── Wednesday, January 7, 2026 ──────────────────────────────────────────────

-- Sleep (00:00-06:30 CST = 06:00-12:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0443', 'day_2026-01-07',
    '2026-01-07T06:00:00Z', '2026-01-07T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Slept from midnight to 6:30am, 6.5 hours.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 59
) ON CONFLICT DO NOTHING;

-- Morning routine (06:30-07:15 CST = 12:30-13:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0444', 'day_2026-01-07',
    '2026-01-07T12:30:00Z', '2026-01-07T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Morning coffee and caught up on texts from Jess about Friday plans.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 63
) ON CONFLICT DO NOTHING;

-- Bike commute (07:15-07:45 CST = 13:15-13:45 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0445', 'day_2026-01-07',
    '2026-01-07T13:15:00Z', '2026-01-07T13:45:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked to office, overcast day.', '["commute", "cycling", "podcast"]', '[]',
    NULL, NULL, NULL, 'NEW', 116
) ON CONFLICT DO NOTHING;

-- Coffee and Slack (07:45-08:15 CST = 13:45-14:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0446', 'day_2026-01-07',
    '2026-01-07T13:45:00Z', '2026-01-07T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Office coffee and Slack catch-up before standup.', '["messaging", "work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 72
) ON CONFLICT DO NOTHING;

-- Standup (08:15-08:45 CST = 14:15-14:45 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0447', 'day_2026-01-07',
    '2026-01-07T14:15:00Z', '2026-01-07T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Quick standup with Maya and David, everyone aligned on settings page progress.', '["meeting", "standup", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 73
) ON CONFLICT DO NOTHING;

-- Focused work (08:45-11:30 CST = 14:45-17:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0448', 'day_2026-01-07',
    '2026-01-07T14:45:00Z', '2026-01-07T17:30:00Z',
    'Focused design work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Deep work in Figma on settings page interaction patterns.', '["design", "figma", "focus", "deep-work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 64
) ON CONFLICT DO NOTHING;

-- Lunch with Maya at Tatsu-ya (11:30-12:30 CST = 17:30-18:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0449', 'day_2026-01-07',
    '2026-01-07T17:30:00Z', '2026-01-07T18:30:00Z',
    'Lunch with Maya', 'Ramen Tatsu-ya', '["location_visit", "transcription"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Weekly lunch at Ramen Tatsu-ya with Maya, talked about team goals for Q1.', '["social", "food", "ramen"]', '["person_demo_maya", "place_demo_ramen"]',
    NULL, NULL, NULL, 'NEW', 75
) ON CONFLICT DO NOTHING;

-- Afternoon work (12:30-16:30 CST = 18:30-22:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0450', 'day_2026-01-07',
    '2026-01-07T18:30:00Z', '2026-01-07T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Continued on settings page, wrote up spec notes for David to implement.', '["design", "figma", "work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 66
) ON CONFLICT DO NOTHING;

-- Bike commute home (16:30-17:00 CST = 22:30-23:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0451', 'day_2026-01-07',
    '2026-01-07T22:30:00Z', '2026-01-07T23:00:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked home, stopped to pick up groceries on the way.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 112
) ON CONFLICT DO NOTHING;

-- Dinner and browsing (18:00-22:00 CST = 00:00-04:00+1 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0452', 'day_2026-01-07',
    '2026-01-08T00:00:00Z', '2026-01-08T04:00:00Z',
    'Dinner and browsing', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Made a salad for dinner, then browsed apartment listings and read online.', '["food", "leisure", "browsing", "house-hunting"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 68
) ON CONFLICT DO NOTHING;

-- ── Thursday, January 8, 2026 — RACHEL FIRST CONTACT ────────────────────────

-- Sleep (00:00-06:20 CST = 06:00-12:20 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0453', 'day_2026-01-08',
    '2026-01-08T06:00:00Z', '2026-01-08T12:20:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Slept from midnight to about 6:20am.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 62
) ON CONFLICT DO NOTHING;

-- Morning routine (06:20-07:10 CST = 12:20-13:10 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0454', 'day_2026-01-08',
    '2026-01-08T12:20:00Z', '2026-01-08T13:10:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Coffee and morning routine, checked email — saw a message from a realtor named Rachel Torres.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 68
) ON CONFLICT DO NOTHING;

-- Bike commute (07:10-07:40 CST = 13:10-13:40 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0455', 'day_2026-01-08',
    '2026-01-08T13:10:00Z', '2026-01-08T13:40:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Bike commute to the office.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 133
) ON CONFLICT DO NOTHING;

-- Coffee and Slack (07:40-08:15 CST = 13:40-14:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0456', 'day_2026-01-08',
    '2026-01-08T13:40:00Z', '2026-01-08T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Coffee at the office, caught up on Slack.', '["messaging", "work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 69
) ON CONFLICT DO NOTHING;

-- Standup (08:15-08:45 CST = 14:15-14:45 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0457', 'day_2026-01-08',
    '2026-01-08T14:15:00Z', '2026-01-08T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Standup with Maya and David, discussed design review feedback from Tuesday.', '["meeting", "standup", "design", "design-review"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 72
) ON CONFLICT DO NOTHING;

-- Focused work (08:45-11:30 CST = 14:45-17:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0458', 'day_2026-01-08',
    '2026-01-08T14:45:00Z', '2026-01-08T17:30:00Z',
    'Focused design work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Worked on settings page prototypes in Figma.', '["design", "figma", "focus", "deep-work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 69
) ON CONFLICT DO NOTHING;

-- Lunch solo (11:30-12:15 CST = 17:30-18:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0459', 'day_2026-01-08',
    '2026-01-08T17:30:00Z', '2026-01-08T18:15:00Z',
    'Lunch', 'Office', '["location_visit"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Ate lunch at the office, packed leftovers.', '["food"]', '["place_demo_office"]',
    NULL, NULL, NULL, 'NEW', 71
) ON CONFLICT DO NOTHING;

-- Afternoon work (12:15-16:00 CST = 18:15-22:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0460', 'day_2026-01-08',
    '2026-01-08T18:15:00Z', '2026-01-08T22:00:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Wrapped up a Figma prototype and shared it in the design channel for async feedback.', '["design", "figma", "work", "messaging"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 64
) ON CONFLICT DO NOTHING;

-- Bike commute home (16:00-16:30 CST = 22:00-22:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0461', 'day_2026-01-08',
    '2026-01-08T22:00:00Z', '2026-01-08T22:30:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked home from the office, left a bit early.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 133
) ON CONFLICT DO NOTHING;

-- ** RACHEL FIRST CONTACT ** Phone call (17:00-17:20 CST = 23:00-23:20 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0462', 'day_2026-01-08',
    '2026-01-08T23:00:00Z', '2026-01-08T23:20:00Z',
    'Phone call with Rachel Torres', 'Home', '["message", "transcription"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Rachel Torres from Torres Realty called about house hunting — she has some listings in East Austin and Bouldin Creek she thinks would be a good fit.', '["phone-call", "house-hunting", "real-estate"]', '["person_demo_rachel", "org_demo_realty", "place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 65
) ON CONFLICT DO NOTHING;

-- Walk on Mueller trails (17:30-18:15 CST = 23:30-00:15+1 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0463', 'day_2026-01-08',
    '2026-01-08T23:30:00Z', '2026-01-09T00:15:00Z',
    'Walk', 'Mueller Trails', '["steps"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Went for an evening walk on Mueller trails, thinking about whether to seriously start house hunting.', '["exercise", "outdoors", "mueller-trails", "house-hunting"]', '["place_demo_mueller_trails"]',
    NULL, NULL, NULL, 'NEW', 148
) ON CONFLICT DO NOTHING;

-- Dinner and evening (19:00-22:00 CST = 01:00-04:00+1 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0464', 'day_2026-01-08',
    '2026-01-09T01:00:00Z', '2026-01-09T04:00:00Z',
    'Dinner and browsing', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Made tacos for dinner, then browsed Zillow looking at East Austin listings Rachel mentioned.', '["food", "leisure", "browsing", "house-hunting", "real-estate"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 61
) ON CONFLICT DO NOTHING;

-- ── Friday, January 9, 2026 — Game night at Jess's ─────────────────────────

-- Sleep (00:00-06:30 CST = 06:00-12:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0465', 'day_2026-01-09',
    '2026-01-09T06:00:00Z', '2026-01-09T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Slept from midnight to 6:30am.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 58
) ON CONFLICT DO NOTHING;

-- Morning routine (06:30-07:15 CST = 12:30-13:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0466', 'day_2026-01-09',
    '2026-01-09T12:30:00Z', '2026-01-09T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Coffee and morning routine, texted Jess to confirm game night tonight.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 66
) ON CONFLICT DO NOTHING;

-- Bike commute (07:15-07:45 CST = 13:15-13:45 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0467', 'day_2026-01-09',
    '2026-01-09T13:15:00Z', '2026-01-09T13:45:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked to the office on a crisp Friday morning.', '["commute", "cycling", "podcast"]', '[]',
    NULL, NULL, NULL, 'NEW', 135
) ON CONFLICT DO NOTHING;

-- Coffee and Slack (07:45-08:15 CST = 13:45-14:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0468', 'day_2026-01-09',
    '2026-01-09T13:45:00Z', '2026-01-09T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Coffee and Slack at the office.', '["messaging", "work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 68
) ON CONFLICT DO NOTHING;

-- Standup (08:15-08:45 CST = 14:15-14:45 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0469', 'day_2026-01-09',
    '2026-01-09T14:15:00Z', '2026-01-09T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Friday standup — reviewed the week and Maya mentioned the onboarding funnel might become a priority soon.', '["meeting", "standup", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 78
) ON CONFLICT DO NOTHING;

-- Focused work (08:45-11:30 CST = 14:45-17:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0470', 'day_2026-01-09',
    '2026-01-09T14:45:00Z', '2026-01-09T17:30:00Z',
    'Focused design work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Wrapped up the settings page first draft, tidied up layers in Figma.', '["design", "figma", "focus"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 67
) ON CONFLICT DO NOTHING;

-- Lunch solo (11:30-12:15 CST = 17:30-18:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0471', 'day_2026-01-09',
    '2026-01-09T17:30:00Z', '2026-01-09T18:15:00Z',
    'Lunch', 'Office', '["location_visit"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Quick lunch at the office before heading out for a shorter Friday afternoon.', '["food"]', '["place_demo_office"]',
    NULL, NULL, NULL, 'NEW', 64
) ON CONFLICT DO NOTHING;

-- Afternoon work — shorter Friday (12:15-15:30 CST = 18:15-21:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0472', 'day_2026-01-09',
    '2026-01-09T18:15:00Z', '2026-01-09T21:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Light Friday afternoon — cleaned up design files and responded to a few PRs.', '["work", "design", "figma", "code-review"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 66
) ON CONFLICT DO NOTHING;

-- Bike commute home (15:30-16:00 CST = 21:30-22:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0473', 'day_2026-01-09',
    '2026-01-09T21:30:00Z', '2026-01-09T22:00:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked home early on Friday.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 124
) ON CONFLICT DO NOTHING;

-- Mom call (17:00-17:40 CST = 23:00-23:40 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0474', 'day_2026-01-09',
    '2026-01-09T23:00:00Z', '2026-01-09T23:40:00Z',
    'Phone call with Mom', 'Home', '["message", "transcription"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Weekly call with Mom, caught up on her week and mentioned the realtor who reached out.', '["family", "phone-call"]', '["person_demo_mom", "place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 66
) ON CONFLICT DO NOTHING;

-- Game night at Jess's (19:00-23:00 CST = 01:00-05:00+1 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0475', 'day_2026-01-09',
    '2026-01-10T01:00:00Z', '2026-01-10T05:00:00Z',
    'Game night', 'Jess''s Place', '["location_visit", "transcription"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Game night at Jess''s with Priya — played Catan and Ticket to Ride, ordered pizza.', '["social", "games", "food"]', '["person_demo_jess", "person_demo_priya", "place_demo_jess"]',
    NULL, NULL, NULL, 'NEW', 74
) ON CONFLICT DO NOTHING;

-- ── Saturday, January 10, 2026 ──────────────────────────────────────────────

-- Sleep (01:00-08:00 CST = 07:00-14:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0476', 'day_2026-01-10',
    '2026-01-10T07:00:00Z', '2026-01-10T14:00:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Slept in after game night, about 7 hours.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 57
) ON CONFLICT DO NOTHING;

-- Slow morning (08:00-09:30 CST = 14:00-15:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0477', 'day_2026-01-10',
    '2026-01-10T14:00:00Z', '2026-01-10T15:30:00Z',
    'Slow morning', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Lazy Saturday morning, coffee on the couch, scrolled through Instagram.', '["routine", "morning", "coffee", "browsing"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 64
) ON CONFLICT DO NOTHING;

-- Lady Bird Lake walk (10:00-11:30 CST = 16:00-17:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0478', 'day_2026-01-10',
    '2026-01-10T16:00:00Z', '2026-01-10T17:30:00Z',
    'Lady Bird Lake walk', 'Lady Bird Lake', '["steps", "location_visit"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Walked the boardwalk loop at Lady Bird Lake, cool but pleasant morning.', '["exercise", "outdoors", "walking"]', '["place_demo_ladybird"]',
    NULL, NULL, NULL, 'NEW', 92
) ON CONFLICT DO NOTHING;

-- Errands and lunch (11:30-13:30 CST = 17:30-19:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0479', 'day_2026-01-10',
    '2026-01-10T17:30:00Z', '2026-01-10T19:30:00Z',
    'Errands and lunch', NULL, '["location_visit"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Ran errands at HEB, grabbed a taco from a food truck on the way home.', '["food"]', '[]',
    NULL, NULL, NULL, 'NEW', 73
) ON CONFLICT DO NOTHING;

-- Afternoon reading (14:00-17:00 CST = 20:00-23:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0480', 'day_2026-01-10',
    '2026-01-10T20:00:00Z', '2026-01-10T23:00:00Z',
    'Reading', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Spent the afternoon reading and doing a bit of journaling.', '["leisure", "reflection"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 58
) ON CONFLICT DO NOTHING;

-- Dinner and movie (18:00-22:00 CST = 00:00-04:00+1 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0481', 'day_2026-01-10',
    '2026-01-11T00:00:00Z', '2026-01-11T04:00:00Z',
    'Dinner and movie', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Cooked a stir fry for dinner and watched a movie at home.', '["food", "leisure"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 68
) ON CONFLICT DO NOTHING;

-- ── Sunday, January 11, 2026 ────────────────────────────────────────────────

-- Sleep (00:00-08:00 CST = 06:00-14:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0482', 'day_2026-01-11',
    '2026-01-11T06:00:00Z', '2026-01-11T14:00:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Slept in on Sunday, about 8 hours.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 55
) ON CONFLICT DO NOTHING;

-- Slow morning (08:00-09:30 CST = 14:00-15:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0483', 'day_2026-01-11',
    '2026-01-11T14:00:00Z', '2026-01-11T15:30:00Z',
    'Slow morning', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Quiet Sunday morning with coffee and the NYT crossword.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 67
) ON CONFLICT DO NOTHING;

-- Mueller trails run (09:30-10:30 CST = 15:30-16:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0484', 'day_2026-01-11',
    '2026-01-11T15:30:00Z', '2026-01-11T16:30:00Z',
    'Morning run', 'Mueller Trails', '["steps", "workout"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Sunday morning run on Mueller trails, 4 miles.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
    NULL, NULL, NULL, 'NEW', 68
) ON CONFLICT DO NOTHING;

-- Cooking and meal prep (11:00-13:00 CST = 17:00-19:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0485', 'day_2026-01-11',
    '2026-01-11T17:00:00Z', '2026-01-11T19:00:00Z',
    'Cooking', 'Home', '["location_visit"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Meal prepped soup and grain bowls for the week ahead.', '["food"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 72
) ON CONFLICT DO NOTHING;

-- Afternoon reading and browsing (13:00-17:00 CST = 19:00-23:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0486', 'day_2026-01-11',
    '2026-01-11T19:00:00Z', '2026-01-11T23:00:00Z',
    'Reading and browsing', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Read a design book for a while, then browsed some articles about onboarding UX patterns.', '["leisure", "browsing", "onboarding"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 62
) ON CONFLICT DO NOTHING;

-- Dinner and wind down (18:00-22:00 CST = 00:00-04:00+1 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0487', 'day_2026-01-11',
    '2026-01-12T00:00:00Z', '2026-01-12T04:00:00Z',
    'Dinner and wind down', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Simple dinner from the meal prep, then watched TV and got ready for the week.', '["food", "leisure"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 68
) ON CONFLICT DO NOTHING;

-- =============================================================================
-- WEEK 8: January 12 (Mon) - January 18 (Sun) — Onboarding project ramps up
-- =============================================================================

-- ── Monday, January 12, 2026 ────────────────────────────────────────────────

-- Sleep (00:00-06:30 CST = 06:00-12:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0488', 'day_2026-01-12',
    '2026-01-12T06:00:00Z', '2026-01-12T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Sleep from midnight to 6:30am.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 62
) ON CONFLICT DO NOTHING;

-- Morning routine (06:30-07:15 CST = 12:30-13:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0489', 'day_2026-01-12',
    '2026-01-12T12:30:00Z', '2026-01-12T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Coffee and morning routine, saw a Slack message from Maya about the onboarding project kickoff this week.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 67
) ON CONFLICT DO NOTHING;

-- Bike commute (07:15-07:45 CST = 13:15-13:45 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0490', 'day_2026-01-12',
    '2026-01-12T13:15:00Z', '2026-01-12T13:45:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked to the office.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 120
) ON CONFLICT DO NOTHING;

-- Coffee and Slack (07:45-08:15 CST = 13:45-14:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0491', 'day_2026-01-12',
    '2026-01-12T13:45:00Z', '2026-01-12T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Coffee and Slack, read the brief for the onboarding funnel redesign project.', '["messaging", "work", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 71
) ON CONFLICT DO NOTHING;

-- Standup + onboarding kickoff (08:15-09:30 CST = 14:15-15:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0492', 'day_2026-01-12',
    '2026-01-12T14:15:00Z', '2026-01-12T15:30:00Z',
    'Standup and onboarding kickoff', 'Office', '["calendar", "message", "transcription"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Standup followed by onboarding redesign kickoff meeting with Maya and David — reviewed funnel metrics and drop-off points.', '["meeting", "standup", "design", "onboarding"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 71
) ON CONFLICT DO NOTHING;

-- Focused work on onboarding audit (09:30-11:30 CST = 15:30-17:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0493', 'day_2026-01-12',
    '2026-01-12T15:30:00Z', '2026-01-12T17:30:00Z',
    'Onboarding audit', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Started auditing the current onboarding flow in Figma, mapping out every screen and drop-off point.', '["design", "figma", "focus", "deep-work", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 64
) ON CONFLICT DO NOTHING;

-- Lunch solo (11:30-12:15 CST = 17:30-18:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0494', 'day_2026-01-12',
    '2026-01-12T17:30:00Z', '2026-01-12T18:15:00Z',
    'Lunch', 'Office', '["location_visit"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Ate the grain bowl from Sunday meal prep at my desk.', '["food"]', '["place_demo_office"]',
    NULL, NULL, NULL, 'NEW', 70
) ON CONFLICT DO NOTHING;

-- Afternoon work (12:15-16:30 CST = 18:15-22:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0495', 'day_2026-01-12',
    '2026-01-12T18:15:00Z', '2026-01-12T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Continued the onboarding audit and started collecting competitor screenshots.', '["design", "figma", "work", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 72
) ON CONFLICT DO NOTHING;

-- Bike commute home (16:30-17:00 CST = 22:30-23:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0496', 'day_2026-01-12',
    '2026-01-12T22:30:00Z', '2026-01-12T23:00:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked home from the office.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 112
) ON CONFLICT DO NOTHING;

-- Evening — dinner and reading (18:00-22:00 CST = 00:00-04:00+1 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0497', 'day_2026-01-12',
    '2026-01-13T00:00:00Z', '2026-01-13T04:00:00Z',
    'Dinner and reading', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Made soup from the meal prep batch, spent the evening reading.', '["food", "leisure"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 64
) ON CONFLICT DO NOTHING;

-- ── Tuesday, January 13, 2026 ───────────────────────────────────────────────

-- Sleep (00:00-06:20 CST = 06:00-12:20 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0498', 'day_2026-01-13',
    '2026-01-13T06:00:00Z', '2026-01-13T12:20:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Slept from midnight to 6:20am.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 56
) ON CONFLICT DO NOTHING;

-- Morning routine (06:20-07:10 CST = 12:20-13:10 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0499', 'day_2026-01-13',
    '2026-01-13T12:20:00Z', '2026-01-13T13:10:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Coffee, quick check of email and Slack.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 68
) ON CONFLICT DO NOTHING;

-- Bike commute (07:10-07:40 CST = 13:10-13:40 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0500', 'day_2026-01-13',
    '2026-01-13T13:10:00Z', '2026-01-13T13:40:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked to the office.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 111
) ON CONFLICT DO NOTHING;

-- Coffee and Slack (07:40-08:15 CST = 13:40-14:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0501', 'day_2026-01-13',
    '2026-01-13T13:40:00Z', '2026-01-13T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Coffee and Slack catch-up.', '["messaging", "work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 68
) ON CONFLICT DO NOTHING;

-- Standup + design review (08:15-09:30 CST = 14:15-15:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0502', 'day_2026-01-13',
    '2026-01-13T14:15:00Z', '2026-01-13T15:30:00Z',
    'Standup and design review', 'Office', '["calendar", "message", "transcription"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Standup then design review with David on the onboarding audit findings so far.', '["meeting", "standup", "design", "design-review", "onboarding"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 75
) ON CONFLICT DO NOTHING;

-- Focused work (09:30-11:30 CST = 15:30-17:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0503', 'day_2026-01-13',
    '2026-01-13T15:30:00Z', '2026-01-13T17:30:00Z',
    'Focused design work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Deep work mapping the onboarding user journey and identifying friction points.', '["design", "figma", "focus", "deep-work", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 70
) ON CONFLICT DO NOTHING;

-- Lunch solo (11:30-12:15 CST = 17:30-18:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0504', 'day_2026-01-13',
    '2026-01-13T17:30:00Z', '2026-01-13T18:15:00Z',
    'Lunch', 'Office', '["location_visit"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Lunch at the office, leftover grain bowl.', '["food"]', '["place_demo_office"]',
    NULL, NULL, NULL, 'NEW', 68
) ON CONFLICT DO NOTHING;

-- Afternoon work (12:15-16:30 CST = 18:15-22:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0505', 'day_2026-01-13',
    '2026-01-13T18:15:00Z', '2026-01-13T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Worked on wireframe sketches for the new onboarding flow and shared them in the design channel.', '["design", "figma", "work", "onboarding", "messaging"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 71
) ON CONFLICT DO NOTHING;

-- Bike commute home (16:30-17:00 CST = 22:30-23:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0506', 'day_2026-01-13',
    '2026-01-13T22:30:00Z', '2026-01-13T23:00:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked home.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 111
) ON CONFLICT DO NOTHING;

-- Evening run (17:30-18:20 CST = 23:30-00:20+1 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0507', 'day_2026-01-13',
    '2026-01-13T23:30:00Z', '2026-01-14T00:20:00Z',
    'Evening run', 'Mueller Trails', '["steps", "workout"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Tuesday evening run on Mueller trails, 3 miles.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
    NULL, NULL, NULL, 'NEW', 154
) ON CONFLICT DO NOTHING;

-- Dinner and TV (19:00-22:00 CST = 01:00-04:00+1 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0508', 'day_2026-01-13',
    '2026-01-14T01:00:00Z', '2026-01-14T04:00:00Z',
    'Dinner and TV', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Quick dinner then watched a couple episodes of a series.', '["food", "leisure"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 72
) ON CONFLICT DO NOTHING;

-- ── Wednesday, January 14, 2026 ─────────────────────────────────────────────

-- Sleep (00:00-06:30 CST = 06:00-12:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0509', 'day_2026-01-14',
    '2026-01-14T06:00:00Z', '2026-01-14T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Slept midnight to 6:30am, about 6.5 hours.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 55
) ON CONFLICT DO NOTHING;

-- Morning routine (06:30-07:15 CST = 12:30-13:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0510', 'day_2026-01-14',
    '2026-01-14T12:30:00Z', '2026-01-14T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Morning coffee and Slack check.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 63
) ON CONFLICT DO NOTHING;

-- Bike commute (07:15-07:45 CST = 13:15-13:45 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0511', 'day_2026-01-14',
    '2026-01-14T13:15:00Z', '2026-01-14T13:45:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked to the office, warmer than usual for January.', '["commute", "cycling", "podcast"]', '[]',
    NULL, NULL, NULL, 'NEW', 124
) ON CONFLICT DO NOTHING;

-- Coffee and Slack (07:45-08:15 CST = 13:45-14:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0512', 'day_2026-01-14',
    '2026-01-14T13:45:00Z', '2026-01-14T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Coffee and caught up on Slack, David had feedback on the onboarding wireframes.', '["messaging", "work", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 65
) ON CONFLICT DO NOTHING;

-- Standup (08:15-08:45 CST = 14:15-14:45 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0513', 'day_2026-01-14',
    '2026-01-14T14:15:00Z', '2026-01-14T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Standup — focused on onboarding redesign progress and next steps.', '["meeting", "standup", "design", "onboarding"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 71
) ON CONFLICT DO NOTHING;

-- Focused work (08:45-11:30 CST = 14:45-17:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0514', 'day_2026-01-14',
    '2026-01-14T14:45:00Z', '2026-01-14T17:30:00Z',
    'Focused design work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Iterated on onboarding wireframes in Figma based on David''s feedback.', '["design", "figma", "focus", "deep-work", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 66
) ON CONFLICT DO NOTHING;

-- Lunch with Maya at Tatsu-ya (11:30-12:30 CST = 17:30-18:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0515', 'day_2026-01-14',
    '2026-01-14T17:30:00Z', '2026-01-14T18:30:00Z',
    'Lunch with Maya', 'Ramen Tatsu-ya', '["location_visit", "transcription"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Lunch at Ramen Tatsu-ya with Maya, talked about the onboarding project scope and user research plans.', '["social", "food", "ramen", "onboarding"]', '["person_demo_maya", "place_demo_ramen"]',
    NULL, NULL, NULL, 'NEW', 71
) ON CONFLICT DO NOTHING;

-- Afternoon work (12:30-16:30 CST = 18:30-22:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0516', 'day_2026-01-14',
    '2026-01-14T18:30:00Z', '2026-01-14T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Continued refining onboarding wireframes, started a user research plan doc.', '["design", "figma", "work", "onboarding", "research"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 65
) ON CONFLICT DO NOTHING;

-- Bike commute home (16:30-17:00 CST = 22:30-23:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0517', 'day_2026-01-14',
    '2026-01-14T22:30:00Z', '2026-01-14T23:00:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked home from the office.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 127
) ON CONFLICT DO NOTHING;

-- Evening walk and dinner (17:30-22:00 CST = 23:30-04:00+1 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0518', 'day_2026-01-14',
    '2026-01-14T23:30:00Z', '2026-01-15T04:00:00Z',
    'Evening walk and dinner', 'Home', '["steps", "app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Short walk around the neighborhood, then made pasta and read before bed.', '["exercise", "outdoors", "walking", "food", "leisure"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 67
) ON CONFLICT DO NOTHING;

-- ── Thursday, January 15, 2026 — WFH afternoon ─────────────────────────────

-- Sleep (00:00-06:15 CST = 06:00-12:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0519', 'day_2026-01-15',
    '2026-01-15T06:00:00Z', '2026-01-15T12:15:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Slept from midnight to about 6:15am.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 60
) ON CONFLICT DO NOTHING;

-- Morning routine (06:15-07:10 CST = 12:15-13:10 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0520', 'day_2026-01-15',
    '2026-01-15T12:15:00Z', '2026-01-15T13:10:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Coffee, morning routine, checked messages.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 64
) ON CONFLICT DO NOTHING;

-- Bike commute (07:10-07:40 CST = 13:10-13:40 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0521', 'day_2026-01-15',
    '2026-01-15T13:10:00Z', '2026-01-15T13:40:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked to the office.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 123
) ON CONFLICT DO NOTHING;

-- Coffee and Slack (07:40-08:15 CST = 13:40-14:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0522', 'day_2026-01-15',
    '2026-01-15T13:40:00Z', '2026-01-15T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Coffee and Slack at the office.', '["messaging", "work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 70
) ON CONFLICT DO NOTHING;

-- Standup (08:15-08:45 CST = 14:15-14:45 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0523', 'day_2026-01-15',
    '2026-01-15T14:15:00Z', '2026-01-15T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Standup with Maya and David, shared progress on onboarding wireframes.', '["meeting", "standup", "design", "onboarding"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 72
) ON CONFLICT DO NOTHING;

-- Focused work (08:45-11:30 CST = 14:45-17:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0524', 'day_2026-01-15',
    '2026-01-15T14:45:00Z', '2026-01-15T17:30:00Z',
    'Focused design work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Worked on high-fidelity Figma screens for the first two steps of the new onboarding flow.', '["design", "figma", "focus", "deep-work", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 69
) ON CONFLICT DO NOTHING;

-- Lunch solo (11:30-12:15 CST = 17:30-18:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0525', 'day_2026-01-15',
    '2026-01-15T17:30:00Z', '2026-01-15T18:15:00Z',
    'Lunch', 'Office', '["location_visit"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Ate lunch at the office, salad from the place down the block.', '["food"]', '["place_demo_office"]',
    NULL, NULL, NULL, 'NEW', 72
) ON CONFLICT DO NOTHING;

-- Bike commute home early for WFH (12:30-13:00 CST = 18:30-19:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0526', 'day_2026-01-15',
    '2026-01-15T18:30:00Z', '2026-01-15T19:00:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Headed home early to WFH for the afternoon.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 111
) ON CONFLICT DO NOTHING;

-- WFH afternoon work (13:30-16:30 CST = 19:30-22:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0527', 'day_2026-01-15',
    '2026-01-15T19:30:00Z', '2026-01-15T22:30:00Z',
    'WFH afternoon', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Worked from home on the onboarding research plan and drafted interview questions.', '["work", "design", "onboarding", "research", "remote"]', '["place_demo_home", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 72
) ON CONFLICT DO NOTHING;

-- Mueller trails walk (17:00-17:45 CST = 23:00-23:45 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0528', 'day_2026-01-15',
    '2026-01-15T23:00:00Z', '2026-01-15T23:45:00Z',
    'Walk', 'Mueller Trails', '["steps"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Afternoon walk on Mueller trails to clear my head.', '["exercise", "outdoors", "walking", "mueller-trails"]', '["place_demo_mueller_trails"]',
    NULL, NULL, NULL, 'NEW', 149
) ON CONFLICT DO NOTHING;

-- Dinner and browsing (18:30-22:00 CST = 00:30-04:00+1 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0529', 'day_2026-01-15',
    '2026-01-16T00:30:00Z', '2026-01-16T04:00:00Z',
    'Dinner and browsing', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Made a quick dinner, then spent the evening browsing design inspiration for the onboarding project.', '["food", "browsing", "leisure", "onboarding"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 68
) ON CONFLICT DO NOTHING;

-- ── Friday, January 16, 2026 — Game night at Jess's ────────────────────────

-- Sleep (00:00-06:30 CST = 06:00-12:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0530', 'day_2026-01-16',
    '2026-01-16T06:00:00Z', '2026-01-16T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Slept from midnight to 6:30am.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 56
) ON CONFLICT DO NOTHING;

-- Morning routine (06:30-07:15 CST = 12:30-13:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0531', 'day_2026-01-16',
    '2026-01-16T12:30:00Z', '2026-01-16T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Coffee and morning routine, confirmed game night plans with Jess.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 65
) ON CONFLICT DO NOTHING;

-- Bike commute (07:15-07:45 CST = 13:15-13:45 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0532', 'day_2026-01-16',
    '2026-01-16T13:15:00Z', '2026-01-16T13:45:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked to the office.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 124
) ON CONFLICT DO NOTHING;

-- Coffee and Slack (07:45-08:15 CST = 13:45-14:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0533', 'day_2026-01-16',
    '2026-01-16T13:45:00Z', '2026-01-16T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Coffee and Slack, reviewed feedback on onboarding wireframes from the team.', '["messaging", "work", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 64
) ON CONFLICT DO NOTHING;

-- Standup (08:15-08:45 CST = 14:15-14:45 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0534', 'day_2026-01-16',
    '2026-01-16T14:15:00Z', '2026-01-16T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Friday standup, wrapped up the week on onboarding progress.', '["meeting", "standup", "design", "onboarding"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 72
) ON CONFLICT DO NOTHING;

-- Focused work (08:45-11:30 CST = 14:45-17:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0535', 'day_2026-01-16',
    '2026-01-16T14:45:00Z', '2026-01-16T17:30:00Z',
    'Focused design work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Polished the onboarding prototype for the step-1 and step-2 screens.', '["design", "figma", "focus", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 69
) ON CONFLICT DO NOTHING;

-- Lunch (11:30-12:15 CST = 17:30-18:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0536', 'day_2026-01-16',
    '2026-01-16T17:30:00Z', '2026-01-16T18:15:00Z',
    'Lunch', 'Office', '["location_visit"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Quick lunch at the office.', '["food"]', '["place_demo_office"]',
    NULL, NULL, NULL, 'NEW', 68
) ON CONFLICT DO NOTHING;

-- Afternoon work — shorter Friday (12:15-15:30 CST = 18:15-21:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0537', 'day_2026-01-16',
    '2026-01-16T18:15:00Z', '2026-01-16T21:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Wrapped up loose ends for the week, cleaned up Figma files.', '["work", "design", "figma"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 67
) ON CONFLICT DO NOTHING;

-- Bike commute home (15:30-16:00 CST = 21:30-22:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0538', 'day_2026-01-16',
    '2026-01-16T21:30:00Z', '2026-01-16T22:00:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked home early for Friday.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 121
) ON CONFLICT DO NOTHING;

-- Game night at Jess's (19:00-23:00 CST = 01:00-05:00+1 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0539', 'day_2026-01-16',
    '2026-01-17T01:00:00Z', '2026-01-17T05:00:00Z',
    'Game night', 'Jess''s Place', '["location_visit", "transcription"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Game night at Jess''s place with Priya — played Catan and a new card game Priya brought.', '["social", "games", "food"]', '["person_demo_jess", "person_demo_priya", "place_demo_jess"]',
    NULL, NULL, NULL, 'NEW', 68
) ON CONFLICT DO NOTHING;

-- ── Saturday, January 17, 2026 — Mom call ───────────────────────────────────

-- Sleep (01:00-08:30 CST = 07:00-14:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0540', 'day_2026-01-17',
    '2026-01-17T07:00:00Z', '2026-01-17T14:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Slept in after game night, about 7.5 hours.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 62
) ON CONFLICT DO NOTHING;

-- Slow morning (08:30-10:00 CST = 14:30-16:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0541', 'day_2026-01-17',
    '2026-01-17T14:30:00Z', '2026-01-17T16:00:00Z',
    'Slow morning', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Lazy Saturday morning, coffee and scrolling through Instagram.', '["routine", "morning", "coffee", "browsing"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 67
) ON CONFLICT DO NOTHING;

-- Lady Bird Lake walk (10:00-11:30 CST = 16:00-17:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0542', 'day_2026-01-17',
    '2026-01-17T16:00:00Z', '2026-01-17T17:30:00Z',
    'Lady Bird Lake walk', 'Lady Bird Lake', '["steps", "location_visit"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Long walk along Lady Bird Lake, boardwalk section.', '["exercise", "outdoors", "walking"]', '["place_demo_ladybird"]',
    NULL, NULL, NULL, 'NEW', 85
) ON CONFLICT DO NOTHING;

-- Errands (11:30-13:00 CST = 17:30-19:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0543', 'day_2026-01-17',
    '2026-01-17T17:30:00Z', '2026-01-17T19:00:00Z',
    'Errands and lunch', NULL, '["location_visit"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Ran errands, stopped for a breakfast taco at a food truck.', '["food"]', '[]',
    NULL, NULL, NULL, 'NEW', 78
) ON CONFLICT DO NOTHING;

-- Mom call (14:00-14:45 CST = 20:00-20:45 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0544', 'day_2026-01-17',
    '2026-01-17T20:00:00Z', '2026-01-17T20:45:00Z',
    'Phone call with Mom', 'Home', '["message", "transcription"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Weekly call with Mom, she asked about work and whether I''m still thinking about buying a place.', '["family", "phone-call"]', '["person_demo_mom", "place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 71
) ON CONFLICT DO NOTHING;

-- Afternoon reading (15:00-17:30 CST = 21:00-23:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0545', 'day_2026-01-17',
    '2026-01-17T21:00:00Z', '2026-01-17T23:30:00Z',
    'Reading', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Spent the afternoon reading and journaling.', '["leisure", "reflection"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 65
) ON CONFLICT DO NOTHING;

-- Dinner and movie (18:00-22:00 CST = 00:00-04:00+1 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0546', 'day_2026-01-17',
    '2026-01-18T00:00:00Z', '2026-01-18T04:00:00Z',
    'Dinner and movie', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Cooked chicken and rice, watched a movie at home.', '["food", "leisure"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 70
) ON CONFLICT DO NOTHING;

-- ── Sunday, January 18, 2026 ────────────────────────────────────────────────

-- Sleep (00:00-08:00 CST = 06:00-14:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0547', 'day_2026-01-18',
    '2026-01-18T06:00:00Z', '2026-01-18T14:00:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Slept in on Sunday, about 8 hours.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 57
) ON CONFLICT DO NOTHING;

-- Slow morning (08:00-09:30 CST = 14:00-15:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0548', 'day_2026-01-18',
    '2026-01-18T14:00:00Z', '2026-01-18T15:30:00Z',
    'Slow morning', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Sunday morning, coffee and the crossword.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 65
) ON CONFLICT DO NOTHING;

-- Morning run (09:30-10:30 CST = 15:30-16:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0549', 'day_2026-01-18',
    '2026-01-18T15:30:00Z', '2026-01-18T16:30:00Z',
    'Morning run', 'Mueller Trails', '["steps", "workout"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Sunday run on Mueller trails, 4 miles at an easy pace.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
    NULL, NULL, NULL, 'NEW', 63
) ON CONFLICT DO NOTHING;

-- Jo's Coffee (11:00-12:00 CST = 17:00-18:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0550', 'day_2026-01-18',
    '2026-01-18T17:00:00Z', '2026-01-18T18:00:00Z',
    'Coffee', 'Jo''s Coffee', '["location_visit"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Stopped by Jo''s on South Congress for a latte and some reading.', '["coffee", "leisure"]', '["place_demo_jos"]',
    NULL, NULL, NULL, 'NEW', 68
) ON CONFLICT DO NOTHING;

-- Cooking and meal prep (12:30-14:30 CST = 18:30-20:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0551', 'day_2026-01-18',
    '2026-01-18T18:30:00Z', '2026-01-18T20:30:00Z',
    'Cooking', 'Home', '["location_visit"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Meal prepped chili and rice for the week.', '["food"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 75
) ON CONFLICT DO NOTHING;

-- Afternoon browsing (15:00-17:30 CST = 21:00-23:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0552', 'day_2026-01-18',
    '2026-01-18T21:00:00Z', '2026-01-18T23:30:00Z',
    'Browsing and reading', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Browsed onboarding UX articles and took notes for Monday.', '["browsing", "leisure", "onboarding"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 64
) ON CONFLICT DO NOTHING;

-- Dinner and wind down (18:00-22:00 CST = 00:00-04:00+1 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0553', 'day_2026-01-18',
    '2026-01-19T00:00:00Z', '2026-01-19T04:00:00Z',
    'Dinner and wind down', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Ate the chili, then TV and early to bed for the week ahead.', '["food", "leisure"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 70
) ON CONFLICT DO NOTHING;

-- =============================================================================
-- WEEK 9: January 19 (Mon) - January 25 (Sun) — Onboarding in full swing
-- =============================================================================

-- ── Monday, January 19, 2026 ────────────────────────────────────────────────

-- Sleep (00:00-06:30 CST = 06:00-12:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0554', 'day_2026-01-19',
    '2026-01-19T06:00:00Z', '2026-01-19T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Slept from midnight to 6:30am.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 58
) ON CONFLICT DO NOTHING;

-- Morning routine (06:30-07:15 CST = 12:30-13:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0555', 'day_2026-01-19',
    '2026-01-19T12:30:00Z', '2026-01-19T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Coffee and morning routine, reviewed onboarding notes from the weekend.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 63
) ON CONFLICT DO NOTHING;

-- Bike commute (07:15-07:45 CST = 13:15-13:45 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0556', 'day_2026-01-19',
    '2026-01-19T13:15:00Z', '2026-01-19T13:45:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked to office.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 127
) ON CONFLICT DO NOTHING;

-- Coffee and Slack (07:45-08:15 CST = 13:45-14:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0557', 'day_2026-01-19',
    '2026-01-19T13:45:00Z', '2026-01-19T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Coffee and Slack at the office, caught up on weekend messages.', '["messaging", "work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 69
) ON CONFLICT DO NOTHING;

-- Standup (08:15-09:00 CST = 14:15-15:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0558', 'day_2026-01-19',
    '2026-01-19T14:15:00Z', '2026-01-19T15:00:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Monday standup — discussed onboarding user research schedule for this week.', '["meeting", "standup", "design", "onboarding"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 77
) ON CONFLICT DO NOTHING;

-- User research session (09:00-10:30 CST = 15:00-16:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0559', 'day_2026-01-19',
    '2026-01-19T15:00:00Z', '2026-01-19T16:30:00Z',
    'User research session', 'Office', '["calendar", "transcription"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'First onboarding user research session — interviewed a customer about their setup experience.', '["research", "onboarding", "design"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 64
) ON CONFLICT DO NOTHING;

-- Focused work (10:30-11:30 CST = 16:30-17:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0560', 'day_2026-01-19',
    '2026-01-19T16:30:00Z', '2026-01-19T17:30:00Z',
    'Research notes', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Synthesized notes from the user research session, tagged key themes.', '["research", "onboarding", "focus"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 69
) ON CONFLICT DO NOTHING;

-- Lunch solo (11:30-12:15 CST = 17:30-18:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0561', 'day_2026-01-19',
    '2026-01-19T17:30:00Z', '2026-01-19T18:15:00Z',
    'Lunch', 'Office', '["location_visit"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Ate the chili from meal prep at my desk.', '["food"]', '["place_demo_office"]',
    NULL, NULL, NULL, 'NEW', 68
) ON CONFLICT DO NOTHING;

-- Afternoon work (12:15-16:30 CST = 18:15-22:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0562', 'day_2026-01-19',
    '2026-01-19T18:15:00Z', '2026-01-19T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Worked on onboarding Figma prototypes, incorporating feedback from the research session.', '["design", "figma", "work", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 68
) ON CONFLICT DO NOTHING;

-- Bike commute home (16:30-17:00 CST = 22:30-23:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0563', 'day_2026-01-19',
    '2026-01-19T22:30:00Z', '2026-01-19T23:00:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked home.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 122
) ON CONFLICT DO NOTHING;

-- Evening run (17:30-18:15 CST = 23:30-00:15+1 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0564', 'day_2026-01-19',
    '2026-01-19T23:30:00Z', '2026-01-20T00:15:00Z',
    'Evening run', 'Mueller Trails', '["steps", "workout"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Quick 3-mile run on Mueller trails.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
    NULL, NULL, NULL, 'NEW', 154
) ON CONFLICT DO NOTHING;

-- Dinner and reading (19:00-22:00 CST = 01:00-04:00+1 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0565', 'day_2026-01-19',
    '2026-01-20T01:00:00Z', '2026-01-20T04:00:00Z',
    'Dinner and reading', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Dinner at home, then read for a couple hours before bed.', '["food", "leisure"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 59
) ON CONFLICT DO NOTHING;

-- ── Tuesday, January 20, 2026 ───────────────────────────────────────────────

-- Sleep (00:00-06:15 CST = 06:00-12:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0566', 'day_2026-01-20',
    '2026-01-20T06:00:00Z', '2026-01-20T12:15:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Slept midnight to about 6:15am.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 60
) ON CONFLICT DO NOTHING;

-- Morning routine (06:15-07:10 CST = 12:15-13:10 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0567', 'day_2026-01-20',
    '2026-01-20T12:15:00Z', '2026-01-20T13:10:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Coffee and morning routine.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 66
) ON CONFLICT DO NOTHING;

-- Bike commute (07:10-07:40 CST = 13:10-13:40 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0568', 'day_2026-01-20',
    '2026-01-20T13:10:00Z', '2026-01-20T13:40:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked to the office, foggy morning.', '["commute", "cycling", "podcast"]', '[]',
    NULL, NULL, NULL, 'NEW', 111
) ON CONFLICT DO NOTHING;

-- Coffee and Slack (07:40-08:15 CST = 13:40-14:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0569', 'day_2026-01-20',
    '2026-01-20T13:40:00Z', '2026-01-20T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Coffee and Slack at the office.', '["messaging", "work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 70
) ON CONFLICT DO NOTHING;

-- Standup + design review (08:15-09:30 CST = 14:15-15:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0570', 'day_2026-01-20',
    '2026-01-20T14:15:00Z', '2026-01-20T15:30:00Z',
    'Standup and design review', 'Office', '["calendar", "message", "transcription"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Standup then design review with David — walked through the onboarding prototype and got good feedback.', '["meeting", "standup", "design", "design-review", "onboarding"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 75
) ON CONFLICT DO NOTHING;

-- User research session (09:30-11:00 CST = 15:30-17:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0571', 'day_2026-01-20',
    '2026-01-20T15:30:00Z', '2026-01-20T17:00:00Z',
    'User research session', 'Office', '["calendar", "transcription"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Second onboarding user research interview — this customer had a very different setup journey.', '["research", "onboarding", "design"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 68
) ON CONFLICT DO NOTHING;

-- Lunch solo (11:00-12:00 CST = 17:00-18:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0572', 'day_2026-01-20',
    '2026-01-20T17:00:00Z', '2026-01-20T18:00:00Z',
    'Lunch', 'Office', '["location_visit"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Lunch at the office, leftover chili.', '["food"]', '["place_demo_office"]',
    NULL, NULL, NULL, 'NEW', 70
) ON CONFLICT DO NOTHING;

-- Afternoon work (12:00-16:30 CST = 18:00-22:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0573', 'day_2026-01-20',
    '2026-01-20T18:00:00Z', '2026-01-20T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Synthesized research notes and updated the onboarding journey map in Figma.', '["design", "figma", "work", "onboarding", "research"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 69
) ON CONFLICT DO NOTHING;

-- Bike commute home (16:30-17:00 CST = 22:30-23:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0574', 'day_2026-01-20',
    '2026-01-20T22:30:00Z', '2026-01-20T23:00:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked home.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 115
) ON CONFLICT DO NOTHING;

-- Evening run (17:30-18:20 CST = 23:30-00:20+1 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0575', 'day_2026-01-20',
    '2026-01-20T23:30:00Z', '2026-01-21T00:20:00Z',
    'Evening run', 'Mueller Trails', '["steps", "workout"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Tuesday evening run on Mueller trails, 3.5 miles.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
    NULL, NULL, NULL, 'NEW', 158
) ON CONFLICT DO NOTHING;

-- Dinner and TV (19:00-22:00 CST = 01:00-04:00+1 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0576', 'day_2026-01-20',
    '2026-01-21T01:00:00Z', '2026-01-21T04:00:00Z',
    'Dinner and TV', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Made a quick stir fry, watched TV.', '["food", "leisure"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 65
) ON CONFLICT DO NOTHING;

-- ── Wednesday, January 21, 2026 ─────────────────────────────────────────────

-- Sleep (00:00-06:30 CST = 06:00-12:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0577', 'day_2026-01-21',
    '2026-01-21T06:00:00Z', '2026-01-21T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Slept midnight to 6:30am.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 58
) ON CONFLICT DO NOTHING;

-- Morning routine (06:30-07:15 CST = 12:30-13:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0578', 'day_2026-01-21',
    '2026-01-21T12:30:00Z', '2026-01-21T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Coffee and checked messages.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 68
) ON CONFLICT DO NOTHING;

-- Bike commute (07:15-07:45 CST = 13:15-13:45 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0579', 'day_2026-01-21',
    '2026-01-21T13:15:00Z', '2026-01-21T13:45:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked to the office.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 131
) ON CONFLICT DO NOTHING;

-- Coffee and Slack (07:45-08:15 CST = 13:45-14:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0580', 'day_2026-01-21',
    '2026-01-21T13:45:00Z', '2026-01-21T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Coffee and Slack.', '["messaging", "work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 64
) ON CONFLICT DO NOTHING;

-- Standup (08:15-08:45 CST = 14:15-14:45 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0581', 'day_2026-01-21',
    '2026-01-21T14:15:00Z', '2026-01-21T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Standup — shared research findings with Maya and David, aligned on onboarding design direction.', '["meeting", "standup", "design", "onboarding"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 78
) ON CONFLICT DO NOTHING;

-- Focused work (08:45-11:30 CST = 14:45-17:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0582', 'day_2026-01-21',
    '2026-01-21T14:45:00Z', '2026-01-21T17:30:00Z',
    'Focused design work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Iterated on the onboarding flow based on research insights — simplified the account setup step.', '["design", "figma", "focus", "deep-work", "onboarding", "form-validation"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 68
) ON CONFLICT DO NOTHING;

-- Lunch with Maya at Tatsu-ya (11:30-12:30 CST = 17:30-18:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0583', 'day_2026-01-21',
    '2026-01-21T17:30:00Z', '2026-01-21T18:30:00Z',
    'Lunch with Maya', 'Ramen Tatsu-ya', '["location_visit", "transcription"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Lunch at Ramen Tatsu-ya with Maya, chatted about the research sessions and weekend plans.', '["social", "food", "ramen"]', '["person_demo_maya", "place_demo_ramen"]',
    NULL, NULL, NULL, 'NEW', 74
) ON CONFLICT DO NOTHING;

-- Afternoon work (12:30-16:30 CST = 18:30-22:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0584', 'day_2026-01-21',
    '2026-01-21T18:30:00Z', '2026-01-21T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Continued iterating on the onboarding prototype and prepared a research summary doc.', '["design", "figma", "work", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 72
) ON CONFLICT DO NOTHING;

-- Bike commute home (16:30-17:00 CST = 22:30-23:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0585', 'day_2026-01-21',
    '2026-01-21T22:30:00Z', '2026-01-21T23:00:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked home.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 133
) ON CONFLICT DO NOTHING;

-- Dinner and browsing (18:00-22:00 CST = 00:00-04:00+1 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0586', 'day_2026-01-21',
    '2026-01-22T00:00:00Z', '2026-01-22T04:00:00Z',
    'Dinner and browsing', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Made a salad for dinner, browsed the web and read before bed.', '["food", "leisure", "browsing"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 66
) ON CONFLICT DO NOTHING;

-- ── Thursday, January 22, 2026 ──────────────────────────────────────────────

-- Sleep (00:00-06:20 CST = 06:00-12:20 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0587', 'day_2026-01-22',
    '2026-01-22T06:00:00Z', '2026-01-22T12:20:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Slept from midnight to about 6:20am.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 60
) ON CONFLICT DO NOTHING;

-- Morning routine (06:20-07:10 CST = 12:20-13:10 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0588', 'day_2026-01-22',
    '2026-01-22T12:20:00Z', '2026-01-22T13:10:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Coffee and morning routine, checked texts from Rachel about scheduling a house showing this weekend.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 64
) ON CONFLICT DO NOTHING;

-- Bike commute (07:10-07:40 CST = 13:10-13:40 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0589', 'day_2026-01-22',
    '2026-01-22T13:10:00Z', '2026-01-22T13:40:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked to the office.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 134
) ON CONFLICT DO NOTHING;

-- Coffee and Slack (07:40-08:15 CST = 13:40-14:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0590', 'day_2026-01-22',
    '2026-01-22T13:40:00Z', '2026-01-22T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Coffee and Slack at the office.', '["messaging", "work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 69
) ON CONFLICT DO NOTHING;

-- Standup (08:15-08:45 CST = 14:15-14:45 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0591', 'day_2026-01-22',
    '2026-01-22T14:15:00Z', '2026-01-22T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Standup with Maya and David, talked about finishing the onboarding prototype this week.', '["meeting", "standup", "design", "onboarding"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 73
) ON CONFLICT DO NOTHING;

-- Focused work (08:45-11:30 CST = 14:45-17:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0592', 'day_2026-01-22',
    '2026-01-22T14:45:00Z', '2026-01-22T17:30:00Z',
    'Focused design work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Built out the step-3 and step-4 screens for the onboarding prototype in Figma.', '["design", "figma", "focus", "deep-work", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 63
) ON CONFLICT DO NOTHING;

-- Lunch solo (11:30-12:15 CST = 17:30-18:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0593', 'day_2026-01-22',
    '2026-01-22T17:30:00Z', '2026-01-22T18:15:00Z',
    'Lunch', 'Office', '["location_visit"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Ate lunch at my desk, sandwich from the deli.', '["food"]', '["place_demo_office"]',
    NULL, NULL, NULL, 'NEW', 72
) ON CONFLICT DO NOTHING;

-- Afternoon work (12:15-16:30 CST = 18:15-22:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0594', 'day_2026-01-22',
    '2026-01-22T18:15:00Z', '2026-01-22T22:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Finished the onboarding prototype first pass, shared it with the team for feedback.', '["design", "figma", "work", "onboarding", "messaging"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 68
) ON CONFLICT DO NOTHING;

-- Bike commute home (16:30-17:00 CST = 22:30-23:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0595', 'day_2026-01-22',
    '2026-01-22T22:30:00Z', '2026-01-22T23:00:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked home from the office.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 115
) ON CONFLICT DO NOTHING;

-- Walk on Mueller trails (17:30-18:15 CST = 23:30-00:15+1 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0596', 'day_2026-01-22',
    '2026-01-22T23:30:00Z', '2026-01-23T00:15:00Z',
    'Walk', 'Mueller Trails', '["steps"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Evening walk on Mueller trails.', '["exercise", "outdoors", "walking", "mueller-trails"]', '["place_demo_mueller_trails"]',
    NULL, NULL, NULL, 'NEW', 150
) ON CONFLICT DO NOTHING;

-- Dinner and reading (19:00-22:00 CST = 01:00-04:00+1 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0597', 'day_2026-01-22',
    '2026-01-23T01:00:00Z', '2026-01-23T04:00:00Z',
    'Dinner and reading', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Cooked pasta, read a few chapters of a novel before bed.', '["food", "leisure"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 58
) ON CONFLICT DO NOTHING;

-- ── Friday, January 23, 2026 — No game night this week, quiet Friday ────────

-- Sleep (00:00-06:30 CST = 06:00-12:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0598', 'day_2026-01-23',
    '2026-01-23T06:00:00Z', '2026-01-23T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Slept from midnight to 6:30am.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 61
) ON CONFLICT DO NOTHING;

-- Morning routine (06:30-07:15 CST = 12:30-13:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0599', 'day_2026-01-23',
    '2026-01-23T12:30:00Z', '2026-01-23T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Coffee and morning routine, texted Jess — she''s busy this weekend so no game night.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 67
) ON CONFLICT DO NOTHING;

-- Bike commute (07:15-07:45 CST = 13:15-13:45 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0600', 'day_2026-01-23',
    '2026-01-23T13:15:00Z', '2026-01-23T13:45:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked to the office.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 133
) ON CONFLICT DO NOTHING;

-- Coffee and Slack (07:45-08:15 CST = 13:45-14:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0601', 'day_2026-01-23',
    '2026-01-23T13:45:00Z', '2026-01-23T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Coffee at the office, checked Slack.', '["messaging", "work"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 67
) ON CONFLICT DO NOTHING;

-- Standup (08:15-08:45 CST = 14:15-14:45 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0602', 'day_2026-01-23',
    '2026-01-23T14:15:00Z', '2026-01-23T14:45:00Z',
    'Design standup', 'Office', '["calendar", "message"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Friday standup, reviewed the week on the onboarding project.', '["meeting", "standup", "design", "onboarding"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 72
) ON CONFLICT DO NOTHING;

-- Focused work (08:45-11:30 CST = 14:45-17:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0603', 'day_2026-01-23',
    '2026-01-23T14:45:00Z', '2026-01-23T17:30:00Z',
    'Focused design work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Refined the onboarding prototype based on team feedback, polished transitions.', '["design", "figma", "focus", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 66
) ON CONFLICT DO NOTHING;

-- Lunch solo (11:30-12:15 CST = 17:30-18:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0604', 'day_2026-01-23',
    '2026-01-23T17:30:00Z', '2026-01-23T18:15:00Z',
    'Lunch', 'Office', '["location_visit"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Lunch at the office.', '["food"]', '["place_demo_office"]',
    NULL, NULL, NULL, 'NEW', 71
) ON CONFLICT DO NOTHING;

-- Afternoon work (12:15-15:30 CST = 18:15-21:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0605', 'day_2026-01-23',
    '2026-01-23T18:15:00Z', '2026-01-23T21:30:00Z',
    'Afternoon work', 'Office', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Lighter Friday afternoon, wrapped up loose ends and organized research notes.', '["work", "design", "figma", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
    NULL, NULL, NULL, 'NEW', 66
) ON CONFLICT DO NOTHING;

-- Bike commute home (15:30-16:00 CST = 21:30-22:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0606', 'day_2026-01-23',
    '2026-01-23T21:30:00Z', '2026-01-23T22:00:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    FALSE, TRUE, FALSE, FALSE, FALSE,

    'Biked home.', '["commute", "cycling"]', '[]',
    NULL, NULL, NULL, 'NEW', 120
) ON CONFLICT DO NOTHING;

-- Mom call (17:00-17:35 CST = 23:00-23:35 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0607', 'day_2026-01-23',
    '2026-01-23T23:00:00Z', '2026-01-23T23:35:00Z',
    'Phone call with Mom', 'Home', '["message", "transcription"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Weekly call with Mom, told her about seeing a house this weekend with Rachel.', '["family", "phone-call"]', '["person_demo_mom", "place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 69
) ON CONFLICT DO NOTHING;

-- Quiet Friday evening (18:00-22:00 CST = 00:00-04:00+1 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0608', 'day_2026-01-23',
    '2026-01-24T00:00:00Z', '2026-01-24T04:00:00Z',
    'Quiet evening', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Quiet Friday night at home — cooked dinner, watched a movie, early to bed.', '["food", "leisure"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 66
) ON CONFLICT DO NOTHING;

-- ── Saturday, January 24, 2026 ──────────────────────────────────────────────

-- Sleep (00:00-07:30 CST = 06:00-13:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0609', 'day_2026-01-24',
    '2026-01-24T06:00:00Z', '2026-01-24T13:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Slept in on Saturday, about 7.5 hours.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 57
) ON CONFLICT DO NOTHING;

-- Slow morning (07:30-09:00 CST = 13:30-15:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0610', 'day_2026-01-24',
    '2026-01-24T13:30:00Z', '2026-01-24T15:00:00Z',
    'Slow morning', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Lazy Saturday morning, coffee and catching up on articles.', '["routine", "morning", "coffee", "browsing"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 63
) ON CONFLICT DO NOTHING;

-- Lady Bird Lake walk (09:30-11:00 CST = 15:30-17:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0611', 'day_2026-01-24',
    '2026-01-24T15:30:00Z', '2026-01-24T17:00:00Z',
    'Lady Bird Lake walk', 'Lady Bird Lake', '["steps", "location_visit"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Walked the Lady Bird Lake boardwalk loop, beautiful clear winter morning.', '["exercise", "outdoors", "walking"]', '["place_demo_ladybird"]',
    NULL, NULL, NULL, 'NEW', 98
) ON CONFLICT DO NOTHING;

-- Errands and lunch (11:00-13:00 CST = 17:00-19:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0612', 'day_2026-01-24',
    '2026-01-24T17:00:00Z', '2026-01-24T19:00:00Z',
    'Errands and lunch', NULL, '["location_visit"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Ran errands at Target, grabbed a taco on the way home.', '["food"]', '[]',
    NULL, NULL, NULL, 'NEW', 82
) ON CONFLICT DO NOTHING;

-- Afternoon reading (13:30-17:00 CST = 19:30-23:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0613', 'day_2026-01-24',
    '2026-01-24T19:30:00Z', '2026-01-24T23:00:00Z',
    'Reading and journaling', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Read and did some journaling about the upcoming house showing tomorrow.', '["leisure", "reflection", "house-hunting"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 64
) ON CONFLICT DO NOTHING;

-- Dinner and movie (18:00-22:00 CST = 00:00-04:00+1 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0614', 'day_2026-01-24',
    '2026-01-25T00:00:00Z', '2026-01-25T04:00:00Z',
    'Dinner and movie', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Cooked dinner, then watched a movie at home.', '["food", "leisure"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 67
) ON CONFLICT DO NOTHING;

-- ── Sunday, January 25, 2026 — RACHEL SECOND APPEARANCE (house showing) ────

-- Sleep (00:00-07:30 CST = 06:00-13:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0615', 'day_2026-01-25',
    '2026-01-25T06:00:00Z', '2026-01-25T13:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    FALSE, FALSE, TRUE, FALSE, FALSE,

    'Slept in on Sunday, about 7.5 hours.', '["sleep"]', '[]',
    NULL, NULL, NULL, 'NEW', 55
) ON CONFLICT DO NOTHING;

-- Slow morning (07:30-09:00 CST = 13:30-15:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0616', 'day_2026-01-25',
    '2026-01-25T13:30:00Z', '2026-01-25T15:00:00Z',
    'Slow morning', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Sunday morning, coffee and the crossword, a bit nervous about the house showing later.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 66
) ON CONFLICT DO NOTHING;

-- Morning run (09:00-10:00 CST = 15:00-16:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0617', 'day_2026-01-25',
    '2026-01-25T15:00:00Z', '2026-01-25T16:00:00Z',
    'Morning run', 'Mueller Trails', '["steps", "workout"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Quick run on Mueller trails before the house showing, 3 miles.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
    NULL, NULL, NULL, 'NEW', 67
) ON CONFLICT DO NOTHING;

-- ** RACHEL SECOND APPEARANCE ** House showing (11:00-11:45 CST = 17:00-17:45 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0618', 'day_2026-01-25',
    '2026-01-25T17:00:00Z', '2026-01-25T17:45:00Z',
    'House showing', 'East Austin', '["location_visit", "transcription"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Toured a 2-bed bungalow on Webberville Rd with Rachel — cute but the kitchen was too small and the yard was tiny, not feeling it.', '["house-hunting", "real-estate", "neighborhood"]', '["person_demo_rachel", "org_demo_realty"]',
    NULL, NULL, NULL, 'NEW', 82
) ON CONFLICT DO NOTHING;

-- Lunch (12:00-13:00 CST = 18:00-19:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0619', 'day_2026-01-25',
    '2026-01-25T18:00:00Z', '2026-01-25T19:00:00Z',
    'Lunch', NULL, '["location_visit"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Grabbed a quick lunch at a taco truck after the showing.', '["food"]', '[]',
    NULL, NULL, NULL, 'NEW', 76
) ON CONFLICT DO NOTHING;

-- Cooking and meal prep (13:30-15:30 CST = 19:30-21:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0620', 'day_2026-01-25',
    '2026-01-25T19:30:00Z', '2026-01-25T21:30:00Z',
    'Cooking', 'Home', '["location_visit"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Meal prepped curry and rice for the week ahead.', '["food"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 69
) ON CONFLICT DO NOTHING;

-- Afternoon reading (15:30-17:30 CST = 21:30-23:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0621', 'day_2026-01-25',
    '2026-01-25T21:30:00Z', '2026-01-25T23:30:00Z',
    'Reading', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Read for a couple hours, reflected on the house showing — not the right place.', '["leisure", "reflection", "house-hunting"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 59
) ON CONFLICT DO NOTHING;

-- Dinner and wind down (18:00-22:00 CST = 00:00-04:00+1 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES (
    'ev_b0622', 'day_2026-01-25',
    '2026-01-26T00:00:00Z', '2026-01-26T04:00:00Z',
    'Dinner and wind down', 'Home', '["app_usage"]',
    FALSE, FALSE, FALSE, FALSE, FALSE,

    'Simple dinner from the curry batch, watched TV and got ready for the week.', '["food", "leisure"]', '["place_demo_home"]',
    NULL, NULL, NULL, 'NEW', 67
) ON CONFLICT DO NOTHING;
-- =============================================================================
-- Baseline Seed: Weeks 10-12 — January 26 through February 11, 2026
-- =============================================================================
--
-- Character: UX designer, early 30s, Mueller (East Austin), works at Canopy.
-- Onboarding funnel redesign is the primary work focus.
-- No Rachel / house-hunting in this window.
-- Game night at Jess's: Friday Feb 6.
-- Mom calls: Saturday Jan 31, Friday Feb 6, Saturday Feb 7.
-- All times UTC (CST = UTC-6).
--
-- Event IDs: ev_b0631 through ev_b0800
-- =============================================================================

-- Clean up any prior run
DELETE FROM wiki_events WHERE id LIKE 'ev_b0%' AND CAST(SUBSTR(id, 5) AS INTEGER) BETWEEN 631 AND 800;

-- ─────────────────────────────────────────────────────────────────────────────
-- Wiki Days
-- ─────────────────────────────────────────────────────────────────────────────

INSERT INTO wiki_days (id, date, start_timezone, morning_baseline)
VALUES
('day_2026-01-26', '2026-01-26', 'America/Chicago', 0.48),
('day_2026-01-27', '2026-01-27', 'America/Chicago', 0.52),
('day_2026-01-28', '2026-01-28', 'America/Chicago', 0.50),
('day_2026-01-29', '2026-01-29', 'America/Chicago', 0.45),
('day_2026-01-30', '2026-01-30', 'America/Chicago', 0.53),
('day_2026-01-31', '2026-01-31', 'America/Chicago', 0.55),
('day_2026-02-01', '2026-02-01', 'America/Chicago', 0.47),
('day_2026-02-02', '2026-02-02', 'America/Chicago', 0.50),
('day_2026-02-03', '2026-02-03', 'America/Chicago', 0.44),
('day_2026-02-04', '2026-02-04', 'America/Chicago', 0.51),
('day_2026-02-05', '2026-02-05', 'America/Chicago', 0.46),
('day_2026-02-06', '2026-02-06', 'America/Chicago', 0.54),
('day_2026-02-07', '2026-02-07', 'America/Chicago', 0.58),
('day_2026-02-08', '2026-02-08', 'America/Chicago', 0.49),
('day_2026-02-09', '2026-02-09', 'America/Chicago', 0.42),
('day_2026-02-10', '2026-02-10', 'America/Chicago', 0.50),
('day_2026-02-11', '2026-02-11', 'America/Chicago', 0.48) ON CONFLICT DO NOTHING;

-- ─────────────────────────────────────────────────────────────────────────────
-- Wiki Events
-- ─────────────────────────────────────────────────────────────────────────────

-- =============================================================================
-- Monday, January 26, 2026 (10 events)
-- =============================================================================

INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES
('ev_b0631', 'day_2026-01-26', '2026-01-26T06:00:00Z', '2026-01-26T12:30:00Z',
 'Sleep', 'Home', '["sleep"]',
 FALSE, FALSE, TRUE, FALSE, FALSE,

 'Overnight sleep, about 6.5 hours.', '["sleep"]', '[]',
 NULL, NULL, NULL, 'NEW', 57),

('ev_b0632', 'day_2026-01-26', '2026-01-26T12:30:00Z', '2026-01-26T13:15:00Z',
 'Morning routine', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Coffee and catching up on Slack messages before heading out.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 65),

('ev_b0633', 'day_2026-01-26', '2026-01-26T13:15:00Z', '2026-01-26T13:45:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 FALSE, TRUE, FALSE, FALSE, FALSE,

 'Bike commute to the downtown office, 30 minutes.', '["commute", "cycling", "podcast"]', '[]',
 NULL, NULL, NULL, 'NEW', 126),

('ev_b0634', 'day_2026-01-26', '2026-01-26T13:45:00Z', '2026-01-26T14:15:00Z',
 'Coffee and Slack', 'Office', '["app_usage", "message"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Settled in at the office with coffee and cleared Slack notifications.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 71),

('ev_b0635', 'day_2026-01-26', '2026-01-26T14:15:00Z', '2026-01-26T15:00:00Z',
 'Design standup', 'Office', '["calendar", "message", "transcription"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Monday standup with Maya and David reviewing onboarding funnel metrics from last week.', '["meeting", "standup", "design", "onboarding"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 73),

('ev_b0636', 'day_2026-01-26', '2026-01-26T15:00:00Z', '2026-01-26T17:30:00Z',
 'Focused design work', 'Office', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Deep work on the onboarding step-progress component in Figma.', '["design", "figma", "focus", "deep-work", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 66),

('ev_b0637', 'day_2026-01-26', '2026-01-26T17:30:00Z', '2026-01-26T18:30:00Z',
 'Lunch', 'Office', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Ate lunch at desk while reading design articles.', '["food", "browsing"]', '["place_demo_office"]',
 NULL, NULL, NULL, 'NEW', 73),

('ev_b0638', 'day_2026-01-26', '2026-01-26T18:30:00Z', '2026-01-26T22:30:00Z',
 'Afternoon work', 'Office', '["app_usage", "message"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Continued onboarding flow wireframes and responded to design feedback on Slack.', '["work", "design", "figma", "onboarding", "messaging"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 68),

('ev_b0639', 'day_2026-01-26', '2026-01-26T22:30:00Z', '2026-01-26T23:00:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 FALSE, TRUE, FALSE, FALSE, FALSE,

 'Bike commute home from the office.', '["commute", "cycling"]', '[]',
 NULL, NULL, NULL, 'NEW', 122),

('ev_b0640', 'day_2026-01-26', '2026-01-26T23:00:00Z', '2026-01-27T04:00:00Z',
 'Dinner and reading', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Made stir fry for dinner and read a few chapters of a novel before bed.', '["food", "leisure", "reflection"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 63) ON CONFLICT DO NOTHING;

-- =============================================================================
-- Tuesday, January 27, 2026 (10 events)
-- =============================================================================

INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES
('ev_b0641', 'day_2026-01-27', '2026-01-27T06:00:00Z', '2026-01-27T12:30:00Z',
 'Sleep', 'Home', '["sleep"]',
 FALSE, FALSE, TRUE, FALSE, FALSE,

 'Overnight sleep, about 6.5 hours.', '["sleep"]', '[]',
 NULL, NULL, NULL, 'NEW', 62),

('ev_b0642', 'day_2026-01-27', '2026-01-27T12:30:00Z', '2026-01-27T13:15:00Z',
 'Morning routine', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Morning coffee and checked texts from Jess about weekend plans.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 67),

('ev_b0643', 'day_2026-01-27', '2026-01-27T13:15:00Z', '2026-01-27T13:45:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 FALSE, TRUE, FALSE, FALSE, FALSE,

 'Bike commute to the office.', '["commute", "cycling", "podcast"]', '[]',
 NULL, NULL, NULL, 'NEW', 124),

('ev_b0644', 'day_2026-01-27', '2026-01-27T13:45:00Z', '2026-01-27T14:15:00Z',
 'Coffee and Slack', 'Office', '["app_usage", "message"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Grabbed coffee and synced up on Slack threads about the onboarding redesign.', '["messaging", "work", "coffee", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 66),

('ev_b0645', 'day_2026-01-27', '2026-01-27T14:15:00Z', '2026-01-27T15:00:00Z',
 'Design standup', 'Office', '["calendar", "message", "transcription"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Tuesday standup with Maya and David, discussed funnel drop-off at the email verification step.', '["meeting", "standup", "design", "onboarding", "form-validation"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 73),

('ev_b0646', 'day_2026-01-27', '2026-01-27T15:00:00Z', '2026-01-27T16:00:00Z',
 'Design review', 'Office', '["calendar", "app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Design review session with David on onboarding form validation patterns.', '["meeting", "design-review", "design", "onboarding", "form-validation"]', '["person_demo_david", "place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 73),

('ev_b0647', 'day_2026-01-27', '2026-01-27T16:00:00Z', '2026-01-27T17:30:00Z',
 'Focused design work', 'Office', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Iterated on the form validation error states in Figma.', '["design", "figma", "focus", "deep-work", "onboarding", "form-validation"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 62),

('ev_b0648', 'day_2026-01-27', '2026-01-27T17:30:00Z', '2026-01-27T18:30:00Z',
 'Lunch', 'Office', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Quick lunch at the office, ate at desk.', '["food"]', '["place_demo_office"]',
 NULL, NULL, NULL, 'NEW', 75),

('ev_b0649', 'day_2026-01-27', '2026-01-27T18:30:00Z', '2026-01-27T22:30:00Z',
 'Afternoon work', 'Office', '["app_usage", "message"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Wrapped up onboarding wireframes and posted updates to the design channel.', '["work", "design", "figma", "onboarding", "messaging"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 64),

('ev_b0650', 'day_2026-01-27', '2026-01-27T23:00:00Z', '2026-01-28T01:00:00Z',
 'Evening run', 'Mueller Trails', '["steps", "location_visit"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Evening 3-mile run on Mueller trails, good pace.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
 NULL, NULL, NULL, 'NEW', 68) ON CONFLICT DO NOTHING;

-- =============================================================================
-- Wednesday, January 28, 2026 (10 events)
-- =============================================================================

INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES
('ev_b0651', 'day_2026-01-28', '2026-01-28T06:00:00Z', '2026-01-28T12:30:00Z',
 'Sleep', 'Home', '["sleep"]',
 FALSE, FALSE, TRUE, FALSE, FALSE,

 'Overnight sleep, about 6.5 hours.', '["sleep"]', '[]',
 NULL, NULL, NULL, 'NEW', 58),

('ev_b0652', 'day_2026-01-28', '2026-01-28T12:30:00Z', '2026-01-28T13:15:00Z',
 'Morning routine', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Morning coffee and browsed design blogs.', '["routine", "morning", "coffee", "browsing"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 67),

('ev_b0653', 'day_2026-01-28', '2026-01-28T13:15:00Z', '2026-01-28T13:45:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 FALSE, TRUE, FALSE, FALSE, FALSE,

 'Bike commute to the office.', '["commute", "cycling", "podcast"]', '[]',
 NULL, NULL, NULL, 'NEW', 117),

('ev_b0654', 'day_2026-01-28', '2026-01-28T13:45:00Z', '2026-01-28T14:15:00Z',
 'Coffee and Slack', 'Office', '["app_usage", "message"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Coffee at the office and caught up on Slack.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 65),

('ev_b0655', 'day_2026-01-28', '2026-01-28T14:15:00Z', '2026-01-28T15:00:00Z',
 'Design standup', 'Office', '["calendar", "message", "transcription"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Wednesday standup, Maya flagged a navigation redesign issue in the onboarding flow.', '["meeting", "standup", "design", "onboarding", "navigation"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 71),

('ev_b0656', 'day_2026-01-28', '2026-01-28T15:00:00Z', '2026-01-28T17:30:00Z',
 'Focused design work', 'Office', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Worked on the navigation redesign for the onboarding sidebar in Figma.', '["design", "figma", "focus", "deep-work", "onboarding", "navigation", "sidebar"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 67),

('ev_b0657', 'day_2026-01-28', '2026-01-28T17:30:00Z', '2026-01-28T18:30:00Z',
 'Lunch at Ramen Tatsu-ya', 'Ramen Tatsu-ya', '["location_visit"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Weekly lunch with Maya at Tatsu-ya, talked about the onboarding sprint timeline.', '["food", "social", "ramen"]', '["person_demo_maya", "place_demo_ramen"]',
 NULL, NULL, NULL, 'NEW', 70),

('ev_b0658', 'day_2026-01-28', '2026-01-28T18:30:00Z', '2026-01-28T22:30:00Z',
 'Afternoon work', 'Office', '["app_usage", "message"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Afternoon heads-down on the onboarding navigation prototype.', '["work", "focus", "deep-work", "onboarding", "navigation", "figma"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 67),

('ev_b0659', 'day_2026-01-28', '2026-01-28T22:30:00Z', '2026-01-28T23:00:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 FALSE, TRUE, FALSE, FALSE, FALSE,

 'Bike commute home.', '["commute", "cycling"]', '[]',
 NULL, NULL, NULL, 'NEW', 112),

('ev_b0660', 'day_2026-01-28', '2026-01-28T23:00:00Z', '2026-01-29T04:00:00Z',
 'Evening at home', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Cooked a simple pasta dinner and watched a documentary.', '["food", "leisure"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 60) ON CONFLICT DO NOTHING;

-- =============================================================================
-- Thursday, January 29, 2026 (10 events — WFH afternoon)
-- =============================================================================

INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES
('ev_b0661', 'day_2026-01-29', '2026-01-29T06:00:00Z', '2026-01-29T12:45:00Z',
 'Sleep', 'Home', '["sleep"]',
 FALSE, FALSE, TRUE, FALSE, FALSE,

 'Slept in a bit, about 6 hours 45 minutes.', '["sleep"]', '[]',
 NULL, NULL, NULL, 'NEW', 60),

('ev_b0662', 'day_2026-01-29', '2026-01-29T12:45:00Z', '2026-01-29T13:15:00Z',
 'Morning routine', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Quick morning routine and coffee.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 63),

('ev_b0663', 'day_2026-01-29', '2026-01-29T13:15:00Z', '2026-01-29T13:45:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 FALSE, TRUE, FALSE, FALSE, FALSE,

 'Bike commute to the office.', '["commute", "cycling"]', '[]',
 NULL, NULL, NULL, 'NEW', 126),

('ev_b0664', 'day_2026-01-29', '2026-01-29T13:45:00Z', '2026-01-29T14:15:00Z',
 'Coffee and Slack', 'Office', '["app_usage", "message"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Morning coffee and reviewing overnight Slack threads.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 68),

('ev_b0665', 'day_2026-01-29', '2026-01-29T14:15:00Z', '2026-01-29T15:00:00Z',
 'Design standup', 'Office', '["calendar", "message", "transcription"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Thursday standup, discussed form validation edge cases for the onboarding flow with David.', '["meeting", "standup", "design", "onboarding", "form-validation"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 74),

('ev_b0666', 'day_2026-01-29', '2026-01-29T15:00:00Z', '2026-01-29T17:30:00Z',
 'Focused design work', 'Office', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Morning design session on onboarding error handling screens.', '["design", "figma", "focus", "deep-work", "onboarding", "form-validation"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 67),

('ev_b0667', 'day_2026-01-29', '2026-01-29T17:30:00Z', '2026-01-29T18:15:00Z',
 'Lunch', 'Office', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Grabbed a sandwich from the cafe downstairs.', '["food"]', '["place_demo_office"]',
 NULL, NULL, NULL, 'NEW', 77),

('ev_b0668', 'day_2026-01-29', '2026-01-29T18:30:00Z', '2026-01-29T22:30:00Z',
 'WFH afternoon', 'Home', '["app_usage", "message"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Worked from home in the afternoon, polishing the onboarding prototype and writing spec notes.', '["work", "design", "figma", "onboarding", "focus"]', '["place_demo_home", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 65),

('ev_b0669', 'day_2026-01-29', '2026-01-29T23:30:00Z', '2026-01-30T01:00:00Z',
 'Evening run', 'Mueller Trails', '["steps", "location_visit"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Evening run on Mueller trails, 3 miles at easy pace.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
 NULL, NULL, NULL, 'NEW', 68),

('ev_b0670', 'day_2026-01-29', '2026-01-30T01:00:00Z', '2026-01-30T04:00:00Z',
 'Evening wind-down', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Showered after the run, read and browsed the internet before bed.', '["leisure", "browsing", "reflection"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 62) ON CONFLICT DO NOTHING;

-- =============================================================================
-- Friday, January 30, 2026 (10 events — no game night, Mom call Saturday instead)
-- =============================================================================

INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES
('ev_b0671', 'day_2026-01-30', '2026-01-30T06:00:00Z', '2026-01-30T12:30:00Z',
 'Sleep', 'Home', '["sleep"]',
 FALSE, FALSE, TRUE, FALSE, FALSE,

 'Overnight sleep, about 6.5 hours.', '["sleep"]', '[]',
 NULL, NULL, NULL, 'NEW', 62),

('ev_b0672', 'day_2026-01-30', '2026-01-30T12:30:00Z', '2026-01-30T13:15:00Z',
 'Morning routine', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Coffee and scrolled through messages, quiet Friday morning.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 64),

('ev_b0673', 'day_2026-01-30', '2026-01-30T13:15:00Z', '2026-01-30T13:45:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 FALSE, TRUE, FALSE, FALSE, FALSE,

 'Bike commute to the office.', '["commute", "cycling", "podcast"]', '[]',
 NULL, NULL, NULL, 'NEW', 135),

('ev_b0674', 'day_2026-01-30', '2026-01-30T13:45:00Z', '2026-01-30T14:15:00Z',
 'Coffee and Slack', 'Office', '["app_usage", "message"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Friday coffee and Slack catch-up.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 72),

('ev_b0675', 'day_2026-01-30', '2026-01-30T14:15:00Z', '2026-01-30T15:00:00Z',
 'Design standup', 'Office', '["calendar", "message", "transcription"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Friday standup with Maya and David, wrapped up the week on onboarding progress.', '["meeting", "standup", "design", "onboarding"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 76),

('ev_b0676', 'day_2026-01-30', '2026-01-30T15:00:00Z', '2026-01-30T17:30:00Z',
 'Focused design work', 'Office', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Worked on polishing the onboarding email verification screen.', '["design", "figma", "focus", "onboarding", "form-validation"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 63),

('ev_b0677', 'day_2026-01-30', '2026-01-30T17:30:00Z', '2026-01-30T18:30:00Z',
 'Lunch', 'Office', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Lunch at desk, leftover soup from home.', '["food"]', '["place_demo_office"]',
 NULL, NULL, NULL, 'NEW', 71),

('ev_b0678', 'day_2026-01-30', '2026-01-30T18:30:00Z', '2026-01-30T21:30:00Z',
 'Afternoon work', 'Office', '["app_usage", "message"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Shorter Friday afternoon, finished up design documentation for the onboarding handoff.', '["work", "design", "onboarding", "code-review"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 65),

('ev_b0679', 'day_2026-01-30', '2026-01-30T21:30:00Z', '2026-01-30T22:00:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 FALSE, TRUE, FALSE, FALSE, FALSE,

 'Bike commute home, left a bit early on Friday.', '["commute", "cycling"]', '[]',
 NULL, NULL, NULL, 'NEW', 131),

('ev_b0680', 'day_2026-01-30', '2026-01-30T22:00:00Z', '2026-01-31T04:30:00Z',
 'Quiet evening', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Quiet Friday night at home, made tacos and watched a movie.', '["food", "leisure"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 66) ON CONFLICT DO NOTHING;

-- =============================================================================
-- Saturday, January 31, 2026 (7 events — Lady Bird Lake, Mom call)
-- =============================================================================

INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES
('ev_b0681', 'day_2026-01-31', '2026-01-31T06:00:00Z', '2026-01-31T13:30:00Z',
 'Sleep', 'Home', '["sleep"]',
 FALSE, FALSE, TRUE, FALSE, FALSE,

 'Slept in on Saturday, about 7.5 hours.', '["sleep"]', '[]',
 NULL, NULL, NULL, 'NEW', 60),

('ev_b0682', 'day_2026-01-31', '2026-01-31T13:30:00Z', '2026-01-31T15:00:00Z',
 'Slow morning', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Slow Saturday morning, made pancakes and read the news.', '["routine", "morning", "coffee", "food"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 66),

('ev_b0683', 'day_2026-01-31', '2026-01-31T15:00:00Z', '2026-01-31T17:00:00Z',
 'Walk at Lady Bird Lake', 'Lady Bird Lake', '["steps", "location_visit"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Long walk around Lady Bird Lake, clear and cool morning.', '["exercise", "outdoors"]', '["place_demo_ladybird"]',
 NULL, NULL, NULL, 'NEW', 98),

('ev_b0684', 'day_2026-01-31', '2026-01-31T17:00:00Z', '2026-01-31T19:00:00Z',
 'Errands', NULL, '["location_visit"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Ran errands, grocery store and Target.', '["routine", "driving"]', '[]',
 NULL, NULL, NULL, 'NEW', 79),

('ev_b0685', 'day_2026-01-31', '2026-01-31T19:00:00Z', '2026-01-31T20:00:00Z',
 'Phone call with Mom', 'Home', '["transcription"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Weekly phone call with Mom, caught up on family news and Dad''s golf trip.', '["family", "phone-call"]', '["person_demo_mom", "place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 65),

('ev_b0686', 'day_2026-01-31', '2026-01-31T23:00:00Z', '2026-02-01T02:00:00Z',
 'Dinner and movie', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Cooked a stew and watched a thriller at home.', '["food", "leisure"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 66),

('ev_b0687', 'day_2026-01-31', '2026-02-01T02:00:00Z', '2026-02-01T04:00:00Z',
 'Wind down', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Browsed Pinterest for apartment decor ideas before bed.', '["leisure", "browsing", "reflection"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 58) ON CONFLICT DO NOTHING;

-- =============================================================================
-- Sunday, February 1, 2026 (7 events — slow day)
-- =============================================================================

INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES
('ev_b0688', 'day_2026-02-01', '2026-02-01T06:00:00Z', '2026-02-01T14:00:00Z',
 'Sleep', 'Home', '["sleep"]',
 FALSE, FALSE, TRUE, FALSE, FALSE,

 'Long Sunday sleep, about 8 hours.', '["sleep"]', '[]',
 NULL, NULL, NULL, 'NEW', 61),

('ev_b0689', 'day_2026-02-01', '2026-02-01T14:00:00Z', '2026-02-01T15:30:00Z',
 'Slow morning', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Lazy Sunday morning, made coffee and journaled.', '["routine", "morning", "coffee", "reflection"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 68),

('ev_b0690', 'day_2026-02-01', '2026-02-01T15:30:00Z', '2026-02-01T17:00:00Z',
 'Mueller trails run', 'Mueller Trails', '["steps", "location_visit"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Sunday morning run on Mueller trails, 4 miles.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
 NULL, NULL, NULL, 'NEW', 150),

('ev_b0691', 'day_2026-02-01', '2026-02-01T17:00:00Z', '2026-02-01T19:00:00Z',
 'Reading and relaxing', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Spent the afternoon reading and catching up on design newsletters.', '["leisure", "browsing", "reflection"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 59),

('ev_b0692', 'day_2026-02-01', '2026-02-01T19:00:00Z', '2026-02-01T21:00:00Z',
 'Meal prep', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Meal prepped for the week — chicken and rice bowls.', '["food", "routine"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 71),

('ev_b0693', 'day_2026-02-01', '2026-02-01T21:00:00Z', '2026-02-02T01:00:00Z',
 'Evening at home', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Watched a design talk on YouTube and texted with Maya about Monday plans.', '["leisure", "messaging", "browsing"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 63),

('ev_b0694', 'day_2026-02-01', '2026-02-02T01:00:00Z', '2026-02-02T04:00:00Z',
 'Wind down', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Read in bed for a while before falling asleep.', '["leisure", "reflection"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 59) ON CONFLICT DO NOTHING;

-- =============================================================================
-- Monday, February 2, 2026 (10 events)
-- =============================================================================

INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES
('ev_b0695', 'day_2026-02-02', '2026-02-02T06:00:00Z', '2026-02-02T12:30:00Z',
 'Sleep', 'Home', '["sleep"]',
 FALSE, FALSE, TRUE, FALSE, FALSE,

 'Overnight sleep, about 6.5 hours.', '["sleep"]', '[]',
 NULL, NULL, NULL, 'NEW', 62),

('ev_b0696', 'day_2026-02-02', '2026-02-02T12:30:00Z', '2026-02-02T13:15:00Z',
 'Morning routine', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Morning coffee and checking Slack and email.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 64),

('ev_b0697', 'day_2026-02-02', '2026-02-02T13:15:00Z', '2026-02-02T13:45:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 FALSE, TRUE, FALSE, FALSE, FALSE,

 'Bike commute to the office on a chilly Monday morning.', '["commute", "cycling", "podcast"]', '[]',
 NULL, NULL, NULL, 'NEW', 123),

('ev_b0698', 'day_2026-02-02', '2026-02-02T13:45:00Z', '2026-02-02T14:15:00Z',
 'Coffee and Slack', 'Office', '["app_usage", "message"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Office coffee and clearing out weekend Slack messages.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 67),

('ev_b0699', 'day_2026-02-02', '2026-02-02T14:15:00Z', '2026-02-02T15:00:00Z',
 'Design standup', 'Office', '["calendar", "message", "transcription"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Monday standup, kicked off the week with onboarding navigation redesign status update.', '["meeting", "standup", "design", "onboarding", "navigation"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 74),

('ev_b0700', 'day_2026-02-02', '2026-02-02T15:00:00Z', '2026-02-02T17:30:00Z',
 'Focused design work', 'Office', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Deep focus on the onboarding navigation prototype, building out the sidebar flow.', '["design", "figma", "focus", "deep-work", "onboarding", "navigation", "sidebar"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 65),

('ev_b0701', 'day_2026-02-02', '2026-02-02T17:30:00Z', '2026-02-02T18:30:00Z',
 'Lunch', 'Office', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Ate the meal-prepped chicken bowl at desk.', '["food"]', '["place_demo_office"]',
 NULL, NULL, NULL, 'NEW', 73),

('ev_b0702', 'day_2026-02-02', '2026-02-02T18:30:00Z', '2026-02-02T22:00:00Z',
 'Afternoon work', 'Office', '["app_usage", "message"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Finished the onboarding navigation prototype and shared with the team for async review.', '["work", "design", "onboarding", "navigation", "code-review"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 65),

('ev_b0703', 'day_2026-02-02', '2026-02-02T22:00:00Z', '2026-02-02T22:30:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 FALSE, TRUE, FALSE, FALSE, FALSE,

 'Bike commute home.', '["commute", "cycling"]', '[]',
 NULL, NULL, NULL, 'NEW', 124),

('ev_b0704', 'day_2026-02-02', '2026-02-02T22:30:00Z', '2026-02-03T04:00:00Z',
 'Evening at home', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Heated up leftovers and spent the evening reading.', '["food", "leisure", "reflection"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 68) ON CONFLICT DO NOTHING;

-- =============================================================================
-- Tuesday, February 3, 2026 (10 events — run in evening)
-- =============================================================================

INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES
('ev_b0705', 'day_2026-02-03', '2026-02-03T06:00:00Z', '2026-02-03T12:30:00Z',
 'Sleep', 'Home', '["sleep"]',
 FALSE, FALSE, TRUE, FALSE, FALSE,

 'Overnight sleep, about 6.5 hours.', '["sleep"]', '[]',
 NULL, NULL, NULL, 'NEW', 56),

('ev_b0706', 'day_2026-02-03', '2026-02-03T12:30:00Z', '2026-02-03T13:15:00Z',
 'Morning routine', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Coffee and caught up on texts.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 63),

('ev_b0707', 'day_2026-02-03', '2026-02-03T13:15:00Z', '2026-02-03T13:45:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 FALSE, TRUE, FALSE, FALSE, FALSE,

 'Bike commute to the office.', '["commute", "cycling"]', '[]',
 NULL, NULL, NULL, 'NEW', 130),

('ev_b0708', 'day_2026-02-03', '2026-02-03T13:45:00Z', '2026-02-03T14:15:00Z',
 'Coffee and Slack', 'Office', '["app_usage", "message"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Grabbed coffee and reviewed feedback on the nav prototype from yesterday.', '["messaging", "work", "coffee", "code-review", "navigation"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 65),

('ev_b0709', 'day_2026-02-03', '2026-02-03T14:15:00Z', '2026-02-03T15:00:00Z',
 'Design standup', 'Office', '["calendar", "message", "transcription"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Tuesday standup with Maya and David, reviewed async nav prototype feedback.', '["meeting", "standup", "design", "onboarding", "navigation"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 71),

('ev_b0710', 'day_2026-02-03', '2026-02-03T15:00:00Z', '2026-02-03T16:00:00Z',
 'Design review', 'Office', '["calendar", "app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Design review with David on the onboarding form validation iteration.', '["meeting", "design-review", "design", "onboarding", "form-validation"]', '["person_demo_david", "place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 78),

('ev_b0711', 'day_2026-02-03', '2026-02-03T16:00:00Z', '2026-02-03T17:30:00Z',
 'Focused design work', 'Office', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Applied review feedback to the onboarding error states.', '["design", "figma", "focus", "onboarding", "form-validation"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 68),

('ev_b0712', 'day_2026-02-03', '2026-02-03T17:30:00Z', '2026-02-03T18:30:00Z',
 'Lunch', 'Office', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Meal-prepped chicken bowl for lunch at desk.', '["food"]', '["place_demo_office"]',
 NULL, NULL, NULL, 'NEW', 73),

('ev_b0713', 'day_2026-02-03', '2026-02-03T18:30:00Z', '2026-02-03T22:00:00Z',
 'Afternoon work', 'Office', '["app_usage", "message"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Afternoon of onboarding flow refinements and Slack conversations with engineering.', '["work", "design", "onboarding", "messaging", "code-review"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 66),

('ev_b0714', 'day_2026-02-03', '2026-02-03T23:30:00Z', '2026-02-04T01:00:00Z',
 'Evening run', 'Mueller Trails', '["steps", "location_visit"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Evening 3.5-mile run on Mueller trails, felt strong.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
 NULL, NULL, NULL, 'NEW', 66) ON CONFLICT DO NOTHING;

-- =============================================================================
-- Wednesday, February 4, 2026 (10 events — Ramen with Maya)
-- =============================================================================

INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES
('ev_b0715', 'day_2026-02-04', '2026-02-04T06:00:00Z', '2026-02-04T12:30:00Z',
 'Sleep', 'Home', '["sleep"]',
 FALSE, FALSE, TRUE, FALSE, FALSE,

 'Overnight sleep, about 6.5 hours.', '["sleep"]', '[]',
 NULL, NULL, NULL, 'NEW', 62),

('ev_b0716', 'day_2026-02-04', '2026-02-04T12:30:00Z', '2026-02-04T13:15:00Z',
 'Morning routine', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Morning coffee and checking Slack.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 66),

('ev_b0717', 'day_2026-02-04', '2026-02-04T13:15:00Z', '2026-02-04T13:45:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 FALSE, TRUE, FALSE, FALSE, FALSE,

 'Bike commute to the office.', '["commute", "cycling", "podcast"]', '[]',
 NULL, NULL, NULL, 'NEW', 116),

('ev_b0718', 'day_2026-02-04', '2026-02-04T13:45:00Z', '2026-02-04T14:15:00Z',
 'Coffee and Slack', 'Office', '["app_usage", "message"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Morning coffee and caught up on Slack.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 71),

('ev_b0719', 'day_2026-02-04', '2026-02-04T14:15:00Z', '2026-02-04T15:00:00Z',
 'Design standup', 'Office', '["calendar", "message", "transcription"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Wednesday standup, discussed onboarding funnel conversion improvements with Maya and David.', '["meeting", "standup", "design", "onboarding"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 70),

('ev_b0720', 'day_2026-02-04', '2026-02-04T15:00:00Z', '2026-02-04T17:30:00Z',
 'Focused design work', 'Office', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Built out the onboarding success confirmation screens in Figma.', '["design", "figma", "focus", "deep-work", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 63),

('ev_b0721', 'day_2026-02-04', '2026-02-04T17:30:00Z', '2026-02-04T18:30:00Z',
 'Lunch at Ramen Tatsu-ya', 'Ramen Tatsu-ya', '["location_visit"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Wednesday ramen lunch with Maya, talked about upcoming user research sessions.', '["food", "social", "ramen"]', '["person_demo_maya", "place_demo_ramen"]',
 NULL, NULL, NULL, 'NEW', 76),

('ev_b0722', 'day_2026-02-04', '2026-02-04T18:30:00Z', '2026-02-04T22:30:00Z',
 'Afternoon work', 'Office', '["app_usage", "message"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Afternoon session refining the onboarding page layout with micro-interactions.', '["work", "design", "figma", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 64),

('ev_b0723', 'day_2026-02-04', '2026-02-04T22:30:00Z', '2026-02-04T23:00:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 FALSE, TRUE, FALSE, FALSE, FALSE,

 'Bike commute home.', '["commute", "cycling"]', '[]',
 NULL, NULL, NULL, 'NEW', 122),

('ev_b0724', 'day_2026-02-04', '2026-02-04T23:00:00Z', '2026-02-05T04:00:00Z',
 'Evening at home', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Made a salad for dinner and watched a couple episodes of a show.', '["food", "leisure"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 64) ON CONFLICT DO NOTHING;

-- =============================================================================
-- Thursday, February 5, 2026 (10 events — WFH afternoon, run)
-- =============================================================================

INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES
('ev_b0725', 'day_2026-02-05', '2026-02-05T06:00:00Z', '2026-02-05T12:30:00Z',
 'Sleep', 'Home', '["sleep"]',
 FALSE, FALSE, TRUE, FALSE, FALSE,

 'Overnight sleep, about 6.5 hours.', '["sleep"]', '[]',
 NULL, NULL, NULL, 'NEW', 62),

('ev_b0726', 'day_2026-02-05', '2026-02-05T12:30:00Z', '2026-02-05T13:15:00Z',
 'Morning routine', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Coffee and quick Slack check.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 65),

('ev_b0727', 'day_2026-02-05', '2026-02-05T13:15:00Z', '2026-02-05T13:45:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 FALSE, TRUE, FALSE, FALSE, FALSE,

 'Bike commute to the office.', '["commute", "cycling", "podcast"]', '[]',
 NULL, NULL, NULL, 'NEW', 123),

('ev_b0728', 'day_2026-02-05', '2026-02-05T13:45:00Z', '2026-02-05T14:15:00Z',
 'Coffee and Slack', 'Office', '["app_usage", "message"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Morning coffee and Slack threads about the onboarding eng handoff.', '["messaging", "work", "coffee", "onboarding", "code-review"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 72),

('ev_b0729', 'day_2026-02-05', '2026-02-05T14:15:00Z', '2026-02-05T15:00:00Z',
 'Design standup', 'Office', '["calendar", "message", "transcription"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Thursday standup, David raised edge cases in the onboarding form validation.', '["meeting", "standup", "design", "onboarding", "form-validation"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 72),

('ev_b0730', 'day_2026-02-05', '2026-02-05T15:00:00Z', '2026-02-05T17:30:00Z',
 'Focused design work', 'Office', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Worked through David''s edge case list for the form validation screens.', '["design", "figma", "focus", "deep-work", "onboarding", "form-validation"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 63),

('ev_b0731', 'day_2026-02-05', '2026-02-05T17:30:00Z', '2026-02-05T18:15:00Z',
 'Lunch', 'Office', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Quick sandwich at the office.', '["food"]', '["place_demo_office"]',
 NULL, NULL, NULL, 'NEW', 74),

('ev_b0732', 'day_2026-02-05', '2026-02-05T18:30:00Z', '2026-02-05T22:30:00Z',
 'WFH afternoon', 'Home', '["app_usage", "message"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Worked from home in the afternoon, finishing onboarding edge case designs and writing Jira tickets.', '["work", "design", "onboarding", "form-validation", "focus"]', '["place_demo_home", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 65),

('ev_b0733', 'day_2026-02-05', '2026-02-05T23:30:00Z', '2026-02-06T01:00:00Z',
 'Evening run', 'Mueller Trails', '["steps", "location_visit"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Ran 3 miles on Mueller trails at sunset.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
 NULL, NULL, NULL, 'NEW', 60),

('ev_b0734', 'day_2026-02-05', '2026-02-06T01:00:00Z', '2026-02-06T04:00:00Z',
 'Evening wind-down', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Showered, ate leftovers, and read before bed.', '["leisure", "food", "reflection"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 68) ON CONFLICT DO NOTHING;

-- =============================================================================
-- Friday, February 6, 2026 (10 events — Mom call, Game night at Jess's)
-- =============================================================================

INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES
('ev_b0735', 'day_2026-02-06', '2026-02-06T06:00:00Z', '2026-02-06T12:30:00Z',
 'Sleep', 'Home', '["sleep"]',
 FALSE, FALSE, TRUE, FALSE, FALSE,

 'Overnight sleep, about 6.5 hours.', '["sleep"]', '[]',
 NULL, NULL, NULL, 'NEW', 55),

('ev_b0736', 'day_2026-02-06', '2026-02-06T12:30:00Z', '2026-02-06T13:15:00Z',
 'Morning routine', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Morning coffee and texts with Jess confirming game night tonight.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 68),

('ev_b0737', 'day_2026-02-06', '2026-02-06T13:15:00Z', '2026-02-06T13:45:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 FALSE, TRUE, FALSE, FALSE, FALSE,

 'Bike commute to the office.', '["commute", "cycling"]', '[]',
 NULL, NULL, NULL, 'NEW', 120),

('ev_b0738', 'day_2026-02-06', '2026-02-06T13:45:00Z', '2026-02-06T14:15:00Z',
 'Coffee and Slack', 'Office', '["app_usage", "message"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Friday morning coffee and Slack.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 65),

('ev_b0739', 'day_2026-02-06', '2026-02-06T14:15:00Z', '2026-02-06T15:00:00Z',
 'Design standup', 'Office', '["calendar", "message", "transcription"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Friday standup, wrapped up the week on onboarding with Maya and David.', '["meeting", "standup", "design", "onboarding"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 70),

('ev_b0740', 'day_2026-02-06', '2026-02-06T15:00:00Z', '2026-02-06T17:30:00Z',
 'Focused design work', 'Office', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Morning focus on the onboarding progress indicator component.', '["design", "figma", "focus", "deep-work", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 66),

('ev_b0741', 'day_2026-02-06', '2026-02-06T17:30:00Z', '2026-02-06T18:30:00Z',
 'Lunch', 'Office', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Lunch at desk, salad from the cafe.', '["food"]', '["place_demo_office"]',
 NULL, NULL, NULL, 'NEW', 77),

('ev_b0742', 'day_2026-02-06', '2026-02-06T22:00:00Z', '2026-02-06T23:00:00Z',
 'Phone call with Mom', 'Home', '["transcription"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Weekly call with Mom, talked about her book club and weekend plans.', '["family", "phone-call"]', '["person_demo_mom", "place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 67),

('ev_b0743', 'day_2026-02-06', '2026-02-07T00:00:00Z', '2026-02-07T00:30:00Z',
 'Drive to Jess''s', NULL, '["location_visit"]',
 FALSE, TRUE, FALSE, FALSE, FALSE,

 'Drove to Jess''s place on South Lamar for game night.', '["commute", "driving"]', '[]',
 NULL, NULL, NULL, 'NEW', 68),

('ev_b0744', 'day_2026-02-06', '2026-02-07T00:30:00Z', '2026-02-07T05:00:00Z',
 'Game night', 'Jess''s Place', '["location_visit"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Game night at Jess''s with Jess and Priya, played Catan and Codenames.', '["social", "games"]', '["person_demo_jess", "person_demo_priya", "place_demo_jess"]',
 NULL, NULL, NULL, 'NEW', 69) ON CONFLICT DO NOTHING;

-- =============================================================================
-- Saturday, February 7, 2026 (7 events — Mom call already done Fri, quiet day)
-- =============================================================================

INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES
('ev_b0745', 'day_2026-02-07', '2026-02-07T06:00:00Z', '2026-02-07T14:00:00Z',
 'Sleep', 'Home', '["sleep"]',
 FALSE, FALSE, TRUE, FALSE, FALSE,

 'Slept in after game night, about 8 hours.', '["sleep"]', '[]',
 NULL, NULL, NULL, 'NEW', 57),

('ev_b0746', 'day_2026-02-07', '2026-02-07T14:00:00Z', '2026-02-07T15:30:00Z',
 'Slow morning', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Slow Saturday morning, coffee and catching up on news.', '["routine", "morning", "coffee", "browsing"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 63),

('ev_b0747', 'day_2026-02-07', '2026-02-07T15:30:00Z', '2026-02-07T17:00:00Z',
 'Errands', NULL, '["location_visit"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Grocery run and picked up a new book at BookPeople.', '["routine", "driving"]', '[]',
 NULL, NULL, NULL, 'NEW', 81),

('ev_b0748', 'day_2026-02-07', '2026-02-07T17:00:00Z', '2026-02-07T19:00:00Z',
 'Mueller trails run', 'Mueller Trails', '["steps", "location_visit"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Saturday afternoon run on Mueller trails, 4 miles.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
 NULL, NULL, NULL, 'NEW', 146),

('ev_b0749', 'day_2026-02-07', '2026-02-07T19:00:00Z', '2026-02-07T21:00:00Z',
 'Afternoon reading', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Read the new book for a couple hours on the couch.', '["leisure", "reflection"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 61),

('ev_b0750', 'day_2026-02-07', '2026-02-07T23:00:00Z', '2026-02-08T02:00:00Z',
 'Dinner and movie', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Made curry for dinner and watched a movie at home.', '["food", "leisure"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 71),

('ev_b0751', 'day_2026-02-07', '2026-02-08T02:00:00Z', '2026-02-08T04:00:00Z',
 'Wind down', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Browsed the internet and texted with Jess about last night''s game.', '["leisure", "messaging", "browsing"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 58) ON CONFLICT DO NOTHING;

-- =============================================================================
-- Sunday, February 8, 2026 (7 events — Lady Bird Lake walk, relaxing)
-- =============================================================================

INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES
('ev_b0752', 'day_2026-02-08', '2026-02-08T06:00:00Z', '2026-02-08T13:30:00Z',
 'Sleep', 'Home', '["sleep"]',
 FALSE, FALSE, TRUE, FALSE, FALSE,

 'Overnight sleep, about 7.5 hours.', '["sleep"]', '[]',
 NULL, NULL, NULL, 'NEW', 58),

('ev_b0753', 'day_2026-02-08', '2026-02-08T13:30:00Z', '2026-02-08T15:00:00Z',
 'Slow morning', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Sunday morning, made eggs and read the new book.', '["routine", "morning", "coffee", "food"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 67),

('ev_b0754', 'day_2026-02-08', '2026-02-08T15:00:00Z', '2026-02-08T17:00:00Z',
 'Walk at Lady Bird Lake', 'Lady Bird Lake', '["steps", "location_visit"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Long walk at Lady Bird Lake, sunny and mild afternoon.', '["exercise", "outdoors"]', '["place_demo_ladybird"]',
 NULL, NULL, NULL, 'NEW', 86),

('ev_b0755', 'day_2026-02-08', '2026-02-08T17:00:00Z', '2026-02-08T19:00:00Z',
 'Reading', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Continued the new book at home with tea.', '["leisure", "reflection"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 59),

('ev_b0756', 'day_2026-02-08', '2026-02-08T19:00:00Z', '2026-02-08T21:00:00Z',
 'Meal prep', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Prepped lunches for the week, roasted vegetables and grain bowls.', '["food", "routine"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 76),

('ev_b0757', 'day_2026-02-08', '2026-02-08T21:00:00Z', '2026-02-09T01:00:00Z',
 'Evening at home', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Watched a couple episodes of a show and planned the work week ahead.', '["leisure", "reflection"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 68),

('ev_b0758', 'day_2026-02-08', '2026-02-09T01:00:00Z', '2026-02-09T04:00:00Z',
 'Wind down', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Browsed the web and headed to bed early.', '["leisure", "browsing"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 60) ON CONFLICT DO NOTHING;

-- =============================================================================
-- Monday, February 9, 2026 (10 events)
-- =============================================================================

INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES
('ev_b0759', 'day_2026-02-09', '2026-02-09T06:00:00Z', '2026-02-09T12:30:00Z',
 'Sleep', 'Home', '["sleep"]',
 FALSE, FALSE, TRUE, FALSE, FALSE,

 'Overnight sleep, about 6.5 hours.', '["sleep"]', '[]',
 NULL, NULL, NULL, 'NEW', 59),

('ev_b0760', 'day_2026-02-09', '2026-02-09T12:30:00Z', '2026-02-09T13:15:00Z',
 'Morning routine', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Coffee and checking Slack for Monday updates.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 64),

('ev_b0761', 'day_2026-02-09', '2026-02-09T13:15:00Z', '2026-02-09T13:45:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 FALSE, TRUE, FALSE, FALSE, FALSE,

 'Bike commute to the office.', '["commute", "cycling", "podcast"]', '[]',
 NULL, NULL, NULL, 'NEW', 131),

('ev_b0762', 'day_2026-02-09', '2026-02-09T13:45:00Z', '2026-02-09T14:15:00Z',
 'Coffee and Slack', 'Office', '["app_usage", "message"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Grabbed coffee and caught up on engineering threads about onboarding implementation.', '["messaging", "work", "coffee", "onboarding", "code-review"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 70),

('ev_b0763', 'day_2026-02-09', '2026-02-09T14:15:00Z', '2026-02-09T15:00:00Z',
 'Design standup', 'Office', '["calendar", "message", "transcription"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Monday standup with Maya and David, planning the onboarding user research sessions for this week.', '["meeting", "standup", "design", "onboarding", "ux-research"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 73),

('ev_b0764', 'day_2026-02-09', '2026-02-09T15:00:00Z', '2026-02-09T17:30:00Z',
 'Focused design work', 'Office', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Prepared onboarding user research discussion guide and prototype walkthrough.', '["design", "ux-research", "usability-testing", "onboarding", "focus"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 64),

('ev_b0765', 'day_2026-02-09', '2026-02-09T17:30:00Z', '2026-02-09T18:30:00Z',
 'Lunch', 'Office', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Ate the meal-prepped grain bowl at desk.', '["food"]', '["place_demo_office"]',
 NULL, NULL, NULL, 'NEW', 76),

('ev_b0766', 'day_2026-02-09', '2026-02-09T18:30:00Z', '2026-02-09T22:30:00Z',
 'Afternoon work', 'Office', '["app_usage", "message"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Continued onboarding research prep and coordinated participant scheduling with Maya.', '["work", "ux-research", "usability-testing", "onboarding"]', '["person_demo_maya", "place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 66),

('ev_b0767', 'day_2026-02-09', '2026-02-09T22:30:00Z', '2026-02-09T23:00:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 FALSE, TRUE, FALSE, FALSE, FALSE,

 'Bike commute home.', '["commute", "cycling"]', '[]',
 NULL, NULL, NULL, 'NEW', 131),

('ev_b0768', 'day_2026-02-09', '2026-02-09T23:00:00Z', '2026-02-10T04:00:00Z',
 'Dinner and relaxing', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Heated up soup for dinner and watched a documentary about architecture.', '["food", "leisure"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 69) ON CONFLICT DO NOTHING;

-- =============================================================================
-- Tuesday, February 10, 2026 (10 events — run in evening)
-- =============================================================================

INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES
('ev_b0769', 'day_2026-02-10', '2026-02-10T06:00:00Z', '2026-02-10T12:30:00Z',
 'Sleep', 'Home', '["sleep"]',
 FALSE, FALSE, TRUE, FALSE, FALSE,

 'Overnight sleep, about 6.5 hours.', '["sleep"]', '[]',
 NULL, NULL, NULL, 'NEW', 62),

('ev_b0770', 'day_2026-02-10', '2026-02-10T12:30:00Z', '2026-02-10T13:15:00Z',
 'Morning routine', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Morning coffee and checking messages.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 65),

('ev_b0771', 'day_2026-02-10', '2026-02-10T13:15:00Z', '2026-02-10T13:45:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 FALSE, TRUE, FALSE, FALSE, FALSE,

 'Bike commute to the office.', '["commute", "cycling"]', '[]',
 NULL, NULL, NULL, 'NEW', 134),

('ev_b0772', 'day_2026-02-10', '2026-02-10T13:45:00Z', '2026-02-10T14:15:00Z',
 'Coffee and Slack', 'Office', '["app_usage", "message"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Coffee and Slack, reviewed participant confirmations for user research.', '["messaging", "work", "coffee", "ux-research"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 66),

('ev_b0773', 'day_2026-02-10', '2026-02-10T14:15:00Z', '2026-02-10T15:00:00Z',
 'Design standup', 'Office', '["calendar", "message", "transcription"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Tuesday standup with Maya and David, finalized the onboarding research plan.', '["meeting", "standup", "design", "onboarding", "ux-research"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 70),

('ev_b0774', 'day_2026-02-10', '2026-02-10T15:00:00Z', '2026-02-10T16:00:00Z',
 'Design review', 'Office', '["calendar", "app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Design review with David on the final onboarding flow before user testing.', '["meeting", "design-review", "design", "onboarding", "usability-testing"]', '["person_demo_david", "place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 75),

('ev_b0775', 'day_2026-02-10', '2026-02-10T16:00:00Z', '2026-02-10T17:30:00Z',
 'Focused design work', 'Office', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Last round of polish on the onboarding prototype before research sessions.', '["design", "figma", "focus", "onboarding", "usability-testing"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 66),

('ev_b0776', 'day_2026-02-10', '2026-02-10T17:30:00Z', '2026-02-10T18:30:00Z',
 'Lunch', 'Office', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Grain bowl at desk.', '["food"]', '["place_demo_office"]',
 NULL, NULL, NULL, 'NEW', 71),

('ev_b0777', 'day_2026-02-10', '2026-02-10T18:30:00Z', '2026-02-10T22:00:00Z',
 'Afternoon work', 'Office', '["app_usage", "message"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Afternoon spent writing the user research session scripts and sharing with the team.', '["work", "ux-research", "usability-testing", "onboarding", "recording"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 65),

('ev_b0778', 'day_2026-02-10', '2026-02-10T23:30:00Z', '2026-02-11T01:00:00Z',
 'Evening run', 'Mueller Trails', '["steps", "location_visit"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Evening 3-mile run on Mueller trails.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
 NULL, NULL, NULL, 'NEW', 68) ON CONFLICT DO NOTHING;

-- =============================================================================
-- Wednesday, February 11, 2026 (10 events — Ramen with Maya, LAST DAY)
-- =============================================================================

INSERT INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES
('ev_b0779', 'day_2026-02-11', '2026-02-11T06:00:00Z', '2026-02-11T12:30:00Z',
 'Sleep', 'Home', '["sleep"]',
 FALSE, FALSE, TRUE, FALSE, FALSE,

 'Overnight sleep, about 6.5 hours.', '["sleep"]', '[]',
 NULL, NULL, NULL, 'NEW', 58),

('ev_b0780', 'day_2026-02-11', '2026-02-11T12:30:00Z', '2026-02-11T13:15:00Z',
 'Morning routine', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Coffee and Slack, checking on user research logistics.', '["routine", "morning", "coffee", "messaging", "ux-research"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 67),

('ev_b0781', 'day_2026-02-11', '2026-02-11T13:15:00Z', '2026-02-11T13:45:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 FALSE, TRUE, FALSE, FALSE, FALSE,

 'Bike commute to the office.', '["commute", "cycling", "podcast"]', '[]',
 NULL, NULL, NULL, 'NEW', 118),

('ev_b0782', 'day_2026-02-11', '2026-02-11T13:45:00Z', '2026-02-11T14:15:00Z',
 'Coffee and Slack', 'Office', '["app_usage", "message"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Morning coffee and Slack, reviewed final research session plans.', '["messaging", "work", "coffee", "ux-research"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 67),

('ev_b0783', 'day_2026-02-11', '2026-02-11T14:15:00Z', '2026-02-11T15:00:00Z',
 'Design standup', 'Office', '["calendar", "message", "transcription"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Wednesday standup with Maya and David, confirmed the first user research session for Thursday.', '["meeting", "standup", "design", "onboarding", "ux-research", "usability-testing"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 75),

('ev_b0784', 'day_2026-02-11', '2026-02-11T15:00:00Z', '2026-02-11T17:30:00Z',
 'Focused design work', 'Office', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Final tweaks to the onboarding prototype for the user research walkthrough.', '["design", "figma", "focus", "onboarding", "usability-testing"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 62),

('ev_b0785', 'day_2026-02-11', '2026-02-11T17:30:00Z', '2026-02-11T18:30:00Z',
 'Lunch at Ramen Tatsu-ya', 'Ramen Tatsu-ya', '["location_visit"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Wednesday ramen with Maya, talked about being nervous for the first onboarding research session.', '["food", "social", "ramen"]', '["person_demo_maya", "place_demo_ramen"]',
 NULL, NULL, NULL, 'NEW', 73),

('ev_b0786', 'day_2026-02-11', '2026-02-11T18:30:00Z', '2026-02-11T22:30:00Z',
 'Afternoon work', 'Office', '["app_usage", "message"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Afternoon preparing research materials and coordinating with the product team.', '["work", "ux-research", "usability-testing", "onboarding", "recording"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 69),

('ev_b0787', 'day_2026-02-11', '2026-02-11T22:30:00Z', '2026-02-11T23:00:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 FALSE, TRUE, FALSE, FALSE, FALSE,

 'Bike commute home.', '["commute", "cycling"]', '[]',
 NULL, NULL, NULL, 'NEW', 119),

('ev_b0788', 'day_2026-02-11', '2026-02-11T23:00:00Z', '2026-02-12T04:00:00Z',
 'Evening at home', 'Home', '["app_usage"]',
 FALSE, FALSE, FALSE, FALSE, FALSE,

 'Made stir fry for dinner and read before bed, looking forward to the research session tomorrow.', '["food", "leisure", "reflection"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 62) ON CONFLICT DO NOTHING;
