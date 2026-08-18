-- =============================================================================
-- Demo Day Seed: Friday, February 13, 2026 — Austin, TX
-- =============================================================================
--
-- Character: UX designer, lives in Mueller (East Austin), works downtown.
-- Narrative: Routine morning → work → house showing pivot → run → friends night.
--
-- This seed populates:
--   1. data_location_visit       (10 visits — office split AM/PM, home return, Lady Bird Lake)
--   2. wiki_days                 (3 — primary day + 2 adjacent for cross-day scoring)
--   3. wiki_events               (24 — 13 for Feb 13, 6 for Feb 12, 5 for Feb 14)
--   4. data_health_sleep         (4 sleep records with phase-level stages)
--   5. data_calendar_event       (5 calendar events)
--   6. data_communication_message (9 Slack + text messages)
--   7. data_activity_app_session   (9 app sessions)
--   8. data_health_steps         (14 step readings)
--   9. data_health_heart_rate    (12 HR readings during run)
--  10. data_health_workout       (1 run)
--  11. data_communication_transcription (5 recordings)
--  12. data_location_point       (28 GPS breadcrumbs)
--
-- All times are UTC. America/Chicago = UTC-6 in February.
-- So 06:30 CST = 12:30 UTC, midnight CST = 06:00 UTC.
--
-- Usage: psql "$DATABASE_URL" -f core/seeds/demo_day.sql
-- =============================================================================

-- ─────────────────────────────────────────────────────────────────────────────
-- 0. CLEAR CONFLICTING AUTO-CREATED ROWS
-- ─────────────────────────────────────────────────────────────────────────────
-- Production seed auto-creates wiki_days with hashed IDs. Our demo uses
-- deterministic IDs (day_2026-02-13). Delete any auto-created rows for our
-- dates so INSERT OR IGNORE doesn't skip our rows.
DELETE FROM wiki_events WHERE day_id IN (SELECT id FROM wiki_days WHERE date IN ('2026-02-12', '2026-02-13', '2026-02-14'));
DELETE FROM wiki_days WHERE date IN ('2026-02-12', '2026-02-13', '2026-02-14');

-- ─────────────────────────────────────────────────────────────────────────────
-- 1. LOCATION VISITS
-- ─────────────────────────────────────────────────────────────────────────────
-- Feb 13 visits (times in UTC; CST = UTC-6)

-- Home (Mueller) — overnight + morning
INSERT INTO data_location_visit (
    id, place_name, latitude, longitude,
    started_at, ended_at, duration_minutes,
    source_stream_id, source_table, source_provider
) VALUES (
    'lv_demo_home_morning', 'Home', 30.2989, -97.7055,
    '2026-02-13T04:00:00Z', '2026-02-13T13:15:00Z', 555,
    'demo_lv_001', 'data_location_visit', 'demo'
) ON CONFLICT DO NOTHING;

-- Office (Downtown Austin) — morning session, before lunch
INSERT INTO data_location_visit (
    id, place_name, latitude, longitude,
    started_at, ended_at, duration_minutes,
    source_stream_id, source_table, source_provider
) VALUES (
    'lv_demo_office_am', 'Office', 30.2672, -97.7431,
    '2026-02-13T13:45:00Z', '2026-02-13T17:25:00Z', 220,
    'demo_lv_002a', 'data_location_visit', 'demo'
) ON CONFLICT DO NOTHING;

-- Office (Downtown Austin) — afternoon session, after lunch
INSERT INTO data_location_visit (
    id, place_name, latitude, longitude,
    started_at, ended_at, duration_minutes,
    source_stream_id, source_table, source_provider
) VALUES (
    'lv_demo_office_pm', 'Office', 30.2672, -97.7431,
    '2026-02-13T18:30:00Z', '2026-02-13T20:28:00Z', 118,
    'demo_lv_002b', 'data_location_visit', 'demo'
) ON CONFLICT DO NOTHING;

-- Ramen Tatsu-ya (lunch)
INSERT INTO data_location_visit (
    id, place_name, latitude, longitude,
    started_at, ended_at, duration_minutes,
    source_stream_id, source_table, source_provider
) VALUES (
    'lv_demo_ramen', 'Ramen Tatsu-ya', 30.2700, -97.7400,
    '2026-02-13T17:30:00Z', '2026-02-13T18:30:00Z', 60,
    'demo_lv_003', 'data_location_visit', 'demo'
) ON CONFLICT DO NOTHING;

-- Trader Joe's Seaholm (the anomaly — first visit in user's logged history)
INSERT INTO data_location_visit (
    id, place_name, latitude, longitude,
    started_at, ended_at, duration_minutes,
    source_stream_id, source_table, source_provider
) VALUES (
    'lv_demo_trader_joes', 'Trader Joe''s — Seaholm', 30.2696, -97.7521,
    '2026-02-13T22:12:00Z', '2026-02-13T23:04:00Z', 52,
    'demo_lv_004', 'data_location_visit', 'demo'
) ON CONFLICT DO NOTHING;

-- Home (Mueller) — evening return after the TJ's detour
INSERT INTO data_location_visit (
    id, place_name, latitude, longitude,
    started_at, ended_at, duration_minutes,
    source_stream_id, source_table, source_provider
) VALUES (
    'lv_demo_home_evening', 'Home', 30.2989, -97.7055,
    '2026-02-13T23:34:00Z', '2026-02-14T05:00:00Z', 326,
    'demo_lv_005', 'data_location_visit', 'demo'
) ON CONFLICT DO NOTHING;

-- Feb 14: Lady Bird Lake walk
INSERT INTO data_location_visit (
    id, place_name, latitude, longitude,
    started_at, ended_at, duration_minutes,
    source_stream_id, source_table, source_provider
) VALUES (
    'lv_demo_ladybird', 'Lady Bird Lake', 30.2615, -97.7480,
    '2026-02-14T17:30:00Z', '2026-02-14T19:00:00Z', 90,
    'demo_lv_009', 'data_location_visit', 'demo'
) ON CONFLICT DO NOTHING;

-- ─────────────────────────────────────────────────────────────────────────────
-- 2. WIKI DAYS
-- ─────────────────────────────────────────────────────────────────────────────

-- Primary day: Feb 13 (the detailed one)
INSERT INTO wiki_days (
    id, date, start_timezone, morning_baseline, epigraph
) VALUES
(
    'day_2026-02-13', '2026-02-13', 'America/Chicago',
    0.52,
    'The Trader Joe''s detour'
) ON CONFLICT DO NOTHING;

-- Data quality for primary demo day
UPDATE wiki_days SET data_quality = '{"coverage":{"who":4,"whom":3,"what":5,"when":5,"where":5,"why":2,"how":4},"overall":4,"note":"Strong location, financial, and biometric coverage anchored by a single specific anomaly. Lower signal on motivation and broader social context."}' WHERE date = '2026-02-13';

-- Readiness scores (0-100, computed from overnight HRV/RHR/sleep)
UPDATE wiki_days SET readiness_score = 68, readiness_details = '{"hrv":62,"rhr":72,"sleep_duration":78,"deep_rem":55,"consistency":70}' WHERE date = '2026-02-13';
UPDATE wiki_days SET readiness_score = 82, readiness_details = '{"hrv":85,"rhr":80,"sleep_duration":90,"deep_rem":75,"consistency":78}' WHERE date = '2026-02-12';
UPDATE wiki_days SET readiness_score = 59, readiness_details = '{"hrv":50,"rhr":65,"sleep_duration":72,"deep_rem":42,"consistency":60}' WHERE date = '2026-02-14';

-- Adjacent day: Feb 12 (routine Thursday — for cross-day comparison)
INSERT INTO wiki_days (
    id, date, start_timezone, morning_baseline
) VALUES
(
    'day_2026-02-12', '2026-02-12', 'America/Chicago',
    0.48
) ON CONFLICT DO NOTHING;

-- Adjacent day: Feb 14 (Valentine's Saturday — slightly different texture)
INSERT INTO wiki_days (
    id, date, start_timezone, morning_baseline
) VALUES
(
    'day_2026-02-14', '2026-02-14', 'America/Chicago',
    0.55
) ON CONFLICT DO NOTHING;

-- ─────────────────────────────────────────────────────────────────────────────
-- 3. WIKI EVENTS — Feb 13 (13 events, partial day ending ~5:30 PM CST)
-- ─────────────────────────────────────────────────────────────────────────────
-- E01: Sleep (00:00-06:30 CST = 06:00-12:30 UTC)
-- Single sleep event for the narrative timeline. Cycle-level scoring is derived
-- at query time from data_health_sleep stages + heart rate data.
INSERT INTO wiki_events (
    id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, event_summary, topics, entities, novelty_z, autonomic_z, agent_action, avg_hr
) VALUES (
    'ev_demo_01', 'day_2026-02-13',
    '2026-02-13T06:00:00Z', '2026-02-13T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',

    'Slept 6.5 hours. Four full cycles with good deep sleep in the first two. One brief wake-up around 3 AM.', '["sleep"]', '[]',
    NULL, -1.1, 'NEW', 56
) ON CONFLICT DO NOTHING;

-- E02: Morning routine (06:30-07:15 CST = 12:30-13:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, event_summary, topics, entities, novelty_z, autonomic_z, agent_action, avg_hr
) VALUES (
    'ev_demo_02', 'day_2026-02-13',
    '2026-02-13T12:30:00Z', '2026-02-13T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',

    'Coffee and checking Slack at home before the commute. Three unread PRs from overnight, quick scan of the design channel.', '["routine", "morning"]', '["place_demo_home"]',
    -2.055, -0.3, 'NEW', 66
) ON CONFLICT DO NOTHING;

-- E03: Bike commute (07:15-07:45 CST = 13:15-13:45 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, autonomic_z, agent_action, avg_hr
) VALUES (
    'ev_demo_03', 'day_2026-02-13',
    '2026-02-13T13:15:00Z', '2026-02-13T13:45:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]', FALSE, FALSE,

    'Bike commute from Mueller to downtown office. Listened to a Daily Stoic episode on patience. Cool morning, light traffic on the Speedway bike lane.', '["commute", "podcast"]', '["place_demo_home", "place_demo_office"]',
    -1.346, -0.6, 'NEW', 122
) ON CONFLICT DO NOTHING;

-- E04: Coffee and Slack (07:45-08:15 CST = 13:45-14:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, event_summary, topics, entities, novelty_z, autonomic_z, agent_action, avg_hr
) VALUES (
    'ev_demo_04', 'day_2026-02-13',
    '2026-02-13T13:45:00Z', '2026-02-13T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',

    'Grabbed coffee at the office kitchen, caught up on Slack threads and reviewed Maya''s Figma comments on the nav redesign.', '["messaging", "design-review"]', '["place_demo_office", "org_demo_employer", "person_demo_maya"]',
    -1.450, 0.1, 'NEW', 67
) ON CONFLICT DO NOTHING;

-- E05: Design standup (08:15-09:00 CST = 14:15-15:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, event_summary, topics, entities, novelty_z, autonomic_z, agent_action, avg_hr
) VALUES (
    'ev_demo_05', 'day_2026-02-13',
    '2026-02-13T14:15:00Z', '2026-02-13T15:00:00Z',
    'Design standup', 'Office', '["calendar", "message", "transcription"]',

    'Design team standup with Maya and David. Main topic was the onboarding funnel drop-off at step 3 — form validation errors causing 40% abandonment. Agreed to prototype a simplified flow by Monday.', '["standup", "onboarding", "form-validation", "ux-research", "design", "hiring"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
    1.443, 0.8, 'NEW', 74
) ON CONFLICT DO NOTHING;

-- E06: Focused design work (09:00-11:30 CST = 15:00-17:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, event_summary, topics, entities, novelty_z, autonomic_z, agent_action, avg_hr
) VALUES (
    'ev_demo_06', 'day_2026-02-13',
    '2026-02-13T15:00:00Z', '2026-02-13T17:30:00Z',
    'Focused design work', 'Office', '["app_usage"]',

    'Deep work session in Figma on the navigation redesign. Explored three layout variants for the sidebar collapse pattern. Hit flow state around 10 AM — no Slack interruptions for 90 minutes.', '["design", "figma", "deep-work"]', '["place_demo_office", "org_demo_employer"]',
    0.086, -0.5, 'NEW', 63
) ON CONFLICT DO NOTHING;

-- E07: Lunch with Maya (11:30-12:30 CST = 17:30-18:30 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, event_summary, topics, entities, novelty_z, autonomic_z, agent_action, avg_hr
) VALUES (
    'ev_demo_07', 'day_2026-02-13',
    '2026-02-13T17:30:00Z', '2026-02-13T18:30:00Z',
    'Lunch with Maya', 'Ramen Tatsu-ya', '["location_visit", "calendar", "transcription"]',

    'Lunch at Ramen Tatsu-ya with Maya. She''s unsure about the new hire — talked through the tradeoffs of senior vs mid-level for the open role. Also debated whether the nav redesign needs user testing before the sprint demo.', '["social", "ramen", "hiring", "team-decisions", "design"]', '["person_demo_maya", "place_demo_ramen"]',
    0.855, 1.2, 'NEW', 76
) ON CONFLICT DO NOTHING;

-- E08: User research session (12:30-14:15 CST = 18:30-20:15 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, event_summary, topics, entities, novelty_z, autonomic_z, agent_action, avg_hr
) VALUES (
    'ev_demo_08', 'day_2026-02-13',
    '2026-02-13T18:30:00Z', '2026-02-13T20:15:00Z',
    'User research session', 'Office', '["calendar", "transcription"]',

    'Moderated usability test with three participants on the navigation redesign. Participant 2 found the breadcrumb pattern confusing — strong signal to revisit. Recorded all sessions for the team async review.', '["ux-research", "usability-testing", "navigation", "breadcrumbs", "design", "recording"]', '["place_demo_office", "org_demo_employer"]',
    0.752, 0.7, 'NEW', 79
) ON CONFLICT DO NOTHING;

-- E09: Office afternoon — design iteration (14:15-16:00 CST = 20:15-22:00 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, event_summary, topics, entities, novelty_z, autonomic_z, agent_action, avg_hr
) VALUES (
    'ev_demo_09', 'day_2026-02-13',
    '2026-02-13T20:15:00Z', '2026-02-13T22:00:00Z',
    'Office afternoon', 'Office', '["app_usage", "message"]',

    'Synthesised the research session findings in Figma. Wrote up the breadcrumb confusion + label-on-icon recommendations and posted them to the design channel before wrapping for the day.', '["design", "figma", "wrap-up"]', '["place_demo_office", "org_demo_employer"]',
    -0.18, -0.2, 'NEW', 65
) ON CONFLICT DO NOTHING;

-- E10: Drive to Trader Joe's (16:00-16:12 CST = 22:00-22:12 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, autonomic_z, agent_action, avg_hr
) VALUES (
    'ev_demo_10', 'day_2026-02-13',
    '2026-02-13T22:00:00Z', '2026-02-13T22:12:00Z',
    'Drive to Seaholm', NULL, '["location_visit"]', FALSE, FALSE,

    'Twelve-minute drive from the downtown office south to the Seaholm district. Detour off the usual commute home.', '["commute", "errand"]', '[]',
    -0.4, 0.2, 'NEW', 71
) ON CONFLICT DO NOTHING;

-- E11: Trader Joe's grocery run — the anomaly (16:12-17:04 CST = 22:12-23:04 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, event_summary, topics, entities, novelty_z, autonomic_z, agent_action, avg_hr
) VALUES (
    'ev_demo_11', 'day_2026-02-13',
    '2026-02-13T22:12:00Z', '2026-02-13T23:04:00Z',
    'Trader Joe''s — Seaholm', 'Trader Joe''s — Seaholm', '["location_visit", "financial_transaction"]',

    'First visit on record to the Seaholm Trader Joe''s. Lingered 52 minutes in the aisles and checked out at $328.50 — versus a $45 average for routine Friday grocery runs. Heaviest single grocery transaction in the trailing 90 days.', '["groceries", "errand", "anomaly"]', '[]',
    2.3, 0.6, 'NEW', 78
) ON CONFLICT DO NOTHING;

-- E12: Drive home with voice memo (17:04-17:34 CST = 23:04-23:34 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, is_user_added, is_user_edited, event_summary, topics, entities, novelty_z, autonomic_z, agent_action, avg_hr
) VALUES (
    'ev_demo_12', 'day_2026-02-13',
    '2026-02-13T23:04:00Z', '2026-02-13T23:34:00Z',
    'Drive home', NULL, '["location_visit", "transcription"]', FALSE, FALSE,

    'Drove from Seaholm back to Mueller with a trunk full of groceries. Recorded a short voice memo to Maya from the car: "I just bought enough snacks to survive a winter."', '["commute", "voice-memo"]', '["person_demo_maya", "place_demo_home"]',
    -0.3, -0.4, 'NEW', 68
) ON CONFLICT DO NOTHING;

-- E13: Unpacking groceries (17:34-18:15 CST = 23:34-00:15+1 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, event_summary, topics, entities, novelty_z, autonomic_z, agent_action, avg_hr
) VALUES (
    'ev_demo_13', 'day_2026-02-13',
    '2026-02-13T23:34:00Z', '2026-02-14T00:15:00Z',
    'Unpacking groceries', 'Home', '["app_usage"]',

    'Carried five bags in from the car and reorganised the pantry to fit everything. Put on a Khruangbin album while sorting the freezer items.', '["routine", "groceries"]', '["place_demo_home"]',
    -0.9, -0.7, 'NEW', 70
) ON CONFLICT DO NOTHING;

-- E14: Dinner and TV (18:15-19:30 CST = 00:15-01:30+1 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, event_summary, topics, entities, novelty_z, autonomic_z, agent_action, avg_hr
) VALUES (
    'ev_demo_14', 'day_2026-02-13',
    '2026-02-14T00:15:00Z', '2026-02-14T01:30:00Z',
    'Dinner and TV', 'Home', '["app_usage"]',

    'Made dinner from the new haul — frozen mandarin chicken and a bag salad — and ate on the couch through two episodes of Severance.', '["leisure", "tv", "dinner"]', '["place_demo_home"]',
    -1.7, -1.1, 'NEW', 63
) ON CONFLICT DO NOTHING;

-- E15: Reading and wind-down (19:30-21:00 CST = 01:30-03:00+1 UTC)
INSERT INTO wiki_events (
    id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, event_summary, topics, entities, novelty_z, autonomic_z, agent_action, avg_hr
) VALUES (
    'ev_demo_15', 'day_2026-02-13',
    '2026-02-14T01:30:00Z', '2026-02-14T03:00:00Z',
    'Reading', 'Home', '["app_usage"]',

    'Read 40 pages of Meditations. Lights out around 9. Resting HR before sleep was 60, the lowest reading of the week — HRV trending high through the night.', '["reading", "wind-down"]', '["place_demo_home"]',
    -1.0, -1.4, 'NEW', 60
) ON CONFLICT DO NOTHING;

-- ─────────────────────────────────────────────────────────────────────────────
-- 4. WIKI EVENTS — Feb 12 (simple routine day, 6 events)
-- ─────────────────────────────────────────────────────────────────────────────

INSERT INTO wiki_events (
    id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, event_summary, topics, entities, novelty_z, autonomic_z, agent_action, avg_hr
) VALUES
('ev_feb12_01', 'day_2026-02-12', '2026-02-12T06:00:00Z', '2026-02-12T13:00:00Z',
 'Sleep', 'Home', '["sleep"]',
 'Overnight sleep, 7 hours.', '["sleep"]', '[]',
 NULL, -0.9, 'NEW', 56),
('ev_feb12_02', 'day_2026-02-12', '2026-02-12T13:00:00Z', '2026-02-12T14:00:00Z',
 'Morning routine', 'Home', '["app_usage"]',
 'Morning routine at home, coffee and news.', '["routine", "morning"]', '["place_demo_home"]',
 -0.806, -0.2, 'NEW', 65),
('ev_feb12_03', 'day_2026-02-12', '2026-02-12T14:00:00Z', '2026-02-12T18:00:00Z',
 'Work from home', 'Home', '["app_usage", "message"]',
 'Worked from home on design iteration for the settings page.', '["work", "design"]', '["place_demo_home", "org_demo_employer"]',
 0.066, 0.1, 'NEW', 64),
('ev_feb12_04', 'day_2026-02-12', '2026-02-12T18:00:00Z', '2026-02-12T23:00:00Z',
 'Office work', 'Office', '["app_usage", "calendar", "message"]',
 'Afternoon at the office, settings page review meeting with David.', '["work", "meeting"]', '["person_demo_david", "place_demo_office", "org_demo_employer"]',
 -0.432, 0.3, 'NEW', 68),
('ev_feb12_05', 'day_2026-02-12', '2026-02-12T23:00:00Z', '2026-02-13T01:00:00Z',
 'Dinner and reading', 'Home', '["app_usage"]',
 'Leftovers for dinner and reading at home.', '["routine", "leisure"]', '["place_demo_home"]',
 0.071, -0.6, 'NEW', 64),
('ev_feb12_06', 'day_2026-02-12', '2026-02-13T01:00:00Z', '2026-02-13T06:00:00Z',
 'Sleep', 'Home', '["sleep"]',
 'Overnight sleep, 5 hours before Friday.', '["sleep"]', '[]',
 NULL, -0.8, 'NEW', 58) ON CONFLICT DO NOTHING;

-- ─────────────────────────────────────────────────────────────────────────────
-- 5. WIKI EVENTS — Feb 14 (quiet Saturday, 5 events)
-- ─────────────────────────────────────────────────────────────────────────────

INSERT INTO wiki_events (
    id, day_id, start_time, end_time, auto_label, auto_location, source_ontologies, event_summary, topics, entities, novelty_z, autonomic_z, agent_action, avg_hr
) VALUES
('ev_feb14_01', 'day_2026-02-14', '2026-02-14T06:00:00Z', '2026-02-14T12:30:00Z',
 'Sleep', 'Home', '["sleep"]',
 'Overnight sleep after game night, 6.5 hours. Less deep than usual early on.', '["sleep"]', '[]',
 NULL, -1.0, 'NEW', 58),
('ev_feb14_02', 'day_2026-02-14', '2026-02-14T13:00:00Z', '2026-02-14T17:00:00Z',
 'Sprint demo and office', 'Office', '["calendar", "message", "app_usage"]',
 'Biweekly sprint demo showing navigation redesign progress, then early afternoon.', '["work", "meeting", "design"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
 0.484, 0.5, 'NEW', 73),
('ev_feb14_03', 'day_2026-02-14', '2026-02-14T17:30:00Z', '2026-02-14T19:00:00Z',
 'Walk at Lady Bird Lake', 'Lady Bird Lake', '["steps", "location_visit"]',
 'Walked Lady Bird Lake for 90 minutes on Saturday afternoon.', '["exercise", "outdoors"]', '["place_demo_ladybird"]',
 0.429, 0.2, 'NEW', 92),
('ev_feb14_04', 'day_2026-02-14', '2026-02-14T19:30:00Z', '2026-02-14T20:30:00Z',
 'Phone call with Mom', 'Home', '["transcription"]',
 'Weekly phone call with Mom, talked about the house and Dad''s knee surgery.', '["family", "phone-call"]', '["person_demo_mom", "place_demo_home"]',
 0.870, 0.6, 'NEW', 71),
('ev_feb14_05', 'day_2026-02-14', '2026-02-14T21:00:00Z', '2026-02-15T06:00:00Z',
 'Dinner and movie', 'Home', '["app_usage"]',
 'Made pasta and watched a movie at home, quiet Valentine''s Saturday evening.', '["leisure", "food"]', '["place_demo_home"]',
 -0.613, -0.7, 'NEW', 66) ON CONFLICT DO NOTHING;

-- =============================================================================
-- 6. ONTOLOGY SOURCE DATA
-- =============================================================================
-- These are the raw data records that feed into day summary generation via
-- the virtues-api pipeline. Events reference these via source_ontologies.

-- ─────────────────────────────────────────────────────────────────────────────
-- 6a. SLEEP (data_health_sleep)
-- ─────────────────────────────────────────────────────────────────────────────

-- Feb 12-13 overnight sleep (7pm-midnight CST = 01:00-06:00 UTC Feb 13, 5 hours — short night)
-- 3 compressed cycles: heavy deep early, truncated final REM
INSERT INTO data_health_sleep (
    id, start_time, end_time, duration_minutes, sleep_quality_score,
    sleep_stages,
    source_stream_id, source_table, source_provider
) VALUES (
    'sleep_demo_feb12', '2026-02-13T01:00:00Z', '2026-02-13T06:00:00Z', 300, 0.62,
    '[
        {"stage": "awake",       "start": "2026-02-13T01:00:00Z", "end": "2026-02-13T01:08:00Z"},
        {"stage": "asleep_core", "start": "2026-02-13T01:08:00Z", "end": "2026-02-13T01:30:00Z"},
        {"stage": "asleep_deep", "start": "2026-02-13T01:30:00Z", "end": "2026-02-13T02:05:00Z"},
        {"stage": "asleep_core", "start": "2026-02-13T02:05:00Z", "end": "2026-02-13T02:20:00Z"},
        {"stage": "asleep_rem",  "start": "2026-02-13T02:20:00Z", "end": "2026-02-13T02:28:00Z"},
        {"stage": "awake",       "start": "2026-02-13T02:28:00Z", "end": "2026-02-13T02:30:00Z"},
        {"stage": "asleep_core", "start": "2026-02-13T02:30:00Z", "end": "2026-02-13T02:55:00Z"},
        {"stage": "asleep_deep", "start": "2026-02-13T02:55:00Z", "end": "2026-02-13T03:20:00Z"},
        {"stage": "asleep_core", "start": "2026-02-13T03:20:00Z", "end": "2026-02-13T03:35:00Z"},
        {"stage": "asleep_rem",  "start": "2026-02-13T03:35:00Z", "end": "2026-02-13T03:50:00Z"},
        {"stage": "awake",       "start": "2026-02-13T03:50:00Z", "end": "2026-02-13T03:52:00Z"},
        {"stage": "asleep_core", "start": "2026-02-13T03:52:00Z", "end": "2026-02-13T04:20:00Z"},
        {"stage": "asleep_deep", "start": "2026-02-13T04:20:00Z", "end": "2026-02-13T04:35:00Z"},
        {"stage": "asleep_core", "start": "2026-02-13T04:35:00Z", "end": "2026-02-13T04:50:00Z"},
        {"stage": "asleep_rem",  "start": "2026-02-13T04:50:00Z", "end": "2026-02-13T05:05:00Z"},
        {"stage": "asleep_core", "start": "2026-02-13T05:05:00Z", "end": "2026-02-13T05:40:00Z"},
        {"stage": "asleep_rem",  "start": "2026-02-13T05:40:00Z", "end": "2026-02-13T05:52:00Z"},
        {"stage": "awake",       "start": "2026-02-13T05:52:00Z", "end": "2026-02-13T06:00:00Z"}
    ]',
    'demo_sleep_001', 'data_health_sleep', 'demo'
) ON CONFLICT DO NOTHING;

-- Feb 13 overnight sleep (midnight-6:30am CST = 06:00-12:30 UTC, 6.5 hours)
-- 4 full cycles: good deep in first two, increasing REM
INSERT INTO data_health_sleep (
    id, start_time, end_time, duration_minutes, sleep_quality_score,
    sleep_stages,
    source_stream_id, source_table, source_provider
) VALUES (
    'sleep_demo_feb13', '2026-02-13T06:00:00Z', '2026-02-13T12:30:00Z', 390, 0.78,
    '[
        {"stage": "awake",       "start": "2026-02-13T06:00:00Z", "end": "2026-02-13T06:10:00Z"},
        {"stage": "asleep_core", "start": "2026-02-13T06:10:00Z", "end": "2026-02-13T06:35:00Z"},
        {"stage": "asleep_deep", "start": "2026-02-13T06:35:00Z", "end": "2026-02-13T07:10:00Z"},
        {"stage": "asleep_core", "start": "2026-02-13T07:10:00Z", "end": "2026-02-13T07:25:00Z"},
        {"stage": "asleep_rem",  "start": "2026-02-13T07:25:00Z", "end": "2026-02-13T07:32:00Z"},
        {"stage": "awake",       "start": "2026-02-13T07:32:00Z", "end": "2026-02-13T07:34:00Z"},
        {"stage": "asleep_core", "start": "2026-02-13T07:34:00Z", "end": "2026-02-13T08:00:00Z"},
        {"stage": "asleep_deep", "start": "2026-02-13T08:00:00Z", "end": "2026-02-13T08:30:00Z"},
        {"stage": "asleep_core", "start": "2026-02-13T08:30:00Z", "end": "2026-02-13T08:45:00Z"},
        {"stage": "asleep_rem",  "start": "2026-02-13T08:45:00Z", "end": "2026-02-13T09:00:00Z"},
        {"stage": "awake",       "start": "2026-02-13T09:00:00Z", "end": "2026-02-13T09:03:00Z"},
        {"stage": "asleep_core", "start": "2026-02-13T09:03:00Z", "end": "2026-02-13T09:30:00Z"},
        {"stage": "asleep_deep", "start": "2026-02-13T09:30:00Z", "end": "2026-02-13T09:45:00Z"},
        {"stage": "asleep_core", "start": "2026-02-13T09:45:00Z", "end": "2026-02-13T10:05:00Z"},
        {"stage": "asleep_rem",  "start": "2026-02-13T10:05:00Z", "end": "2026-02-13T10:25:00Z"},
        {"stage": "awake",       "start": "2026-02-13T10:25:00Z", "end": "2026-02-13T10:27:00Z"},
        {"stage": "asleep_core", "start": "2026-02-13T10:27:00Z", "end": "2026-02-13T10:55:00Z"},
        {"stage": "asleep_deep", "start": "2026-02-13T10:55:00Z", "end": "2026-02-13T11:05:00Z"},
        {"stage": "asleep_core", "start": "2026-02-13T11:05:00Z", "end": "2026-02-13T11:20:00Z"},
        {"stage": "asleep_rem",  "start": "2026-02-13T11:20:00Z", "end": "2026-02-13T11:50:00Z"},
        {"stage": "asleep_core", "start": "2026-02-13T11:50:00Z", "end": "2026-02-13T12:15:00Z"},
        {"stage": "awake",       "start": "2026-02-13T12:15:00Z", "end": "2026-02-13T12:30:00Z"}
    ]',
    'demo_sleep_002', 'data_health_sleep', 'demo'
) ON CONFLICT DO NOTHING;

-- Feb 14 overnight sleep (midnight-6:30am CST = 06:00-12:30 UTC, 6.5 hours — after game night)
-- 4 cycles: slightly less deep sleep due to late alcohol, more fragmented
INSERT INTO data_health_sleep (
    id, start_time, end_time, duration_minutes, sleep_quality_score,
    sleep_stages,
    source_stream_id, source_table, source_provider
) VALUES (
    'sleep_demo_feb14', '2026-02-14T06:00:00Z', '2026-02-14T12:30:00Z', 390, 0.71,
    '[
        {"stage": "awake",       "start": "2026-02-14T06:00:00Z", "end": "2026-02-14T06:12:00Z"},
        {"stage": "asleep_core", "start": "2026-02-14T06:12:00Z", "end": "2026-02-14T06:40:00Z"},
        {"stage": "asleep_deep", "start": "2026-02-14T06:40:00Z", "end": "2026-02-14T07:10:00Z"},
        {"stage": "asleep_core", "start": "2026-02-14T07:10:00Z", "end": "2026-02-14T07:30:00Z"},
        {"stage": "asleep_rem",  "start": "2026-02-14T07:30:00Z", "end": "2026-02-14T07:35:00Z"},
        {"stage": "awake",       "start": "2026-02-14T07:35:00Z", "end": "2026-02-14T07:38:00Z"},
        {"stage": "asleep_core", "start": "2026-02-14T07:38:00Z", "end": "2026-02-14T08:05:00Z"},
        {"stage": "asleep_deep", "start": "2026-02-14T08:05:00Z", "end": "2026-02-14T08:30:00Z"},
        {"stage": "asleep_core", "start": "2026-02-14T08:30:00Z", "end": "2026-02-14T08:50:00Z"},
        {"stage": "asleep_rem",  "start": "2026-02-14T08:50:00Z", "end": "2026-02-14T09:02:00Z"},
        {"stage": "awake",       "start": "2026-02-14T09:02:00Z", "end": "2026-02-14T09:06:00Z"},
        {"stage": "asleep_core", "start": "2026-02-14T09:06:00Z", "end": "2026-02-14T09:35:00Z"},
        {"stage": "asleep_deep", "start": "2026-02-14T09:35:00Z", "end": "2026-02-14T09:45:00Z"},
        {"stage": "asleep_core", "start": "2026-02-14T09:45:00Z", "end": "2026-02-14T10:05:00Z"},
        {"stage": "asleep_rem",  "start": "2026-02-14T10:05:00Z", "end": "2026-02-14T10:20:00Z"},
        {"stage": "awake",       "start": "2026-02-14T10:20:00Z", "end": "2026-02-14T10:23:00Z"},
        {"stage": "asleep_core", "start": "2026-02-14T10:23:00Z", "end": "2026-02-14T10:50:00Z"},
        {"stage": "asleep_core", "start": "2026-02-14T10:50:00Z", "end": "2026-02-14T11:10:00Z"},
        {"stage": "asleep_rem",  "start": "2026-02-14T11:10:00Z", "end": "2026-02-14T11:40:00Z"},
        {"stage": "asleep_core", "start": "2026-02-14T11:40:00Z", "end": "2026-02-14T12:10:00Z"},
        {"stage": "awake",       "start": "2026-02-14T12:10:00Z", "end": "2026-02-14T12:30:00Z"}
    ]',
    'demo_sleep_003', 'data_health_sleep', 'demo'
) ON CONFLICT DO NOTHING;

-- Feb 14-15 overnight sleep (3pm CST Sat = 21:00 UTC to midnight CST Sun = 06:00 UTC, 9 hours — recovery)
-- 5 full cycles: excellent deep sleep early, long REM later, well-rested
INSERT INTO data_health_sleep (
    id, start_time, end_time, duration_minutes, sleep_quality_score,
    sleep_stages,
    source_stream_id, source_table, source_provider
) VALUES (
    'sleep_demo_feb14_night', '2026-02-14T21:00:00Z', '2026-02-15T06:00:00Z', 540, 0.91,
    '[
        {"stage": "awake",       "start": "2026-02-14T21:00:00Z", "end": "2026-02-14T21:07:00Z"},
        {"stage": "asleep_core", "start": "2026-02-14T21:07:00Z", "end": "2026-02-14T21:30:00Z"},
        {"stage": "asleep_deep", "start": "2026-02-14T21:30:00Z", "end": "2026-02-14T22:10:00Z"},
        {"stage": "asleep_core", "start": "2026-02-14T22:10:00Z", "end": "2026-02-14T22:25:00Z"},
        {"stage": "asleep_rem",  "start": "2026-02-14T22:25:00Z", "end": "2026-02-14T22:32:00Z"},
        {"stage": "awake",       "start": "2026-02-14T22:32:00Z", "end": "2026-02-14T22:34:00Z"},
        {"stage": "asleep_core", "start": "2026-02-14T22:34:00Z", "end": "2026-02-14T23:00:00Z"},
        {"stage": "asleep_deep", "start": "2026-02-14T23:00:00Z", "end": "2026-02-14T23:35:00Z"},
        {"stage": "asleep_core", "start": "2026-02-14T23:35:00Z", "end": "2026-02-14T23:50:00Z"},
        {"stage": "asleep_rem",  "start": "2026-02-14T23:50:00Z", "end": "2026-02-15T00:05:00Z"},
        {"stage": "awake",       "start": "2026-02-15T00:05:00Z", "end": "2026-02-15T00:07:00Z"},
        {"stage": "asleep_core", "start": "2026-02-15T00:07:00Z", "end": "2026-02-15T00:35:00Z"},
        {"stage": "asleep_deep", "start": "2026-02-15T00:35:00Z", "end": "2026-02-15T00:55:00Z"},
        {"stage": "asleep_core", "start": "2026-02-15T00:55:00Z", "end": "2026-02-15T01:15:00Z"},
        {"stage": "asleep_rem",  "start": "2026-02-15T01:15:00Z", "end": "2026-02-15T01:40:00Z"},
        {"stage": "awake",       "start": "2026-02-15T01:40:00Z", "end": "2026-02-15T01:42:00Z"},
        {"stage": "asleep_core", "start": "2026-02-15T01:42:00Z", "end": "2026-02-15T02:10:00Z"},
        {"stage": "asleep_deep", "start": "2026-02-15T02:10:00Z", "end": "2026-02-15T02:22:00Z"},
        {"stage": "asleep_core", "start": "2026-02-15T02:22:00Z", "end": "2026-02-15T02:45:00Z"},
        {"stage": "asleep_rem",  "start": "2026-02-15T02:45:00Z", "end": "2026-02-15T03:15:00Z"},
        {"stage": "awake",       "start": "2026-02-15T03:15:00Z", "end": "2026-02-15T03:16:00Z"},
        {"stage": "asleep_core", "start": "2026-02-15T03:16:00Z", "end": "2026-02-15T03:45:00Z"},
        {"stage": "asleep_core", "start": "2026-02-15T03:45:00Z", "end": "2026-02-15T04:10:00Z"},
        {"stage": "asleep_rem",  "start": "2026-02-15T04:10:00Z", "end": "2026-02-15T04:45:00Z"},
        {"stage": "asleep_core", "start": "2026-02-15T04:45:00Z", "end": "2026-02-15T05:30:00Z"},
        {"stage": "asleep_rem",  "start": "2026-02-15T05:30:00Z", "end": "2026-02-15T05:50:00Z"},
        {"stage": "awake",       "start": "2026-02-15T05:50:00Z", "end": "2026-02-15T06:00:00Z"}
    ]',
    'demo_sleep_004', 'data_health_sleep', 'demo'
) ON CONFLICT DO NOTHING;

-- ─────────────────────────────────────────────────────────────────────────────
-- 6b. CALENDAR EVENTS (data_calendar_event)
-- ─────────────────────────────────────────────────────────────────────────────

-- Feb 13: Design standup (08:15-09:00 CST = 14:15-15:00 UTC)
INSERT INTO data_calendar_event (
    id, title, description, calendar_name, event_type, status,
    organizer_identifier, attendee_identifiers,
    location_name, conference_url, conference_platform,
    start_time, end_time, timezone,
    source_stream_id, source_table, source_provider
) VALUES (
    'cal_demo_standup', 'Design Team Standup', 'Daily sync — blockers, progress, plan for the day',
    'Work', 'meeting', 'confirmed',
    'maya.chen@company.com', '["maya.chen@company.com", "david.okafor@company.com", "demo-user@company.com"]',
    NULL, 'https://meet.google.com/abc-defg-hij', 'Google Meet',
    '2026-02-13T14:15:00Z', '2026-02-13T15:00:00Z', 'America/Chicago',
    'demo_cal_001', 'data_calendar_event', 'demo'
) ON CONFLICT DO NOTHING;

-- Feb 13: Lunch with Maya (11:30-12:30 CST = 17:30-18:30 UTC)
INSERT INTO data_calendar_event (
    id, title, description, calendar_name, event_type, status,
    organizer_identifier, attendee_identifiers,
    location_name,
    start_time, end_time, timezone,
    source_stream_id, source_table, source_provider
) VALUES (
    'cal_demo_lunch', 'Lunch', NULL,
    'Personal', 'event', 'confirmed',
    'demo-user@company.com', '["maya.chen@company.com"]',
    'Ramen Tatsu-ya',
    '2026-02-13T17:30:00Z', '2026-02-13T18:30:00Z', 'America/Chicago',
    'demo_cal_002', 'data_calendar_event', 'demo'
) ON CONFLICT DO NOTHING;

-- Feb 13: User Research Session (12:30-14:15 CST = 18:30-20:15 UTC)
INSERT INTO data_calendar_event (
    id, title, description, calendar_name, event_type, status,
    organizer_identifier, attendee_identifiers,
    location_name, conference_url, conference_platform,
    start_time, end_time, timezone,
    source_stream_id, source_table, source_provider
) VALUES (
    'cal_demo_research', 'User Research: Navigation Redesign', 'Moderated usability testing with 3 participants. Focus: main nav patterns and settings discoverability.',
    'Work', 'meeting', 'confirmed',
    'demo-user@company.com', '["participant-1@external.com", "participant-2@external.com", "participant-3@external.com"]',
    'Conference Room B', NULL, NULL,
    '2026-02-13T18:30:00Z', '2026-02-13T20:15:00Z', 'America/Chicago',
    'demo_cal_003', 'data_calendar_event', 'demo'
) ON CONFLICT DO NOTHING;

-- Feb 12: Settings page review (for adjacent day)
INSERT INTO data_calendar_event (
    id, title, description, calendar_name, event_type, status,
    organizer_identifier, attendee_identifiers,
    start_time, end_time, timezone,
    source_stream_id, source_table, source_provider
) VALUES (
    'cal_demo_feb12', 'Settings Page Review', 'Async design review of settings iteration',
    'Work', 'meeting', 'confirmed',
    'demo-user@company.com', '["david.okafor@company.com"]',
    '2026-02-12T19:00:00Z', '2026-02-12T20:00:00Z', 'America/Chicago',
    'demo_cal_004', 'data_calendar_event', 'demo'
) ON CONFLICT DO NOTHING;

-- Feb 14: Sprint Demo (for adjacent day)
INSERT INTO data_calendar_event (
    id, title, description, calendar_name, event_type, status,
    organizer_identifier, attendee_identifiers,
    conference_url, conference_platform,
    start_time, end_time, timezone,
    source_stream_id, source_table, source_provider
) VALUES (
    'cal_demo_feb14', 'Sprint Demo', 'Biweekly sprint demo — show navigation redesign progress',
    'Work', 'meeting', 'confirmed',
    'maya.chen@company.com', '["maya.chen@company.com", "david.okafor@company.com", "demo-user@company.com"]',
    'https://meet.google.com/abc-defg-hij', 'Google Meet',
    '2026-02-14T14:00:00Z', '2026-02-14T15:00:00Z', 'America/Chicago',
    'demo_cal_005', 'data_calendar_event', 'demo'
) ON CONFLICT DO NOTHING;

-- ─────────────────────────────────────────────────────────────────────────────
-- 6c. MESSAGES (data_communication_message)
-- ─────────────────────────────────────────────────────────────────────────────

-- Slack: Maya re standup agenda (07:50 CST = 13:50 UTC)
INSERT INTO data_communication_message (
    id, message_id, thread_id, channel, body,
    from_identifier, from_name, to_identifiers,
    is_read, is_group_message, occurred_at,
    source_stream_id, source_table, source_provider
) VALUES (
    'msg_demo_01', 'slack_msg_001', 'thread_standup_feb13', '#design-team',
    'heads up — I want to talk about the onboarding flow today. got some concerns about the drop-off data',
    'maya.chen@company.com', 'Maya Chen', '["#design-team"]',
    TRUE, TRUE, '2026-02-13T13:50:00Z',
    'demo_msg_001', 'data_communication_message', 'demo'
) ON CONFLICT DO NOTHING;

-- Slack: David reply (07:55 CST = 13:55 UTC)
INSERT INTO data_communication_message (
    id, message_id, thread_id, channel, body,
    from_identifier, from_name, to_identifiers,
    is_read, is_group_message, occurred_at,
    source_stream_id, source_table, source_provider
) VALUES (
    'msg_demo_02', 'slack_msg_002', 'thread_standup_feb13', '#design-team',
    'yeah the step 3 completion rate is brutal. maybe we should look at the form validation UX',
    'david.okafor@company.com', 'David Okafor', '["#design-team"]',
    TRUE, TRUE, '2026-02-13T13:55:00Z',
    'demo_msg_002', 'data_communication_message', 'demo'
) ON CONFLICT DO NOTHING;

-- Slack: User reply (08:05 CST = 14:05 UTC)
INSERT INTO data_communication_message (
    id, message_id, thread_id, channel, body,
    from_identifier, from_name, to_identifiers,
    is_read, is_group_message, occurred_at,
    source_stream_id, source_table, source_provider
) VALUES (
    'msg_demo_03', 'slack_msg_003', 'thread_standup_feb13', '#design-team',
    'pulling up the funnel data now. will have it on screen for standup',
    'demo-user@company.com', NULL, '["#design-team"]',
    TRUE, TRUE, '2026-02-13T14:05:00Z',
    'demo_msg_003', 'data_communication_message', 'demo'
) ON CONFLICT DO NOTHING;

-- Text: Rachel Torres about house (14:20 CST = 20:20 UTC)
INSERT INTO data_communication_message (
    id, message_id, channel, body,
    from_identifier, from_name, to_identifiers,
    is_read, is_group_message, occurred_at,
    source_stream_id, source_table, source_provider
) VALUES (
    'msg_demo_04', 'sms_msg_001', 'sms',
    'Hey! That Bouldin Creek house on S 3rd just came back on market. The one with the big backyard. Want to see it today? I can meet you at 3.',
    'rachel.torres@realty.com', 'Rachel Torres', '["demo-user@phone.com"]',
    TRUE, FALSE, '2026-02-13T20:20:00Z',
    'demo_msg_004', 'data_communication_message', 'demo'
) ON CONFLICT DO NOTHING;

-- Text: User reply to Rachel (14:22 CST = 20:22 UTC)
INSERT INTO data_communication_message (
    id, message_id, channel, body,
    from_identifier, from_name, to_identifiers,
    is_read, is_group_message, occurred_at,
    source_stream_id, source_table, source_provider
) VALUES (
    'msg_demo_05', 'sms_msg_002', 'sms',
    'omg yes!! I can leave work early. see you at 3',
    'demo-user@phone.com', NULL, '["rachel.torres@realty.com"]',
    TRUE, FALSE, '2026-02-13T20:22:00Z',
    'demo_msg_005', 'data_communication_message', 'demo'
) ON CONFLICT DO NOTHING;

-- Text: Rachel confirmation (14:25 CST = 20:25 UTC)
INSERT INTO data_communication_message (
    id, message_id, channel, body,
    from_identifier, from_name, to_identifiers,
    is_read, is_group_message, occurred_at,
    source_stream_id, source_table, source_provider
) VALUES (
    'msg_demo_06', 'sms_msg_003', 'sms',
    'Perfect — 1847 S 3rd St. I''ll be out front. You''re going to love this one.',
    'rachel.torres@realty.com', 'Rachel Torres', '["demo-user@phone.com"]',
    TRUE, FALSE, '2026-02-13T20:25:00Z',
    'demo_msg_006', 'data_communication_message', 'demo'
) ON CONFLICT DO NOTHING;

-- (Evening messages omitted — partial day)

-- ─────────────────────────────────────────────────────────────────────────────
-- 6d. APP USAGE (data_activity_app_session)
-- ─────────────────────────────────────────────────────────────────────────────

-- Morning: Instagram scroll (06:35-06:50 CST = 12:35-12:50 UTC)
INSERT INTO data_activity_app_session (
    id, app_name, app_bundle_id, app_category,
    start_time, end_time, window_title,
    source_stream_id, source_table, source_provider
) VALUES (
    'app_demo_01', 'Instagram', 'com.burbn.instagram', 'Social',
    '2026-02-13T12:35:00Z', '2026-02-13T12:50:00Z', NULL,
    'demo_app_001', 'data_activity_app_session', 'demo'
) ON CONFLICT DO NOTHING;

-- Morning: Apple News (06:50-07:05 CST = 12:50-13:05 UTC)
INSERT INTO data_activity_app_session (
    id, app_name, app_bundle_id, app_category,
    start_time, end_time, window_title,
    source_stream_id, source_table, source_provider
) VALUES (
    'app_demo_02', 'Apple News', 'com.apple.news', 'News',
    '2026-02-13T12:50:00Z', '2026-02-13T13:05:00Z', NULL,
    'demo_app_002', 'data_activity_app_session', 'demo'
) ON CONFLICT DO NOTHING;

-- Pre-standup: Slack desktop (07:45-08:15 CST = 13:45-14:15 UTC)
INSERT INTO data_activity_app_session (
    id, app_name, app_bundle_id, app_category,
    start_time, end_time, window_title,
    source_stream_id, source_table, source_provider
) VALUES (
    'app_demo_03', 'Slack', 'com.tinyspeck.slackmacgap', 'Productivity',
    '2026-02-13T13:45:00Z', '2026-02-13T14:15:00Z', '#design-team',
    'demo_app_003', 'data_activity_app_session', 'demo'
) ON CONFLICT DO NOTHING;

-- Deep work: Figma (09:05-11:25 CST = 15:05-17:25 UTC)
INSERT INTO data_activity_app_session (
    id, app_name, app_bundle_id, app_category,
    start_time, end_time, window_title,
    source_stream_id, source_table, source_provider
) VALUES (
    'app_demo_04', 'Figma', 'com.figma.desktop', 'Design',
    '2026-02-13T15:05:00Z', '2026-02-13T17:25:00Z', 'Navigation Redesign v3 — Figma',
    'demo_app_004', 'data_activity_app_session', 'demo'
) ON CONFLICT DO NOTHING;

-- Post-standup: Notion docs (09:00-09:05 CST = 15:00-15:05 UTC)
INSERT INTO data_activity_app_session (
    id, app_name, app_bundle_id, app_category,
    start_time, end_time, window_title, url,
    source_stream_id, source_table, source_provider
) VALUES (
    'app_demo_05', 'Notion', 'notion.id', 'Productivity',
    '2026-02-13T15:00:00Z', '2026-02-13T15:05:00Z',
    'Standup Notes — Feb 13', 'https://notion.so/standup-feb-13',
    'demo_app_005', 'data_activity_app_session', 'demo'
) ON CONFLICT DO NOTHING;

-- (Evening app usage omitted — partial day)

-- Feb 12: Figma (adjacent day)
INSERT INTO data_activity_app_session (
    id, app_name, app_bundle_id, app_category,
    start_time, end_time, window_title,
    source_stream_id, source_table, source_provider
) VALUES (
    'app_demo_08', 'Figma', 'com.figma.desktop', 'Design',
    '2026-02-12T15:00:00Z', '2026-02-12T17:30:00Z', 'Settings Page v2 — Figma',
    'demo_app_008', 'data_activity_app_session', 'demo'
) ON CONFLICT DO NOTHING;

-- Feb 12: Slack (adjacent day)
INSERT INTO data_activity_app_session (
    id, app_name, app_bundle_id, app_category,
    start_time, end_time, window_title,
    source_stream_id, source_table, source_provider
) VALUES (
    'app_demo_09', 'Slack', 'com.tinyspeck.slackmacgap', 'Productivity',
    '2026-02-12T18:00:00Z', '2026-02-12T22:30:00Z', '#design-team',
    'demo_app_009', 'data_activity_app_session', 'demo'
) ON CONFLICT DO NOTHING;

-- ─────────────────────────────────────────────────────────────────────────────
-- 6e. STEPS (data_health_steps)
-- ─────────────────────────────────────────────────────────────────────────────
-- Step counts captured at intervals. Bike commute and run produce step signals.

INSERT INTO data_health_steps (
    id, step_count, occurred_at,
    source_stream_id, source_table, source_provider
) VALUES
-- Bike commute to work (07:15-07:45 CST = 13:15-13:45 UTC) — pedaling registers as steps
('steps_demo_01', 320, '2026-02-13T13:20:00Z', 'demo_steps_001', 'data_health_steps', 'demo'),
('steps_demo_02', 410, '2026-02-13T13:30:00Z', 'demo_steps_002', 'data_health_steps', 'demo'),
('steps_demo_03', 280, '2026-02-13T13:40:00Z', 'demo_steps_003', 'data_health_steps', 'demo'),

-- Walking around office + lunch (~400 steps/hour ambient)
('steps_demo_04', 420, '2026-02-13T15:00:00Z', 'demo_steps_004', 'data_health_steps', 'demo'),
('steps_demo_05', 380, '2026-02-13T16:00:00Z', 'demo_steps_005', 'data_health_steps', 'demo'),
('steps_demo_06', 850, '2026-02-13T17:30:00Z', 'demo_steps_006', 'data_health_steps', 'demo'),
('steps_demo_07', 620, '2026-02-13T18:30:00Z', 'demo_steps_007', 'data_health_steps', 'demo'),

-- Run at Mueller trails (16:45-17:30 CST = 22:45-23:30 UTC) — high cadence
('steps_demo_08', 890, '2026-02-13T22:50:00Z', 'demo_steps_008', 'data_health_steps', 'demo'),
('steps_demo_09', 920, '2026-02-13T23:00:00Z', 'demo_steps_009', 'data_health_steps', 'demo'),
('steps_demo_10', 940, '2026-02-13T23:10:00Z', 'demo_steps_010', 'data_health_steps', 'demo'),
('steps_demo_11', 910, '2026-02-13T23:20:00Z', 'demo_steps_011', 'data_health_steps', 'demo'),

-- Feb 14: Walk at Lady Bird Lake
('steps_demo_12', 1200, '2026-02-14T17:45:00Z', 'demo_steps_012', 'data_health_steps', 'demo'),
('steps_demo_13', 1350, '2026-02-14T18:15:00Z', 'demo_steps_013', 'data_health_steps', 'demo'),
('steps_demo_14', 980, '2026-02-14T18:45:00Z', 'demo_steps_014', 'data_health_steps', 'demo') ON CONFLICT DO NOTHING;

-- ─────────────────────────────────────────────────────────────────────────────
-- 6f. HEART RATE (data_health_heart_rate)
-- ─────────────────────────────────────────────────────────────────────────────
-- Elevated readings during the run (16:45-17:30 CST = 22:45-23:30 UTC)

INSERT INTO data_health_heart_rate (
    id, bpm, occurred_at,
    source_stream_id, source_table, source_provider
) VALUES
-- Resting before run
('hr_demo_01', 68, '2026-02-13T22:40:00Z', 'demo_hr_001', 'data_health_heart_rate', 'demo'),
-- Warm-up
('hr_demo_02', 95, '2026-02-13T22:47:00Z', 'demo_hr_002', 'data_health_heart_rate', 'demo'),
('hr_demo_03', 118, '2026-02-13T22:50:00Z', 'demo_hr_003', 'data_health_heart_rate', 'demo'),
-- Steady state
('hr_demo_04', 148, '2026-02-13T22:55:00Z', 'demo_hr_004', 'data_health_heart_rate', 'demo'),
('hr_demo_05', 155, '2026-02-13T23:00:00Z', 'demo_hr_005', 'data_health_heart_rate', 'demo'),
('hr_demo_06', 158, '2026-02-13T23:05:00Z', 'demo_hr_006', 'data_health_heart_rate', 'demo'),
('hr_demo_07', 162, '2026-02-13T23:10:00Z', 'demo_hr_007', 'data_health_heart_rate', 'demo'),
-- Peak effort
('hr_demo_08', 168, '2026-02-13T23:15:00Z', 'demo_hr_008', 'data_health_heart_rate', 'demo'),
('hr_demo_09', 165, '2026-02-13T23:20:00Z', 'demo_hr_009', 'data_health_heart_rate', 'demo'),
-- Cool down
('hr_demo_10', 142, '2026-02-13T23:25:00Z', 'demo_hr_010', 'data_health_heart_rate', 'demo'),
('hr_demo_11', 118, '2026-02-13T23:30:00Z', 'demo_hr_011', 'data_health_heart_rate', 'demo'),
('hr_demo_12', 92, '2026-02-13T23:35:00Z', 'demo_hr_012', 'data_health_heart_rate', 'demo') ON CONFLICT DO NOTHING;

-- ─────────────────────────────────────────────────────────────────────────────
-- 6g. WORKOUT (data_health_workout)
-- ─────────────────────────────────────────────────────────────────────────────

-- Run at Mueller trails (16:45-17:30 CST = 22:45-23:30 UTC)
INSERT INTO data_health_workout (
    id, workout_type, start_time, end_time,
    duration_minutes, calories_burned, distance_km,
    avg_heart_rate, max_heart_rate,
    source_stream_id, source_table, source_provider
) VALUES (
    'workout_demo_run', 'running', '2026-02-13T22:45:00Z', '2026-02-13T23:30:00Z',
    45, 380, 5.2,
    152, 168,
    'demo_workout_001', 'data_health_workout', 'demo'
) ON CONFLICT DO NOTHING;

-- ─────────────────────────────────────────────────────────────────────────────
-- 6h. TRANSCRIPTION (data_communication_transcription)
-- ─────────────────────────────────────────────────────────────────────────────

-- User research session recording (12:30-14:15 CST = 18:30-20:15 UTC)
INSERT INTO data_communication_transcription (
    id, text, language, duration_seconds,
    start_time, end_time,
    speaker_count, title, summary, confidence,
    tags,
    source_stream_id, source_table, source_provider
) VALUES (
    'txn_demo_research',
    'Moderator: Thanks for joining. Today we''re looking at the main navigation. Can you walk me through how you''d find your account settings?

Participant 1: Um, I''d probably look up here in the top right... I see my avatar. Let me click that. Oh okay, there''s a dropdown. Settings is there. That was pretty easy.

Moderator: Great. Now imagine you want to change your notification preferences. Where would you look?

Participant 1: I''d go back to that settings page... scrolling down... I don''t see notifications. Maybe under account? No... Oh wait, is it under this "Preferences" tab? Yeah, there it is. That took a second.

Moderator: Interesting. What would have made that faster?

Participant 1: Honestly just having "Notifications" in the left sidebar of settings. I shouldn''t have to guess which tab it''s under.

Participant 2: I actually looked for a bell icon first. Like in the top nav bar. Most apps have that.

Moderator: Good point. Let''s look at the main dashboard next. What''s the first thing that catches your eye?

Participant 3: The activity feed is front and center, which makes sense. But I''m not sure what these icons mean on the left. Are those navigation items? They don''t have labels.

Moderator: Would labels help?

Participant 3: Definitely. Or at least tooltips on hover. Right now I''d have to click each one to figure out what it does.',
    'en', 6300,
    '2026-02-13T18:30:00Z', '2026-02-13T20:15:00Z',
    4, 'Navigation Redesign — Usability Test Round 3',
    'Three participants tested the navigation redesign. Key findings: settings discoverability is poor (notifications buried under Preferences tab), users expect a bell icon for notification access, icon-only nav items need labels or tooltips. All participants found the main avatar dropdown intuitive.',
    0.94,
    '["ux-research", "navigation", "usability-testing"]',
    'demo_txn_001', 'data_communication_transcription', 'demo'
) ON CONFLICT DO NOTHING;

-- Design standup recording (08:15-09:00 CST = 14:15-15:00 UTC)
INSERT INTO data_communication_transcription (
    id, text, language, duration_seconds,
    start_time, end_time,
    speaker_count, title, summary, confidence,
    tags, entities, speaker_segments,
    source_stream_id, source_table, source_provider
) VALUES (
    'txn_demo_standup',
    'Maya: Alright, let''s get going. I want to spend most of this on the onboarding flow. The drop-off data from last week is... not great.

David: Yeah, I pulled the funnel numbers yesterday. Step 3 completion is at 34 percent. It was 51 percent before the redesign.

Maya: That''s worse than I thought. What changed in step 3?

User: I''m sharing my screen — here''s the funnel side by side. The old flow had three fields on step 3. We added two more plus the company size selector. I think that''s where we''re losing people.

David: The form validation is also more aggressive now. It flags errors inline before you even finish typing. I''ve seen users abandon forms over that.

Maya: Okay, that''s two hypotheses. Field count and validation timing. Can we test both?

User: We could run it through the research session this afternoon. I have three participants booked for the nav redesign test, but I can add a quick onboarding task at the end.

Maya: Do it. Even five minutes of signal would help. David, can you mock up a version with lazy validation? Just flag errors on blur instead of on keystroke.

David: Already on it. I''ll have a prototype in Figma by noon.

Maya: Perfect. Anything else before we wrap?

User: That''s it from me. I''ll add the onboarding task to the research session script.

Maya: Perfect. Let''s regroup tomorrow morning with the results.',
    'en', 2700,
    '2026-02-13T14:15:00Z', '2026-02-13T15:00:00Z',
    3, 'Design Team Standup — Feb 13',
    'Discussed onboarding funnel drop-off: step 3 completion fell from 51% to 34% after redesign. Two hypotheses — increased field count and aggressive inline validation. Plan to test both in afternoon research session. David to mock lazy validation prototype by noon.',
    0.92,
    '["standup", "design-team", "onboarding"]',
    '{"people": ["Maya Chen", "David Okafor"], "topics": ["onboarding funnel", "form validation", "step 3 drop-off"], "products": ["Figma"]}',
    '[{"speaker": "Maya Chen", "start": 0.0, "end": 15.2}, {"speaker": "David Okafor", "start": 15.2, "end": 28.5}, {"speaker": "Maya Chen", "start": 28.5, "end": 33.1}, {"speaker": "User", "start": 33.1, "end": 58.4}, {"speaker": "David Okafor", "start": 58.4, "end": 74.0}, {"speaker": "Maya Chen", "start": 74.0, "end": 88.3}, {"speaker": "User", "start": 88.3, "end": 112.0}, {"speaker": "Maya Chen", "start": 112.0, "end": 135.6}, {"speaker": "David Okafor", "start": 135.6, "end": 142.8}, {"speaker": "Maya Chen", "start": 142.8, "end": 150.1}, {"speaker": "User", "start": 150.1, "end": 168.0}, {"speaker": "Maya Chen", "start": 168.0, "end": 172.5}]',
    'demo_txn_003', 'data_communication_transcription', 'demo'
) ON CONFLICT DO NOTHING;

-- Lunch conversation with Maya at Ramen Tatsu-ya (11:30-12:30 CST = 17:30-18:30 UTC)
INSERT INTO data_communication_transcription (
    id, text, language, duration_seconds,
    start_time, end_time,
    speaker_count, title, summary, confidence,
    tags, audio_url, metadata,
    source_stream_id, source_table, source_provider
) VALUES (
    'txn_demo_lunch',
    'Maya: I keep going back and forth on the new hire. On paper, Elise is perfect — great portfolio, strong systems thinking. But in the interview she kept defaulting to "it depends" on every design tradeoff question.

User: I mean, it usually does depend though.

Maya: Sure, but I wanted to see her commit to a position and defend it. You can always caveat later. I need someone who''ll push back in design reviews, not just agree with whatever the loudest voice says.

User: Fair. Did you talk to her references?

Maya: One of them said she''s "a great collaborator" which... could mean anything. The other was more specific — said she redesigned their entire settings architecture and reduced support tickets by 40 percent. That''s real impact.

User: That''s a strong signal. Maybe the interview nerves masked the opinionated side. A lot of people are more assertive once they''re comfortable on a team.

Maya: Maybe. I have until Friday to decide. Anyway — how''s the nav redesign coming? You seemed in flow this morning.

User: Yeah, I think the icon-plus-label approach is the right call. I''m testing it this afternoon. Three participants, focused on settings discoverability and the main nav.

Maya: Good. The tooltips-only version felt like a cop-out to me.

User: Agreed. Oh — unrelated, but Rachel just texted me. That house in Bouldin Creek is back on market. The one I showed you on Zillow.

Maya: The one with the huge backyard? Go see it! Today?

User: She said 3 PM. I might duck out after the research session.

Maya: Do it. Life''s too short to miss a good house.',
    'en', 3600,
    '2026-02-13T17:30:00Z', '2026-02-13T18:30:00Z',
    2, 'Lunch with Maya — Ramen Tatsu-ya',
    'Casual lunch conversation. Maya weighing new hire decision — strong portfolio but noncommittal in interview. Discussed nav redesign progress and upcoming research session. Mentioned Bouldin Creek house coming back on market.',
    0.79,
    '["personal", "work", "lunch"]',
    'https://demo.virtues.app/audio/txn_demo_lunch.m4a',
    '{"ambient_noise_level": "high", "recording_device": "iPhone 15 Pro", "environment": "restaurant"}',
    'demo_txn_004', 'data_communication_transcription', 'demo'
) ON CONFLICT DO NOTHING;

-- Voice memo to Maya from the car after Trader Joe's (5:11 PM CST = 23:11 UTC)
INSERT INTO data_communication_transcription (
    id, text, language, duration_seconds,
    start_time, end_time,
    speaker_count, title, summary, confidence,
    tags, entities,
    source_stream_id, source_table, source_provider
) VALUES (
    'txn_demo_voice_memo',
    'Hey Maya — quick one. So I told myself I was running in for cilantro. I just bought enough snacks to survive a winter. There is a frozen mandarin chicken thing here that I have heard about for years and now I own four of them. Anyway, I''ll see you Monday.',
    'en', 22,
    '2026-02-13T23:11:00Z', '2026-02-13T23:11:22Z',
    1, 'Voice memo to Maya — Trader Joe''s haul',
    'Short voice memo sent to Maya from the car after the Seaholm Trader Joe''s detour. Self-deprecating note about over-shopping; mentions the frozen mandarin chicken specifically.',
    0.97,
    '["personal", "voice-memo", "groceries"]',
    '{"places": ["Trader Joe''s — Seaholm"], "people": ["Maya Chen"], "items": ["mandarin chicken", "cilantro"]}',
    'demo_txn_005', 'data_communication_transcription', 'demo'
) ON CONFLICT DO NOTHING;

-- Feb 14: Phone call with Mom (for adjacent day)
INSERT INTO data_communication_transcription (
    id, text, language, duration_seconds,
    start_time, end_time,
    speaker_count, title, summary, confidence,
    source_stream_id, source_table, source_provider
) VALUES (
    'txn_demo_mom',
    'Summary: Caught up with Mom about the week. Mentioned looking at a house in Bouldin Creek. She asked about the neighborhood and whether it was safe. Talked about Valentine''s Day plans — nothing special, just a quiet Saturday. She mentioned Dad''s knee surgery is scheduled for March.',
    'en', 3600,
    '2026-02-14T19:30:00Z', '2026-02-14T20:30:00Z',
    2, 'Phone call with Mom', 'Weekly catch-up. Discussed house hunting, Valentine''s plans, Dad''s upcoming knee surgery in March.',
    0.88,
    'demo_txn_002', 'data_communication_transcription', 'demo'
) ON CONFLICT DO NOTHING;

-- ─────────────────────────────────────────────────────────────────────────────
-- 6h2. FINANCIAL (account + the Trader Joe's transaction)
-- ─────────────────────────────────────────────────────────────────────────────
-- One demo checking account; one anomalous grocery transaction at $328.50.
-- Amounts stored in cents per schema.

INSERT INTO data_financial_account (
    id, account_name, account_type, institution_name, mask,
    currency, current_balance, available_balance, is_active,
    source_stream_id, source_table, source_provider
) VALUES (
    'fin_acct_demo_checking', 'Everyday Checking', 'checking', 'Demo Bank', '4421',
    'USD', 712340, 698420, TRUE,
    'demo_fin_acct_001', 'data_financial_account', 'demo'
) ON CONFLICT DO NOTHING;

-- The Trader Joe's anomaly: $328.50 grocery transaction at 5:04 PM CST = 23:04 UTC
INSERT INTO data_financial_transaction (
    id, account_id, transaction_id,
    amount, currency, merchant_name, merchant_category,
    description, category, is_pending, transaction_type, payment_channel,
    occurred_at, authorized_timestamp,
    source_stream_id, source_table, source_provider, metadata
) VALUES (
    'fin_txn_demo_tj', 'fin_acct_demo_checking', 'demo_plaid_txn_001',
    32850, 'USD', 'Trader Joe''s', 'GROCERY',
    'TRADER JOE''S #142 SEAHOLM AUSTIN TX', '["food_and_drink", "groceries"]', FALSE, 'debit', 'in_store',
    '2026-02-13T23:04:00Z', '2026-02-13T23:04:00Z',
    'demo_fin_txn_001', 'data_financial_transaction', 'demo',
    '{"location": {"city": "Austin", "region": "TX", "address": "1308 W 5th St"}}'
) ON CONFLICT DO NOTHING;

-- ─────────────────────────────────────────────────────────────────────────────
-- 6i. LOCATION POINTS (data_location_point)
-- ─────────────────────────────────────────────────────────────────────────────
-- GPS breadcrumbs during transit and the run. These feed location clustering.

INSERT INTO data_location_point (
    id, latitude, longitude, horizontal_accuracy, occurred_at,
    source_stream_id, source_table, source_provider
) VALUES
-- Bike commute: Mueller → Downtown (07:15-07:45 CST = 13:15-13:45 UTC)
('lp_demo_01', 30.2989, -97.7055, 5.0, '2026-02-13T13:15:00Z', 'demo_lp_001', 'data_location_point', 'demo'),
('lp_demo_02', 30.2920, -97.7120, 8.0, '2026-02-13T13:20:00Z', 'demo_lp_002', 'data_location_point', 'demo'),
('lp_demo_03', 30.2850, -97.7200, 6.0, '2026-02-13T13:25:00Z', 'demo_lp_003', 'data_location_point', 'demo'),
('lp_demo_04', 30.2780, -97.7280, 5.0, '2026-02-13T13:30:00Z', 'demo_lp_004', 'data_location_point', 'demo'),
('lp_demo_05', 30.2720, -97.7350, 7.0, '2026-02-13T13:35:00Z', 'demo_lp_005', 'data_location_point', 'demo'),
('lp_demo_06', 30.2672, -97.7431, 5.0, '2026-02-13T13:45:00Z', 'demo_lp_006', 'data_location_point', 'demo'),

-- Drive: Office → Trader Joe's Seaholm (16:00-16:12 CST = 22:00-22:12 UTC)
('lp_demo_07', 30.2672, -97.7431, 10.0, '2026-02-13T22:00:00Z', 'demo_lp_007', 'data_location_point', 'demo'),
('lp_demo_08', 30.2685, -97.7470, 12.0, '2026-02-13T22:05:00Z', 'demo_lp_008', 'data_location_point', 'demo'),
('lp_demo_09', 30.2692, -97.7500, 8.0, '2026-02-13T22:09:00Z', 'demo_lp_009', 'data_location_point', 'demo'),
('lp_demo_10', 30.2696, -97.7521, 5.0, '2026-02-13T22:12:00Z', 'demo_lp_010', 'data_location_point', 'demo'),

-- Run: Mueller trails loop (16:45-17:30 CST = 22:45-23:30 UTC)
('lp_demo_11', 30.2989, -97.7055, 4.0, '2026-02-13T22:45:00Z', 'demo_lp_011', 'data_location_point', 'demo'),
('lp_demo_12', 30.3010, -97.7030, 5.0, '2026-02-13T22:50:00Z', 'demo_lp_012', 'data_location_point', 'demo'),
('lp_demo_13', 30.3040, -97.7010, 4.0, '2026-02-13T22:55:00Z', 'demo_lp_013', 'data_location_point', 'demo'),
('lp_demo_14', 30.3060, -97.6990, 5.0, '2026-02-13T23:00:00Z', 'demo_lp_014', 'data_location_point', 'demo'),
('lp_demo_15', 30.3050, -97.7020, 4.0, '2026-02-13T23:05:00Z', 'demo_lp_015', 'data_location_point', 'demo'),
('lp_demo_16', 30.3030, -97.7040, 5.0, '2026-02-13T23:10:00Z', 'demo_lp_016', 'data_location_point', 'demo'),
('lp_demo_17', 30.3010, -97.7050, 4.0, '2026-02-13T23:15:00Z', 'demo_lp_017', 'data_location_point', 'demo'),
('lp_demo_18', 30.2995, -97.7055, 5.0, '2026-02-13T23:20:00Z', 'demo_lp_018', 'data_location_point', 'demo'),
('lp_demo_19', 30.2989, -97.7055, 4.0, '2026-02-13T23:28:00Z', 'demo_lp_019', 'data_location_point', 'demo'),

-- Drive: Trader Joe's Seaholm → Home, Mueller (17:04-17:34 CST = 23:04-23:34 UTC)
('lp_demo_j2h_01', 30.2696, -97.7521, 8.0, '2026-02-13T23:04:00Z', 'demo_lp_j2h_001', 'data_location_point', 'demo'),
('lp_demo_j2h_02', 30.2820, -97.7290, 10.0, '2026-02-13T23:18:00Z', 'demo_lp_j2h_002', 'data_location_point', 'demo'),
('lp_demo_j2h_03', 30.2989, -97.7055, 5.0, '2026-02-13T23:34:00Z', 'demo_lp_j2h_003', 'data_location_point', 'demo') ON CONFLICT DO NOTHING;

-- (Evening GPS breadcrumbs omitted — partial day)

-- =============================================================================
-- 7. WIKI ENTITIES (people, places, organizations)
-- =============================================================================

-- ─────────────────────────────────────────────────────────────────────────────
-- 7a. PEOPLE (wiki_people)
-- ─────────────────────────────────────────────────────────────────────────────

-- Maya Chen — design team lead, close colleague
INSERT INTO wiki_people (
    id, canonical_name, emails, phones,
    relationship_category, notes,
    first_interaction, last_interaction, interaction_count
) VALUES (
    'person_demo_maya', 'Maya Chen',
    '["maya.chen@company.com"]', '[]',
    'colleague',
    'Design team lead. Sharp eye for UX patterns, always pushing for better onboarding flows. Lunch buddy — we hit Tatsu-ya at least once a week.',
    '2025-06-15', '2026-02-13', 215
) ON CONFLICT DO NOTHING;

-- David Okafor — design team, frontend-leaning
INSERT INTO wiki_people (
    id, canonical_name, emails, phones,
    relationship_category, notes,
    first_interaction, last_interaction, interaction_count
) VALUES (
    'person_demo_david', 'David Okafor',
    '["david.okafor@company.com"]', '[]',
    'colleague',
    'Design engineer on the team. Great at bridging design and code. Always first to flag form validation issues.',
    '2025-06-15', '2026-02-13', 180
) ON CONFLICT DO NOTHING;

-- Rachel Torres — realtor
INSERT INTO wiki_people (
    id, canonical_name, emails, phones,
    relationship_category, notes,
    first_interaction, last_interaction, interaction_count
) VALUES (
    'person_demo_rachel', 'Rachel Torres',
    '["rachel.torres@realty.com"]', '["512-555-0147"]',
    'professional',
    'Realtor helping with the house search. Found the Bouldin Creek place on S 3rd. Responsive and knows the Austin market well.',
    '2026-01-08', '2026-02-13', 24
) ON CONFLICT DO NOTHING;

-- Jess Landry — close friend
INSERT INTO wiki_people (
    id, canonical_name, emails, phones,
    relationship_category, nickname, notes,
    first_interaction, last_interaction, interaction_count
) VALUES (
    'person_demo_jess', 'Jess Landry',
    '["jess.landry@email.com"]', '["512-555-0233"]',
    'friend', 'Jess',
    'One of my closest friends in Austin. Lives on South Lamar. Always down for game night — her Catan strategy is ruthless.',
    '2024-03-20', '2026-02-13', 340
) ON CONFLICT DO NOTHING;

-- Priya Mehta — close friend
INSERT INTO wiki_people (
    id, canonical_name, emails, phones,
    relationship_category, notes,
    first_interaction, last_interaction, interaction_count
) VALUES (
    'person_demo_priya', 'Priya Mehta',
    '["priya.mehta@email.com"]', '["512-555-0891"]',
    'friend',
    'Part of the game night crew with Jess. Works in data science at a climate tech startup. Always brings good wine.',
    '2024-09-10', '2026-02-13', 145
) ON CONFLICT DO NOTHING;

-- Mom
INSERT INTO wiki_people (
    id, canonical_name, phones,
    relationship_category, nickname, notes,
    first_interaction, last_interaction, interaction_count
) VALUES (
    'person_demo_mom', 'Linda',
    '["512-555-0012"]',
    'family', 'Mom',
    'Weekly calls, usually Friday evenings. Dad''s knee surgery coming up in March.',
    '1990-01-01', '2026-02-14', 9999
) ON CONFLICT DO NOTHING;

-- ─────────────────────────────────────────────────────────────────────────────
-- 7b. PLACES (wiki_places)
-- ─────────────────────────────────────────────────────────────────────────────

-- Home — Mueller, East Austin
INSERT INTO wiki_places (
    id, name, category, address,
    latitude, longitude, radius_m,
    visit_count, first_visit, last_visit
) VALUES (
    'place_demo_home', 'Home', 'home',
    'Mueller, Austin, TX',
    30.2989, -97.7055, 50.0,
    365, '2025-02-01', '2026-02-14'
) ON CONFLICT DO NOTHING;

-- Office — Downtown Austin
INSERT INTO wiki_places (
    id, name, category, address,
    latitude, longitude, radius_m,
    visit_count, first_visit, last_visit
) VALUES (
    'place_demo_office', 'Office', 'workplace',
    'Downtown Austin, TX',
    30.2672, -97.7431, 80.0,
    220, '2025-06-15', '2026-02-14'
) ON CONFLICT DO NOTHING;

-- Ramen Tatsu-ya
INSERT INTO wiki_places (
    id, name, category, address,
    latitude, longitude, radius_m,
    visit_count, first_visit, last_visit,
    content
) VALUES (
    'place_demo_ramen', 'Ramen Tatsu-ya', 'restaurant',
    '8557 Research Blvd, Austin, TX 78758',
    30.2700, -97.7400, 40.0,
    18, '2025-07-02', '2026-02-13',
    'Go-to lunch spot with Maya. The original Tatsu-ya miso is unbeatable.'
) ON CONFLICT DO NOTHING;

-- Jo's Coffee — South Congress
INSERT INTO wiki_places (
    id, name, category, address,
    latitude, longitude, radius_m,
    visit_count, first_visit, last_visit,
    content
) VALUES (
    'place_demo_jos', 'Jo''s Coffee', 'cafe',
    '1300 S Congress Ave, Austin, TX 78704',
    30.2510, -97.7490, 30.0,
    12, '2025-04-18', '2026-02-13',
    'South Congress classic. Good people-watching spot.'
) ON CONFLICT DO NOTHING;

-- 1847 S 3rd St — Bouldin Creek house showing
INSERT INTO wiki_places (
    id, name, category, address,
    latitude, longitude, radius_m,
    visit_count, first_visit, last_visit,
    content
) VALUES (
    'place_demo_house', '1847 S 3rd St', 'residential',
    '1847 S 3rd St, Austin, TX 78704',
    30.2480, -97.7580, 30.0,
    1, '2026-02-13', '2026-02-13',
    'Bouldin Creek bungalow. Original tile in the kitchen, big backyard. Back on market Feb 13. Rachel showed it — sunlight was perfect in the afternoon.'
) ON CONFLICT DO NOTHING;

-- Jess's Place — South Lamar
INSERT INTO wiki_places (
    id, name, category, address,
    latitude, longitude, radius_m,
    visit_count, first_visit, last_visit
) VALUES (
    'place_demo_jess', 'Jess''s Place', 'residential',
    'South Lamar, Austin, TX',
    30.2520, -97.7545, 40.0,
    28, '2024-04-10', '2026-02-13'
) ON CONFLICT DO NOTHING;

-- Lady Bird Lake
INSERT INTO wiki_places (
    id, name, category, address,
    latitude, longitude, radius_m,
    visit_count, first_visit, last_visit
) VALUES (
    'place_demo_ladybird', 'Lady Bird Lake', 'park',
    'Lady Bird Lake, Austin, TX',
    30.2615, -97.7480, 500.0,
    35, '2024-06-01', '2026-02-14'
) ON CONFLICT DO NOTHING;

-- Mueller trails
INSERT INTO wiki_places (
    id, name, category, address,
    latitude, longitude, radius_m,
    visit_count, first_visit, last_visit,
    content
) VALUES (
    'place_demo_mueller_trails', 'Mueller Trails', 'park',
    'Mueller, Austin, TX',
    30.3030, -97.7020, 300.0,
    48, '2025-02-15', '2026-02-13',
    'Regular running route. ~5K loop from home. Good mix of paved and gravel.'
) ON CONFLICT DO NOTHING;

-- ─────────────────────────────────────────────────────────────────────────────
-- 7c. ORGANIZATIONS (wiki_orgs)
-- ─────────────────────────────────────────────────────────────────────────────

-- Employer — product design company
INSERT INTO wiki_orgs (
    id, canonical_name, organization_type,
    relationship_type, role_title,
    start_date, interaction_count,
    first_interaction, last_interaction,
    content
) VALUES (
    'org_demo_employer', 'Canopy', 'company',
    'employee', 'Senior UX Designer',
    '2025-06-15', 220,
    '2025-06-15', '2026-02-14',
    'B2B SaaS product. Small design team — Maya (lead), David, and me. Currently deep in a navigation redesign.'
) ON CONFLICT DO NOTHING;

-- Torres Realty — Rachel's agency
INSERT INTO wiki_orgs (
    id, canonical_name, organization_type,
    relationship_type,
    interaction_count, first_interaction, last_interaction
) VALUES (
    'org_demo_realty', 'Torres Realty', 'company',
    'client',
    24, '2026-01-08', '2026-02-13'
) ON CONFLICT DO NOTHING;

-- ─────────────────────────────────────────────────────────────────────────────
-- 11. ENTITY REFERENCES (junction table linking entities to ontology records)
-- ─────────────────────────────────────────────────────────────────────────────

-- Location visits → places
INSERT INTO wiki_refs (id, entity_type, entity_id, source_table, source_id, role, occurred_at) VALUES
('eref_lv_home_morning', 'place', 'place_demo_home', 'data_location_visit', 'lv_demo_home_morning', 'location', '2026-02-13T04:00:00Z'),
('eref_lv_office_am', 'place', 'place_demo_office', 'data_location_visit', 'lv_demo_office_am', 'location', '2026-02-13T13:45:00Z'),
('eref_lv_office_pm', 'place', 'place_demo_office', 'data_location_visit', 'lv_demo_office_pm', 'location', '2026-02-13T18:30:00Z'),
('eref_lv_ramen', 'place', 'place_demo_ramen', 'data_location_visit', 'lv_demo_ramen', 'location', '2026-02-13T17:30:00Z'),
('eref_lv_house', 'place', 'place_demo_house', 'data_location_visit', 'lv_demo_house', 'location', '2026-02-13T21:00:00Z'),
('eref_lv_jos', 'place', 'place_demo_jos', 'data_location_visit', 'lv_demo_jos', 'location', '2026-02-13T21:45:00Z'),
('eref_lv_mueller', 'place', 'place_demo_mueller_trails', 'data_location_visit', 'lv_demo_mueller', 'location', '2026-02-13T22:45:00Z'),
('eref_lv_home_eve', 'place', 'place_demo_home', 'data_location_visit', 'lv_demo_home_evening', 'location', '2026-02-13T23:30:00Z'),

-- Calendar events → attendees (people)
('eref_cal_standup_maya', 'person', 'person_demo_maya', 'data_calendar_event', 'cal_demo_standup', 'attendee', '2026-02-13T14:15:00Z'),
('eref_cal_standup_david', 'person', 'person_demo_david', 'data_calendar_event', 'cal_demo_standup', 'attendee', '2026-02-13T14:15:00Z'),
('eref_cal_lunch_maya', 'person', 'person_demo_maya', 'data_calendar_event', 'cal_demo_lunch', 'attendee', '2026-02-13T17:30:00Z'),
('eref_cal_research_place', 'place', 'place_demo_office', 'data_calendar_event', 'cal_demo_research', 'location', '2026-02-13T18:30:00Z'),
('eref_cal_house_rachel', 'person', 'person_demo_rachel', 'data_calendar_event', 'cal_demo_house', 'attendee', '2026-02-13T21:00:00Z'),

-- Messages → senders (mid-event timestamps for temporal precision in chart)
('eref_msg1_maya', 'person', 'person_demo_maya', 'data_communication_message', 'msg_demo_01', 'sender', '2026-02-13T13:50:00Z'),
('eref_msg3_david', 'person', 'person_demo_david', 'data_communication_message', 'msg_demo_03', 'sender', '2026-02-13T14:05:00Z'),
('eref_msg4_rachel', 'person', 'person_demo_rachel', 'data_communication_message', 'msg_demo_04', 'sender', '2026-02-13T20:20:00Z'),
('eref_msg6_rachel', 'person', 'person_demo_rachel', 'data_communication_message', 'msg_demo_06', 'sender', '2026-02-13T20:25:00Z'),

-- Transcriptions → speakers (standup, lunch, research had people speaking mid-event)
('eref_txn_standup_maya', 'person', 'person_demo_maya', 'data_communication_transcription', 'txn_demo_standup', 'speaker', '2026-02-13T14:20:00Z'),
('eref_txn_standup_david', 'person', 'person_demo_david', 'data_communication_transcription', 'txn_demo_standup', 'speaker', '2026-02-13T14:35:00Z'),
('eref_txn_lunch_maya', 'person', 'person_demo_maya', 'data_communication_transcription', 'txn_demo_lunch', 'speaker', '2026-02-13T17:45:00Z') ON CONFLICT DO NOTHING;
