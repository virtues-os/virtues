-- =============================================================================
-- Demo Day Seed: Thursday, February 13, 2025 — Austin, TX
-- =============================================================================
--
-- Character: UX designer, lives in Mueller (East Austin), works downtown.
-- Narrative: Routine morning → work → house showing pivot → run → friends night.
--
-- This seed populates:
--   1. data_location_visit       (10 visits — office split AM/PM, home return, Lady Bird Lake)
--   2. wiki_days                 (3 — primary day + 2 adjacent for cross-day scoring)
--   3. wiki_events               (27 — 16 for Feb 13, 6 for Feb 12, 5 for Feb 14)
--   4. data_health_sleep         (3 sleep records)
--   5. data_calendar_event       (5 calendar events)
--   6. data_communication_message (9 Slack + text messages)
--   7. data_activity_app_usage   (9 app sessions)
--   8. data_health_steps         (14 step readings)
--   9. data_health_heart_rate    (12 HR readings during run)
--  10. data_health_workout       (1 run)
--  11. data_communication_transcription (5 recordings)
--  12. data_location_point       (25 GPS breadcrumbs)
--
-- All times are UTC. Austin = America/Chicago = UTC-6 in February.
-- So 06:30 CST = 12:30 UTC, midnight CST = 06:00 UTC.
--
-- Usage: sqlite3 core/data/virtues.db < core/seed_demo_day.sql
-- =============================================================================

-- ─────────────────────────────────────────────────────────────────────────────
-- 1. LOCATION VISITS
-- ─────────────────────────────────────────────────────────────────────────────
-- Feb 13 visits (times in UTC; CST = UTC-6)

-- Home (Mueller) — overnight + morning
INSERT OR IGNORE INTO data_location_visit (
    id, place_name, latitude, longitude,
    arrival_time, departure_time, duration_minutes,
    source_stream_id, source_table, source_provider
) VALUES (
    'lv_demo_home_morning', 'Home', 30.2989, -97.7055,
    '2025-02-13T04:00:00Z', '2025-02-13T13:15:00Z', 555,
    'demo_lv_001', 'data_location_visit', 'demo'
);

-- Office (Downtown Austin) — morning session, before lunch
INSERT OR IGNORE INTO data_location_visit (
    id, place_name, latitude, longitude,
    arrival_time, departure_time, duration_minutes,
    source_stream_id, source_table, source_provider
) VALUES (
    'lv_demo_office_am', 'Office', 30.2672, -97.7431,
    '2025-02-13T13:45:00Z', '2025-02-13T17:25:00Z', 220,
    'demo_lv_002a', 'data_location_visit', 'demo'
);

-- Office (Downtown Austin) — afternoon session, after lunch
INSERT OR IGNORE INTO data_location_visit (
    id, place_name, latitude, longitude,
    arrival_time, departure_time, duration_minutes,
    source_stream_id, source_table, source_provider
) VALUES (
    'lv_demo_office_pm', 'Office', 30.2672, -97.7431,
    '2025-02-13T18:35:00Z', '2025-02-13T20:28:00Z', 113,
    'demo_lv_002b', 'data_location_visit', 'demo'
);

-- Ramen Tatsu-ya (lunch)
INSERT OR IGNORE INTO data_location_visit (
    id, place_name, latitude, longitude,
    arrival_time, departure_time, duration_minutes,
    source_stream_id, source_table, source_provider
) VALUES (
    'lv_demo_ramen', 'Ramen Tatsu-ya', 30.2700, -97.7400,
    '2025-02-13T17:30:00Z', '2025-02-13T18:30:00Z', 60,
    'demo_lv_003', 'data_location_visit', 'demo'
);

-- Bouldin Creek house (showing)
INSERT OR IGNORE INTO data_location_visit (
    id, place_name, latitude, longitude,
    arrival_time, departure_time, duration_minutes,
    source_stream_id, source_table, source_provider
) VALUES (
    'lv_demo_house', '1847 S 3rd St', 30.2480, -97.7580,
    '2025-02-13T21:00:00Z', '2025-02-13T21:45:00Z', 45,
    'demo_lv_004', 'data_location_visit', 'demo'
);

-- Jo's Coffee (South Congress, post-showing)
INSERT OR IGNORE INTO data_location_visit (
    id, place_name, latitude, longitude,
    arrival_time, departure_time, duration_minutes,
    source_stream_id, source_table, source_provider
) VALUES (
    'lv_demo_coffee', 'Jo''s Coffee', 30.2510, -97.7490,
    '2025-02-13T21:50:00Z', '2025-02-13T22:15:00Z', 25,
    'demo_lv_005', 'data_location_visit', 'demo'
);

-- Home (Mueller) — quick run + shower
INSERT OR IGNORE INTO data_location_visit (
    id, place_name, latitude, longitude,
    arrival_time, departure_time, duration_minutes,
    source_stream_id, source_table, source_provider
) VALUES (
    'lv_demo_home_run', 'Home', 30.2989, -97.7055,
    '2025-02-13T22:45:00Z', '2025-02-14T00:15:00Z', 90,
    'demo_lv_006', 'data_location_visit', 'demo'
);

-- Jess's place (South Lamar — friends night)
INSERT OR IGNORE INTO data_location_visit (
    id, place_name, latitude, longitude,
    arrival_time, departure_time, duration_minutes,
    source_stream_id, source_table, source_provider
) VALUES (
    'lv_demo_jess', 'Jess''s Place', 30.2520, -97.7545,
    '2025-02-14T00:30:00Z', '2025-02-14T04:00:00Z', 210,
    'demo_lv_007', 'data_location_visit', 'demo'
);

-- Home (Mueller) — late-night return from Jess's
INSERT OR IGNORE INTO data_location_visit (
    id, place_name, latitude, longitude,
    arrival_time, departure_time, duration_minutes,
    source_stream_id, source_table, source_provider
) VALUES (
    'lv_demo_home_night', 'Home', 30.2989, -97.7055,
    '2025-02-14T04:15:00Z', '2025-02-14T12:30:00Z', 495,
    'demo_lv_008', 'data_location_visit', 'demo'
);

-- Feb 14: Lady Bird Lake walk
INSERT OR IGNORE INTO data_location_visit (
    id, place_name, latitude, longitude,
    arrival_time, departure_time, duration_minutes,
    source_stream_id, source_table, source_provider
) VALUES (
    'lv_demo_ladybird', 'Lady Bird Lake', 30.2615, -97.7480,
    '2025-02-14T17:30:00Z', '2025-02-14T19:00:00Z', 90,
    'demo_lv_009', 'data_location_visit', 'demo'
);

-- ─────────────────────────────────────────────────────────────────────────────
-- 2. WIKI DAYS
-- ─────────────────────────────────────────────────────────────────────────────

-- Primary day: Feb 13 (the detailed one)
INSERT OR IGNORE INTO wiki_days (
    id, date, start_timezone, end_timezone,
    autobiography, context_vector,
    chaos_score, entropy_calibration_days
) VALUES (
    'day_2025-02-13', '2025-02-13', 'America/Chicago', 'America/Chicago',
    'The morning unfolded in its usual rhythm — coffee at the kitchen counter, the bike ride down Manor Road into downtown, Slack messages piling up before standup. The design review with Maya and David surfaced a question about the onboarding flow that would linger through the afternoon.

Lunch at Tatsu-ya was unhurried, Maya talking through her doubts about the new hire. The user research session after felt productive — three participants, good signal on the navigation redesign.

Then Rachel texted. The house on South 3rd was back on market. By 3 PM she was standing in its kitchen, sunlight hitting the original tile, the backyard bigger than the listing photos suggested. She walked the neighborhood after, got coffee at Jo''s, watched people on South Congress and thought about what it would mean to live here.

A quick run along the Mueller trails cleared her head. By 7 she was at Jess''s, Priya already pouring wine, Settlers of Catan half set up on the dining table. They played until 10, talked about nothing important, and she drove home with the windows down.',
    '{"who": 0.72, "whom": 0.85, "what": 0.91, "when": 0.78, "where": 0.95, "why": 0.65, "how": 0.58}',
    0.42, 5
);

-- Adjacent day: Feb 12 (routine Wednesday — for cross-day comparison)
INSERT OR IGNORE INTO wiki_days (
    id, date, start_timezone, end_timezone,
    autobiography, context_vector,
    chaos_score, entropy_calibration_days
) VALUES (
    'day_2025-02-12', '2025-02-12', 'America/Chicago', 'America/Chicago',
    'A quiet Wednesday. Work from home in the morning, office in the afternoon. Design iteration on the settings page. Leftovers for dinner, read before bed.',
    '{"who": 0.55, "whom": 0.60, "what": 0.70, "when": 0.65, "where": 0.50, "why": 0.40, "how": 0.45}',
    0.18, 4
);

-- Adjacent day: Feb 14 (Valentine's Friday — slightly different texture)
INSERT OR IGNORE INTO wiki_days (
    id, date, start_timezone, end_timezone,
    autobiography, context_vector,
    chaos_score, entropy_calibration_days
) VALUES (
    'day_2025-02-14', '2025-02-14', 'America/Chicago', 'America/Chicago',
    'Friday. Short day at the office — sprint demo in the morning, then an early afternoon. Walked Lady Bird Lake. Called Mom. Made pasta and watched a movie.',
    '{"who": 0.65, "whom": 0.50, "what": 0.55, "when": 0.60, "where": 0.70, "why": 0.45, "how": 0.40}',
    0.25, 5
);

-- ─────────────────────────────────────────────────────────────────────────────
-- 3. WIKI EVENTS — Feb 13 (14 events, midnight-to-midnight CST = 06:00-06:00 UTC)
-- ─────────────────────────────────────────────────────────────────────────────
-- W6H activation order: [who, whom, what, when, where, why, how]
-- Entropy: embedding novelty (0-1). First event uses Shannon fallback.
-- W6H Entropy: Shannon entropy of activation vector (0-1).

-- E01: Sleep (00:00-06:30 CST = 06:00-12:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit,
    w6h_activation, entropy, w6h_entropy
) VALUES (
    'ev_demo_01', 'day_2025-02-13',
    '2025-02-13T06:00:00Z', '2025-02-13T12:30:00Z',
    'Sleep', 'Home', '["sleep"]',
    0, 0,
    '[0.9, 0.0, 0.1, 0.5, 0.3, 0.0, 0.0]', 0.12, 0.12
);

-- E02: Morning routine (06:30-07:15 CST = 12:30-13:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit,
    w6h_activation, entropy, w6h_entropy
) VALUES (
    'ev_demo_02', 'day_2025-02-13',
    '2025-02-13T12:30:00Z', '2025-02-13T13:15:00Z',
    'Morning routine', 'Home', '["app_usage"]',
    0, 0,
    '[0.6, 0.0, 0.2, 0.3, 0.3, 0.0, 0.2]', 0.38, 0.28
);

-- E03: Bike commute (07:15-07:45 CST = 13:15-13:45 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_user_added, is_user_edited,
    w6h_activation, entropy, w6h_entropy
) VALUES (
    'ev_demo_03', 'day_2025-02-13',
    '2025-02-13T13:15:00Z', '2025-02-13T13:45:00Z',
    'Bike commute', NULL, '["location_visit", "steps"]',
    0, 1, 0, 0,
    '[0.4, 0.0, 0.1, 0.2, 0.9, 0.1, 0.7]', 0.55, 0.52
);

-- E04: Coffee and Slack (07:45-08:15 CST = 13:45-14:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit,
    w6h_activation, entropy, w6h_entropy
) VALUES (
    'ev_demo_04', 'day_2025-02-13',
    '2025-02-13T13:45:00Z', '2025-02-13T14:15:00Z',
    'Coffee and Slack', 'Office', '["app_usage", "message"]',
    0, 0,
    '[0.3, 0.5, 0.4, 0.3, 0.4, 0.2, 0.5]', 0.48, 0.72
);

-- E05: Design standup (08:15-09:00 CST = 14:15-15:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit,
    w6h_activation, entropy, w6h_entropy
) VALUES (
    'ev_demo_05', 'day_2025-02-13',
    '2025-02-13T14:15:00Z', '2025-02-13T15:00:00Z',
    'Design standup', 'Office', '["calendar", "message", "transcription"]',
    0, 0,
    '[0.2, 0.8, 0.7, 0.7, 0.3, 0.4, 0.3]', 0.35, 0.78
);

-- E06: Focused design work (09:00-11:30 CST = 15:00-17:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit,
    w6h_activation, entropy, w6h_entropy
) VALUES (
    'ev_demo_06', 'day_2025-02-13',
    '2025-02-13T15:00:00Z', '2025-02-13T17:30:00Z',
    'Focused design work', 'Office', '["app_usage"]',
    0, 0,
    '[0.3, 0.1, 0.9, 0.2, 0.3, 0.6, 0.8]', 0.22, 0.65
);

-- E07: Lunch with Maya (11:30-12:30 CST = 17:30-18:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit,
    w6h_activation, entropy, w6h_entropy
) VALUES (
    'ev_demo_07', 'day_2025-02-13',
    '2025-02-13T17:30:00Z', '2025-02-13T18:30:00Z',
    'Lunch with Maya', 'Ramen Tatsu-ya', '["location_visit", "calendar", "transcription"]',
    0, 0,
    '[0.3, 0.9, 0.5, 0.5, 0.7, 0.3, 0.2]', 0.52, 0.75
);

-- E08: User research session (12:30-14:15 CST = 18:30-20:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit,
    w6h_activation, entropy, w6h_entropy
) VALUES (
    'ev_demo_08', 'day_2025-02-13',
    '2025-02-13T18:30:00Z', '2025-02-13T20:15:00Z',
    'User research session', 'Office', '["calendar", "transcription"]',
    0, 0,
    '[0.2, 0.7, 0.9, 0.6, 0.3, 0.7, 0.4]', 0.31, 0.80
);

-- E09: Drive to house showing (14:30-15:00 CST = 20:30-21:00 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_user_added, is_user_edited,
    w6h_activation, entropy, w6h_entropy
) VALUES (
    'ev_demo_09', 'day_2025-02-13',
    '2025-02-13T20:30:00Z', '2025-02-13T21:00:00Z',
    'Drive to Bouldin Creek', NULL, '["location_visit"]',
    0, 1, 0, 0,
    '[0.2, 0.0, 0.1, 0.2, 0.8, 0.3, 0.5]', 0.61, 0.48
);

-- E10: House showing (15:00-15:45 CST = 21:00-21:45 UTC) *** ENTROPY SPIKE ***
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit,
    w6h_activation, entropy, w6h_entropy
) VALUES (
    'ev_demo_10', 'day_2025-02-13',
    '2025-02-13T21:00:00Z', '2025-02-13T21:45:00Z',
    'House showing', '1847 S 3rd St', '["location_visit", "message"]',
    0, 0,
    '[0.4, 0.6, 0.8, 0.5, 0.9, 0.9, 0.3]', 0.82, 0.88
);

-- E11: Coffee at Jo's (15:50-16:15 CST = 21:50-22:15 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit,
    w6h_activation, entropy, w6h_entropy
) VALUES (
    'ev_demo_11', 'day_2025-02-13',
    '2025-02-13T21:50:00Z', '2025-02-13T22:15:00Z',
    'Coffee at Jo''s', 'Jo''s Coffee', '["location_visit", "transcription"]',
    0, 0,
    '[0.5, 0.0, 0.3, 0.2, 0.7, 0.5, 0.1]', 0.58, 0.55
);

-- E11b: Drive from Jo's to Home (16:15-16:45 CST = 22:15-22:45 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_user_added, is_user_edited,
    w6h_activation, entropy, w6h_entropy
) VALUES (
    'ev_demo_11b', 'day_2025-02-13',
    '2025-02-13T22:15:00Z', '2025-02-13T22:45:00Z',
    'Drive home', NULL, '["location_visit"]',
    0, 1, 0, 0,
    '[0.3, 0.0, 0.1, 0.2, 0.7, 0.4, 0.5]', 0.52, 0.45
);

-- E12: Run at Mueller trails (16:45-17:30 CST = 22:45-23:30 UTC)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit,
    w6h_activation, entropy, w6h_entropy
) VALUES (
    'ev_demo_12', 'day_2025-02-13',
    '2025-02-13T22:45:00Z', '2025-02-13T23:30:00Z',
    'Run', 'Mueller trails', '["steps", "heart_rate", "workout"]',
    0, 0,
    '[0.9, 0.0, 0.3, 0.2, 0.6, 0.2, 0.8]', 0.65, 0.54
);

-- E13: Friends night at Jess's (18:30-22:00 CST = 00:30-04:00 UTC+1)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit,
    w6h_activation, entropy, w6h_entropy
) VALUES (
    'ev_demo_13', 'day_2025-02-13',
    '2025-02-14T00:30:00Z', '2025-02-14T04:00:00Z',
    'Game night at Jess''s', 'Jess''s Place', '["location_visit", "message"]',
    0, 0,
    '[0.4, 0.9, 0.6, 0.3, 0.6, 0.2, 0.2]', 0.71, 0.72
);

-- E13b: Drive home from Jess's (22:00-22:15 CST = 04:00-04:15 UTC+1)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_user_added, is_user_edited,
    w6h_activation, entropy, w6h_entropy
) VALUES (
    'ev_demo_13b', 'day_2025-02-13',
    '2025-02-14T04:00:00Z', '2025-02-14T04:15:00Z',
    'Drive home', NULL, '["location_visit"]',
    0, 1, 0, 0,
    '[0.3, 0.0, 0.1, 0.1, 0.6, 0.1, 0.4]', 0.55, 0.35
);

-- E14: Wind down at home (22:15-24:00 CST = 04:15-06:00 UTC+1)
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit,
    w6h_activation, entropy, w6h_entropy
) VALUES (
    'ev_demo_14', 'day_2025-02-13',
    '2025-02-14T04:15:00Z', '2025-02-14T06:00:00Z',
    'Wind down', 'Home', '["app_usage"]',
    0, 0,
    '[0.6, 0.0, 0.2, 0.1, 0.3, 0.0, 0.1]', 0.45, 0.18
);

-- ─────────────────────────────────────────────────────────────────────────────
-- 4. WIKI EVENTS — Feb 12 (simple routine day, 6 events)
-- ─────────────────────────────────────────────────────────────────────────────

INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit,
    w6h_activation, entropy, w6h_entropy
) VALUES
('ev_feb12_01', 'day_2025-02-12', '2025-02-12T06:00:00Z', '2025-02-12T13:00:00Z',
 'Sleep', 'Home', '["sleep"]', 0, 0,
 '[0.9, 0.0, 0.1, 0.5, 0.3, 0.0, 0.0]', 0.10, 0.12),

('ev_feb12_02', 'day_2025-02-12', '2025-02-12T13:00:00Z', '2025-02-12T14:00:00Z',
 'Morning routine', 'Home', '["app_usage"]', 0, 0,
 '[0.6, 0.0, 0.2, 0.3, 0.3, 0.0, 0.2]', 0.35, 0.28),

('ev_feb12_03', 'day_2025-02-12', '2025-02-12T14:00:00Z', '2025-02-12T18:00:00Z',
 'Work from home', 'Home', '["app_usage", "message"]', 0, 0,
 '[0.3, 0.4, 0.8, 0.5, 0.3, 0.5, 0.7]', 0.30, 0.78),

('ev_feb12_04', 'day_2025-02-12', '2025-02-12T18:00:00Z', '2025-02-12T23:00:00Z',
 'Office work', 'Office', '["app_usage", "calendar", "message"]', 0, 0,
 '[0.2, 0.6, 0.8, 0.6, 0.4, 0.5, 0.6]', 0.28, 0.82),

('ev_feb12_05', 'day_2025-02-12', '2025-02-12T23:00:00Z', '2025-02-13T01:00:00Z',
 'Dinner and reading', 'Home', '["app_usage"]', 0, 0,
 '[0.5, 0.0, 0.4, 0.1, 0.3, 0.2, 0.1]', 0.42, 0.40),

('ev_feb12_06', 'day_2025-02-12', '2025-02-13T01:00:00Z', '2025-02-13T06:00:00Z',
 'Sleep', 'Home', '["sleep"]', 0, 0,
 '[0.9, 0.0, 0.1, 0.5, 0.3, 0.0, 0.0]', 0.38, 0.12);

-- ─────────────────────────────────────────────────────────────────────────────
-- 5. WIKI EVENTS — Feb 14 (quiet Friday, 5 events)
-- ─────────────────────────────────────────────────────────────────────────────

INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit,
    w6h_activation, entropy, w6h_entropy
) VALUES
('ev_feb14_01', 'day_2025-02-14', '2025-02-14T06:00:00Z', '2025-02-14T12:30:00Z',
 'Sleep', 'Home', '["sleep"]', 0, 0,
 '[0.9, 0.0, 0.1, 0.5, 0.3, 0.0, 0.0]', 0.10, 0.12),

('ev_feb14_02', 'day_2025-02-14', '2025-02-14T13:00:00Z', '2025-02-14T17:00:00Z',
 'Sprint demo and office', 'Office', '["calendar", "message", "app_usage"]', 0, 0,
 '[0.2, 0.7, 0.8, 0.7, 0.4, 0.4, 0.5]', 0.40, 0.80),

('ev_feb14_03', 'day_2025-02-14', '2025-02-14T17:30:00Z', '2025-02-14T19:00:00Z',
 'Walk at Lady Bird Lake', 'Lady Bird Lake', '["steps", "location_visit"]', 0, 0,
 '[0.7, 0.0, 0.3, 0.2, 0.9, 0.3, 0.6]', 0.55, 0.55),

('ev_feb14_04', 'day_2025-02-14', '2025-02-14T19:30:00Z', '2025-02-14T20:30:00Z',
 'Phone call with Mom', 'Home', '["transcription"]', 0, 0,
 '[0.5, 0.8, 0.5, 0.3, 0.3, 0.4, 0.1]', 0.50, 0.68),

('ev_feb14_05', 'day_2025-02-14', '2025-02-14T21:00:00Z', '2025-02-15T06:00:00Z',
 'Dinner and movie', 'Home', '["app_usage"]', 0, 0,
 '[0.5, 0.0, 0.5, 0.1, 0.3, 0.2, 0.2]', 0.35, 0.35);

-- =============================================================================
-- 6. ONTOLOGY SOURCE DATA
-- =============================================================================
-- These are the raw data records that feed into day summary generation via
-- the Tollbooth pipeline. Events reference these via source_ontologies.

-- ─────────────────────────────────────────────────────────────────────────────
-- 6a. SLEEP (data_health_sleep)
-- ─────────────────────────────────────────────────────────────────────────────

-- Feb 12-13 overnight sleep
INSERT OR IGNORE INTO data_health_sleep (
    id, start_time, end_time, duration_minutes, sleep_quality_score,
    sleep_stages,
    source_stream_id, source_table, source_provider
) VALUES (
    'sleep_demo_feb12', '2025-02-12T06:00:00Z', '2025-02-12T13:00:00Z', 420, 0.82,
    '{"deep": 95, "light": 180, "rem": 90, "awake": 55}',
    'demo_sleep_001', 'data_health_sleep', 'demo'
);

-- Feb 13 overnight sleep (midnight-6:30am CST = 06:00-12:30 UTC)
INSERT OR IGNORE INTO data_health_sleep (
    id, start_time, end_time, duration_minutes, sleep_quality_score,
    sleep_stages,
    source_stream_id, source_table, source_provider
) VALUES (
    'sleep_demo_feb13', '2025-02-13T06:00:00Z', '2025-02-13T12:30:00Z', 390, 0.78,
    '{"deep": 80, "light": 170, "rem": 85, "awake": 55}',
    'demo_sleep_002', 'data_health_sleep', 'demo'
);

-- Feb 14 overnight sleep (late night after game night — shorter)
INSERT OR IGNORE INTO data_health_sleep (
    id, start_time, end_time, duration_minutes, sleep_quality_score,
    sleep_stages,
    source_stream_id, source_table, source_provider
) VALUES (
    'sleep_demo_feb14', '2025-02-14T06:00:00Z', '2025-02-14T12:30:00Z', 390, 0.75,
    '{"deep": 70, "light": 185, "rem": 75, "awake": 60}',
    'demo_sleep_003', 'data_health_sleep', 'demo'
);

-- ─────────────────────────────────────────────────────────────────────────────
-- 6b. CALENDAR EVENTS (data_calendar_event)
-- ─────────────────────────────────────────────────────────────────────────────

-- Feb 13: Design standup (08:15-09:00 CST = 14:15-15:00 UTC)
INSERT OR IGNORE INTO data_calendar_event (
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
    '2025-02-13T14:15:00Z', '2025-02-13T15:00:00Z', 'America/Chicago',
    'demo_cal_001', 'data_calendar_event', 'demo'
);

-- Feb 13: Lunch with Maya (11:30-12:30 CST = 17:30-18:30 UTC)
INSERT OR IGNORE INTO data_calendar_event (
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
    '2025-02-13T17:30:00Z', '2025-02-13T18:30:00Z', 'America/Chicago',
    'demo_cal_002', 'data_calendar_event', 'demo'
);

-- Feb 13: User Research Session (12:30-14:15 CST = 18:30-20:15 UTC)
INSERT OR IGNORE INTO data_calendar_event (
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
    '2025-02-13T18:30:00Z', '2025-02-13T20:15:00Z', 'America/Chicago',
    'demo_cal_003', 'data_calendar_event', 'demo'
);

-- Feb 12: Settings page review (for adjacent day)
INSERT OR IGNORE INTO data_calendar_event (
    id, title, description, calendar_name, event_type, status,
    organizer_identifier, attendee_identifiers,
    start_time, end_time, timezone,
    source_stream_id, source_table, source_provider
) VALUES (
    'cal_demo_feb12', 'Settings Page Review', 'Async design review of settings iteration',
    'Work', 'meeting', 'confirmed',
    'demo-user@company.com', '["david.okafor@company.com"]',
    '2025-02-12T19:00:00Z', '2025-02-12T20:00:00Z', 'America/Chicago',
    'demo_cal_004', 'data_calendar_event', 'demo'
);

-- Feb 14: Sprint Demo (for adjacent day)
INSERT OR IGNORE INTO data_calendar_event (
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
    '2025-02-14T14:00:00Z', '2025-02-14T15:00:00Z', 'America/Chicago',
    'demo_cal_005', 'data_calendar_event', 'demo'
);

-- ─────────────────────────────────────────────────────────────────────────────
-- 6c. MESSAGES (data_communication_message)
-- ─────────────────────────────────────────────────────────────────────────────

-- Slack: Maya re standup agenda (07:50 CST = 13:50 UTC)
INSERT OR IGNORE INTO data_communication_message (
    id, message_id, thread_id, channel, body,
    from_identifier, from_name, to_identifiers,
    is_read, is_group_message, timestamp,
    source_stream_id, source_table, source_provider
) VALUES (
    'msg_demo_01', 'slack_msg_001', 'thread_standup_feb13', '#design-team',
    'heads up — I want to talk about the onboarding flow today. got some concerns about the drop-off data',
    'maya.chen@company.com', 'Maya Chen', '["#design-team"]',
    1, 1, '2025-02-13T13:50:00Z',
    'demo_msg_001', 'data_communication_message', 'demo'
);

-- Slack: David reply (07:55 CST = 13:55 UTC)
INSERT OR IGNORE INTO data_communication_message (
    id, message_id, thread_id, channel, body,
    from_identifier, from_name, to_identifiers,
    is_read, is_group_message, timestamp,
    source_stream_id, source_table, source_provider
) VALUES (
    'msg_demo_02', 'slack_msg_002', 'thread_standup_feb13', '#design-team',
    'yeah the step 3 completion rate is brutal. maybe we should look at the form validation UX',
    'david.okafor@company.com', 'David Okafor', '["#design-team"]',
    1, 1, '2025-02-13T13:55:00Z',
    'demo_msg_002', 'data_communication_message', 'demo'
);

-- Slack: User reply (08:05 CST = 14:05 UTC)
INSERT OR IGNORE INTO data_communication_message (
    id, message_id, thread_id, channel, body,
    from_identifier, from_name, to_identifiers,
    is_read, is_group_message, timestamp,
    source_stream_id, source_table, source_provider
) VALUES (
    'msg_demo_03', 'slack_msg_003', 'thread_standup_feb13', '#design-team',
    'pulling up the funnel data now. will have it on screen for standup',
    'demo-user@company.com', NULL, '["#design-team"]',
    1, 1, '2025-02-13T14:05:00Z',
    'demo_msg_003', 'data_communication_message', 'demo'
);

-- Text: Rachel Torres about house (14:20 CST = 20:20 UTC)
INSERT OR IGNORE INTO data_communication_message (
    id, message_id, channel, body,
    from_identifier, from_name, to_identifiers,
    is_read, is_group_message, timestamp,
    source_stream_id, source_table, source_provider
) VALUES (
    'msg_demo_04', 'sms_msg_001', 'sms',
    'Hey! That Bouldin Creek house on S 3rd just came back on market. The one with the big backyard. Want to see it today? I can meet you at 3.',
    'rachel.torres@realty.com', 'Rachel Torres', '["demo-user@phone.com"]',
    1, 0, '2025-02-13T20:20:00Z',
    'demo_msg_004', 'data_communication_message', 'demo'
);

-- Text: User reply to Rachel (14:22 CST = 20:22 UTC)
INSERT OR IGNORE INTO data_communication_message (
    id, message_id, channel, body,
    from_identifier, from_name, to_identifiers,
    is_read, is_group_message, timestamp,
    source_stream_id, source_table, source_provider
) VALUES (
    'msg_demo_05', 'sms_msg_002', 'sms',
    'omg yes!! I can leave work early. see you at 3',
    'demo-user@phone.com', NULL, '["rachel.torres@realty.com"]',
    1, 0, '2025-02-13T20:22:00Z',
    'demo_msg_005', 'data_communication_message', 'demo'
);

-- Text: Rachel confirmation (14:25 CST = 20:25 UTC)
INSERT OR IGNORE INTO data_communication_message (
    id, message_id, channel, body,
    from_identifier, from_name, to_identifiers,
    is_read, is_group_message, timestamp,
    source_stream_id, source_table, source_provider
) VALUES (
    'msg_demo_06', 'sms_msg_003', 'sms',
    'Perfect — 1847 S 3rd St. I''ll be out front. You''re going to love this one.',
    'rachel.torres@realty.com', 'Rachel Torres', '["demo-user@phone.com"]',
    1, 0, '2025-02-13T20:25:00Z',
    'demo_msg_006', 'data_communication_message', 'demo'
);

-- Text: Jess about game night (17:40 CST = 23:40 UTC — after run ends)
INSERT OR IGNORE INTO data_communication_message (
    id, message_id, channel, body,
    from_identifier, from_name, to_identifiers,
    is_read, is_group_message, timestamp,
    source_stream_id, source_table, source_provider
) VALUES (
    'msg_demo_07', 'sms_msg_004', 'sms',
    'game night tonight?? priya is in. I have catan and wine',
    'jess.landry@email.com', 'Jess Landry', '["demo-user@phone.com", "priya.mehta@email.com"]',
    1, 1, '2025-02-13T23:40:00Z',
    'demo_msg_007', 'data_communication_message', 'demo'
);

-- Text: User reply to Jess (17:45 CST = 23:45 UTC — just finished run, cooling down)
INSERT OR IGNORE INTO data_communication_message (
    id, message_id, channel, body,
    from_identifier, from_name, to_identifiers,
    is_read, is_group_message, timestamp,
    source_stream_id, source_table, source_provider
) VALUES (
    'msg_demo_08', 'sms_msg_005', 'sms',
    'YES. just got back from a run. shower and I''ll be over by 6:30. I have NEWS',
    'demo-user@phone.com', NULL, '["jess.landry@email.com", "priya.mehta@email.com"]',
    1, 1, '2025-02-13T23:45:00Z',
    'demo_msg_008', 'data_communication_message', 'demo'
);

-- Text: Priya reply (17:50 CST = 23:50 UTC)
INSERT OR IGNORE INTO data_communication_message (
    id, message_id, channel, body,
    from_identifier, from_name, to_identifiers,
    is_read, is_group_message, timestamp,
    source_stream_id, source_table, source_provider
) VALUES (
    'msg_demo_09', 'sms_msg_006', 'sms',
    'oooh news?? 👀 omw',
    'priya.mehta@email.com', 'Priya Mehta', '["jess.landry@email.com", "demo-user@phone.com"]',
    1, 1, '2025-02-13T23:50:00Z',
    'demo_msg_009', 'data_communication_message', 'demo'
);

-- ─────────────────────────────────────────────────────────────────────────────
-- 6d. APP USAGE (data_activity_app_usage)
-- ─────────────────────────────────────────────────────────────────────────────

-- Morning: Instagram scroll (06:35-06:50 CST = 12:35-12:50 UTC)
INSERT OR IGNORE INTO data_activity_app_usage (
    id, app_name, app_bundle_id, app_category,
    start_time, end_time, window_title,
    source_stream_id, source_table, source_provider
) VALUES (
    'app_demo_01', 'Instagram', 'com.burbn.instagram', 'Social',
    '2025-02-13T12:35:00Z', '2025-02-13T12:50:00Z', NULL,
    'demo_app_001', 'data_activity_app_usage', 'demo'
);

-- Morning: Apple News (06:50-07:05 CST = 12:50-13:05 UTC)
INSERT OR IGNORE INTO data_activity_app_usage (
    id, app_name, app_bundle_id, app_category,
    start_time, end_time, window_title,
    source_stream_id, source_table, source_provider
) VALUES (
    'app_demo_02', 'Apple News', 'com.apple.news', 'News',
    '2025-02-13T12:50:00Z', '2025-02-13T13:05:00Z', NULL,
    'demo_app_002', 'data_activity_app_usage', 'demo'
);

-- Pre-standup: Slack desktop (07:45-08:15 CST = 13:45-14:15 UTC)
INSERT OR IGNORE INTO data_activity_app_usage (
    id, app_name, app_bundle_id, app_category,
    start_time, end_time, window_title,
    source_stream_id, source_table, source_provider
) VALUES (
    'app_demo_03', 'Slack', 'com.tinyspeck.slackmacgap', 'Productivity',
    '2025-02-13T13:45:00Z', '2025-02-13T14:15:00Z', '#design-team',
    'demo_app_003', 'data_activity_app_usage', 'demo'
);

-- Deep work: Figma (09:05-11:25 CST = 15:05-17:25 UTC)
INSERT OR IGNORE INTO data_activity_app_usage (
    id, app_name, app_bundle_id, app_category,
    start_time, end_time, window_title,
    source_stream_id, source_table, source_provider
) VALUES (
    'app_demo_04', 'Figma', 'com.figma.desktop', 'Design',
    '2025-02-13T15:05:00Z', '2025-02-13T17:25:00Z', 'Navigation Redesign v3 — Figma',
    'demo_app_004', 'data_activity_app_usage', 'demo'
);

-- Post-standup: Notion docs (09:00-09:05 CST = 15:00-15:05 UTC)
INSERT OR IGNORE INTO data_activity_app_usage (
    id, app_name, app_bundle_id, app_category,
    start_time, end_time, window_title, url,
    source_stream_id, source_table, source_provider
) VALUES (
    'app_demo_05', 'Notion', 'notion.id', 'Productivity',
    '2025-02-13T15:00:00Z', '2025-02-13T15:05:00Z',
    'Standup Notes — Feb 13', 'https://notion.so/standup-feb-13',
    'demo_app_005', 'data_activity_app_usage', 'demo'
);

-- Wind down: Safari browsing (22:30-23:15 CST = 04:30-05:15 UTC Feb 14)
INSERT OR IGNORE INTO data_activity_app_usage (
    id, app_name, app_bundle_id, app_category,
    start_time, end_time, window_title, url,
    source_stream_id, source_table, source_provider
) VALUES (
    'app_demo_06', 'Safari', 'com.apple.Safari', 'Web Browser',
    '2025-02-14T04:30:00Z', '2025-02-14T05:00:00Z',
    'Bouldin Creek neighborhood guide — Austin Chronicle', 'https://austinchronicle.com/neighborhoods/bouldin-creek',
    'demo_app_006', 'data_activity_app_usage', 'demo'
);

-- Wind down: Zillow (23:15-23:45 CST = 05:15-05:45 UTC Feb 14)
INSERT OR IGNORE INTO data_activity_app_usage (
    id, app_name, app_bundle_id, app_category,
    start_time, end_time, window_title, url,
    source_stream_id, source_table, source_provider
) VALUES (
    'app_demo_07', 'Safari', 'com.apple.Safari', 'Web Browser',
    '2025-02-14T05:00:00Z', '2025-02-14T05:30:00Z',
    '1847 S 3rd St, Austin TX — Zillow', 'https://zillow.com/homedetails/1847-s-3rd-st-austin-tx',
    'demo_app_007', 'data_activity_app_usage', 'demo'
);

-- Feb 12: Figma (adjacent day)
INSERT OR IGNORE INTO data_activity_app_usage (
    id, app_name, app_bundle_id, app_category,
    start_time, end_time, window_title,
    source_stream_id, source_table, source_provider
) VALUES (
    'app_demo_08', 'Figma', 'com.figma.desktop', 'Design',
    '2025-02-12T15:00:00Z', '2025-02-12T17:30:00Z', 'Settings Page v2 — Figma',
    'demo_app_008', 'data_activity_app_usage', 'demo'
);

-- Feb 12: Slack (adjacent day)
INSERT OR IGNORE INTO data_activity_app_usage (
    id, app_name, app_bundle_id, app_category,
    start_time, end_time, window_title,
    source_stream_id, source_table, source_provider
) VALUES (
    'app_demo_09', 'Slack', 'com.tinyspeck.slackmacgap', 'Productivity',
    '2025-02-12T18:00:00Z', '2025-02-12T22:30:00Z', '#design-team',
    'demo_app_009', 'data_activity_app_usage', 'demo'
);

-- ─────────────────────────────────────────────────────────────────────────────
-- 6e. STEPS (data_health_steps)
-- ─────────────────────────────────────────────────────────────────────────────
-- Step counts captured at intervals. Bike commute and run produce step signals.

INSERT OR IGNORE INTO data_health_steps (
    id, step_count, timestamp,
    source_stream_id, source_table, source_provider
) VALUES
-- Bike commute to work (07:15-07:45 CST = 13:15-13:45 UTC) — pedaling registers as steps
('steps_demo_01', 320, '2025-02-13T13:20:00Z', 'demo_steps_001', 'data_health_steps', 'demo'),
('steps_demo_02', 410, '2025-02-13T13:30:00Z', 'demo_steps_002', 'data_health_steps', 'demo'),
('steps_demo_03', 280, '2025-02-13T13:40:00Z', 'demo_steps_003', 'data_health_steps', 'demo'),

-- Walking around office + lunch (~400 steps/hour ambient)
('steps_demo_04', 420, '2025-02-13T15:00:00Z', 'demo_steps_004', 'data_health_steps', 'demo'),
('steps_demo_05', 380, '2025-02-13T16:00:00Z', 'demo_steps_005', 'data_health_steps', 'demo'),
('steps_demo_06', 850, '2025-02-13T17:30:00Z', 'demo_steps_006', 'data_health_steps', 'demo'),
('steps_demo_07', 620, '2025-02-13T18:30:00Z', 'demo_steps_007', 'data_health_steps', 'demo'),

-- Run at Mueller trails (16:45-17:30 CST = 22:45-23:30 UTC) — high cadence
('steps_demo_08', 890, '2025-02-13T22:50:00Z', 'demo_steps_008', 'data_health_steps', 'demo'),
('steps_demo_09', 920, '2025-02-13T23:00:00Z', 'demo_steps_009', 'data_health_steps', 'demo'),
('steps_demo_10', 940, '2025-02-13T23:10:00Z', 'demo_steps_010', 'data_health_steps', 'demo'),
('steps_demo_11', 910, '2025-02-13T23:20:00Z', 'demo_steps_011', 'data_health_steps', 'demo'),

-- Feb 14: Walk at Lady Bird Lake
('steps_demo_12', 1200, '2025-02-14T17:45:00Z', 'demo_steps_012', 'data_health_steps', 'demo'),
('steps_demo_13', 1350, '2025-02-14T18:15:00Z', 'demo_steps_013', 'data_health_steps', 'demo'),
('steps_demo_14', 980, '2025-02-14T18:45:00Z', 'demo_steps_014', 'data_health_steps', 'demo');

-- ─────────────────────────────────────────────────────────────────────────────
-- 6f. HEART RATE (data_health_heart_rate)
-- ─────────────────────────────────────────────────────────────────────────────
-- Elevated readings during the run (16:45-17:30 CST = 22:45-23:30 UTC)

INSERT OR IGNORE INTO data_health_heart_rate (
    id, bpm, timestamp,
    source_stream_id, source_table, source_provider
) VALUES
-- Resting before run
('hr_demo_01', 68, '2025-02-13T22:40:00Z', 'demo_hr_001', 'data_health_heart_rate', 'demo'),
-- Warm-up
('hr_demo_02', 95, '2025-02-13T22:47:00Z', 'demo_hr_002', 'data_health_heart_rate', 'demo'),
('hr_demo_03', 118, '2025-02-13T22:50:00Z', 'demo_hr_003', 'data_health_heart_rate', 'demo'),
-- Steady state
('hr_demo_04', 148, '2025-02-13T22:55:00Z', 'demo_hr_004', 'data_health_heart_rate', 'demo'),
('hr_demo_05', 155, '2025-02-13T23:00:00Z', 'demo_hr_005', 'data_health_heart_rate', 'demo'),
('hr_demo_06', 158, '2025-02-13T23:05:00Z', 'demo_hr_006', 'data_health_heart_rate', 'demo'),
('hr_demo_07', 162, '2025-02-13T23:10:00Z', 'demo_hr_007', 'data_health_heart_rate', 'demo'),
-- Peak effort
('hr_demo_08', 168, '2025-02-13T23:15:00Z', 'demo_hr_008', 'data_health_heart_rate', 'demo'),
('hr_demo_09', 165, '2025-02-13T23:20:00Z', 'demo_hr_009', 'data_health_heart_rate', 'demo'),
-- Cool down
('hr_demo_10', 142, '2025-02-13T23:25:00Z', 'demo_hr_010', 'data_health_heart_rate', 'demo'),
('hr_demo_11', 118, '2025-02-13T23:30:00Z', 'demo_hr_011', 'data_health_heart_rate', 'demo'),
('hr_demo_12', 92, '2025-02-13T23:35:00Z', 'demo_hr_012', 'data_health_heart_rate', 'demo');

-- ─────────────────────────────────────────────────────────────────────────────
-- 6g. WORKOUT (data_health_workout)
-- ─────────────────────────────────────────────────────────────────────────────

-- Run at Mueller trails (16:45-17:30 CST = 22:45-23:30 UTC)
INSERT OR IGNORE INTO data_health_workout (
    id, workout_type, start_time, end_time,
    duration_minutes, calories_burned, distance_km,
    avg_heart_rate, max_heart_rate,
    source_stream_id, source_table, source_provider
) VALUES (
    'workout_demo_run', 'running', '2025-02-13T22:45:00Z', '2025-02-13T23:30:00Z',
    45, 380, 5.2,
    152, 168,
    'demo_workout_001', 'data_health_workout', 'demo'
);

-- ─────────────────────────────────────────────────────────────────────────────
-- 6h. TRANSCRIPTION (data_communication_transcription)
-- ─────────────────────────────────────────────────────────────────────────────

-- User research session recording (12:30-14:15 CST = 18:30-20:15 UTC)
INSERT OR IGNORE INTO data_communication_transcription (
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
    '2025-02-13T18:30:00Z', '2025-02-13T20:15:00Z',
    4, 'Navigation Redesign — Usability Test Round 3',
    'Three participants tested the navigation redesign. Key findings: settings discoverability is poor (notifications buried under Preferences tab), users expect a bell icon for notification access, icon-only nav items need labels or tooltips. All participants found the main avatar dropdown intuitive.',
    0.94,
    '["ux-research", "navigation", "usability-testing"]',
    'demo_txn_001', 'data_communication_transcription', 'demo'
);

-- Design standup recording (08:15-09:00 CST = 14:15-15:00 UTC)
INSERT OR IGNORE INTO data_communication_transcription (
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

User: Just a heads up — I might need to leave a bit early today. Rachel found a house in Bouldin Creek that just came back on market.

Maya: Oh nice! Go look at it. We''ve got things covered here.',
    'en', 2700,
    '2025-02-13T14:15:00Z', '2025-02-13T15:00:00Z',
    3, 'Design Team Standup — Feb 13',
    'Discussed onboarding funnel drop-off: step 3 completion fell from 51% to 34% after redesign. Two hypotheses — increased field count and aggressive inline validation. Plan to test both in afternoon research session. David to mock lazy validation prototype by noon.',
    0.92,
    '["standup", "design-team", "onboarding"]',
    '{"people": ["Maya Chen", "David Okafor"], "topics": ["onboarding funnel", "form validation", "step 3 drop-off"], "products": ["Figma"]}',
    '[{"speaker": "Maya Chen", "start": 0.0, "end": 15.2}, {"speaker": "David Okafor", "start": 15.2, "end": 28.5}, {"speaker": "Maya Chen", "start": 28.5, "end": 33.1}, {"speaker": "User", "start": 33.1, "end": 58.4}, {"speaker": "David Okafor", "start": 58.4, "end": 74.0}, {"speaker": "Maya Chen", "start": 74.0, "end": 88.3}, {"speaker": "User", "start": 88.3, "end": 112.0}, {"speaker": "Maya Chen", "start": 112.0, "end": 135.6}, {"speaker": "David Okafor", "start": 135.6, "end": 142.8}, {"speaker": "Maya Chen", "start": 142.8, "end": 150.1}, {"speaker": "User", "start": 150.1, "end": 168.0}, {"speaker": "Maya Chen", "start": 168.0, "end": 172.5}]',
    'demo_txn_003', 'data_communication_transcription', 'demo'
);

-- Lunch conversation with Maya at Ramen Tatsu-ya (11:30-12:30 CST = 17:30-18:30 UTC)
INSERT OR IGNORE INTO data_communication_transcription (
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
    '2025-02-13T17:30:00Z', '2025-02-13T18:30:00Z',
    2, 'Lunch with Maya — Ramen Tatsu-ya',
    'Casual lunch conversation. Maya weighing new hire decision — strong portfolio but noncommittal in interview. Discussed nav redesign progress and upcoming research session. Mentioned Bouldin Creek house coming back on market.',
    0.79,
    '["personal", "work", "lunch"]',
    'https://demo.virtues.app/audio/txn_demo_lunch.m4a',
    '{"ambient_noise_level": "high", "recording_device": "iPhone 15 Pro", "environment": "restaurant"}',
    'demo_txn_004', 'data_communication_transcription', 'demo'
);

-- Voice memo after house showing (~15:50 CST = 21:50 UTC)
INSERT OR IGNORE INTO data_communication_transcription (
    id, text, language, duration_seconds,
    start_time, end_time,
    speaker_count, title, summary, confidence,
    tags, entities,
    source_stream_id, source_table, source_provider
) VALUES (
    'txn_demo_voice_memo',
    'Okay, just walked out of the house on South 3rd. I need to get this down before I forget.

The kitchen — the tile is original. Like, 1940s original. It''s this deep terracotta with a cream border and it''s in perfect condition. The light in the afternoon comes through the south-facing windows and hits the tile and it just... glows. I stood there for a minute and Rachel didn''t rush me.

The backyard is way bigger than the photos. There''s a mature pecan tree in the back corner and enough space for a real garden. The fence is wood, needs some work, but the bones are there.

The neighborhood feels right. I walked a few blocks after — it''s quiet but not dead. I can see the SoCo restaurants from the corner. Jo''s is a five-minute walk. There''s a little free library on the next block.

Price is 485. That''s at the top of my range but under the hard ceiling. Monthly would be around 2,800 with the rate I was quoted.

I think I want to put in an offer. I need to sleep on it. But I think this is the one.',
    'en', 180,
    '2025-02-13T21:50:00Z', '2025-02-13T21:53:00Z',
    1, 'Voice memo — Bouldin Creek house',
    'Post-showing voice note. Loved the original 1940s terracotta tile, large backyard with pecan tree, walkable to South Congress. Price at $485K, monthly ~$2,800. Strongly considering an offer.',
    0.96,
    '["personal", "house-hunting", "voice-memo"]',
    '{"places": ["1847 S 3rd St", "Bouldin Creek", "South Congress", "Jo''s Coffee"], "price": "$485,000", "features": ["original tile", "pecan tree", "south-facing windows"]}',
    'demo_txn_005', 'data_communication_transcription', 'demo'
);

-- Feb 14: Phone call with Mom (for adjacent day)
INSERT OR IGNORE INTO data_communication_transcription (
    id, text, language, duration_seconds,
    start_time, end_time,
    speaker_count, title, summary, confidence,
    source_stream_id, source_table, source_provider
) VALUES (
    'txn_demo_mom',
    'Summary: Caught up with Mom about the week. Mentioned looking at a house in Bouldin Creek. She asked about the neighborhood and whether it was safe. Talked about Valentine''s Day plans — nothing special, just a quiet Friday. She mentioned Dad''s knee surgery is scheduled for March.',
    'en', 3600,
    '2025-02-14T19:30:00Z', '2025-02-14T20:30:00Z',
    2, 'Phone call with Mom', 'Weekly catch-up. Discussed house hunting, Valentine''s plans, Dad''s upcoming knee surgery in March.',
    0.88,
    'demo_txn_002', 'data_communication_transcription', 'demo'
);

-- ─────────────────────────────────────────────────────────────────────────────
-- 6i. LOCATION POINTS (data_location_point)
-- ─────────────────────────────────────────────────────────────────────────────
-- GPS breadcrumbs during transit and the run. These feed location clustering.

INSERT OR IGNORE INTO data_location_point (
    id, latitude, longitude, horizontal_accuracy, timestamp,
    source_stream_id, source_table, source_provider
) VALUES
-- Bike commute: Mueller → Downtown (07:15-07:45 CST = 13:15-13:45 UTC)
('lp_demo_01', 30.2989, -97.7055, 5.0, '2025-02-13T13:15:00Z', 'demo_lp_001', 'data_location_point', 'demo'),
('lp_demo_02', 30.2920, -97.7120, 8.0, '2025-02-13T13:20:00Z', 'demo_lp_002', 'data_location_point', 'demo'),
('lp_demo_03', 30.2850, -97.7200, 6.0, '2025-02-13T13:25:00Z', 'demo_lp_003', 'data_location_point', 'demo'),
('lp_demo_04', 30.2780, -97.7280, 5.0, '2025-02-13T13:30:00Z', 'demo_lp_004', 'data_location_point', 'demo'),
('lp_demo_05', 30.2720, -97.7350, 7.0, '2025-02-13T13:35:00Z', 'demo_lp_005', 'data_location_point', 'demo'),
('lp_demo_06', 30.2672, -97.7431, 5.0, '2025-02-13T13:45:00Z', 'demo_lp_006', 'data_location_point', 'demo'),

-- Drive: Office → Bouldin Creek house (14:30-15:00 CST = 20:30-21:00 UTC)
('lp_demo_07', 30.2672, -97.7431, 10.0, '2025-02-13T20:30:00Z', 'demo_lp_007', 'data_location_point', 'demo'),
('lp_demo_08', 30.2600, -97.7480, 12.0, '2025-02-13T20:40:00Z', 'demo_lp_008', 'data_location_point', 'demo'),
('lp_demo_09', 30.2520, -97.7530, 8.0, '2025-02-13T20:50:00Z', 'demo_lp_009', 'data_location_point', 'demo'),
('lp_demo_10', 30.2480, -97.7580, 5.0, '2025-02-13T21:00:00Z', 'demo_lp_010', 'data_location_point', 'demo'),

-- Run: Mueller trails loop (16:45-17:30 CST = 22:45-23:30 UTC)
('lp_demo_11', 30.2989, -97.7055, 4.0, '2025-02-13T22:45:00Z', 'demo_lp_011', 'data_location_point', 'demo'),
('lp_demo_12', 30.3010, -97.7030, 5.0, '2025-02-13T22:50:00Z', 'demo_lp_012', 'data_location_point', 'demo'),
('lp_demo_13', 30.3040, -97.7010, 4.0, '2025-02-13T22:55:00Z', 'demo_lp_013', 'data_location_point', 'demo'),
('lp_demo_14', 30.3060, -97.6990, 5.0, '2025-02-13T23:00:00Z', 'demo_lp_014', 'data_location_point', 'demo'),
('lp_demo_15', 30.3050, -97.7020, 4.0, '2025-02-13T23:05:00Z', 'demo_lp_015', 'data_location_point', 'demo'),
('lp_demo_16', 30.3030, -97.7040, 5.0, '2025-02-13T23:10:00Z', 'demo_lp_016', 'data_location_point', 'demo'),
('lp_demo_17', 30.3010, -97.7050, 4.0, '2025-02-13T23:15:00Z', 'demo_lp_017', 'data_location_point', 'demo'),
('lp_demo_18', 30.2995, -97.7055, 5.0, '2025-02-13T23:20:00Z', 'demo_lp_018', 'data_location_point', 'demo'),
('lp_demo_19', 30.2989, -97.7055, 4.0, '2025-02-13T23:28:00Z', 'demo_lp_019', 'data_location_point', 'demo'),

-- Drive: Home → Jess's place (18:15-18:30 CST = 00:15-00:30 UTC Feb 14)
('lp_demo_20', 30.2989, -97.7055, 8.0, '2025-02-14T00:15:00Z', 'demo_lp_020', 'data_location_point', 'demo'),
('lp_demo_21', 30.2750, -97.7300, 10.0, '2025-02-14T00:22:00Z', 'demo_lp_021', 'data_location_point', 'demo'),
('lp_demo_22', 30.2520, -97.7545, 5.0, '2025-02-14T00:30:00Z', 'demo_lp_022', 'data_location_point', 'demo'),

-- Drive: Jess's place → Home (22:00-22:15 CST = 04:00-04:15 UTC Feb 14)
('lp_demo_23', 30.2520, -97.7545, 6.0, '2025-02-14T04:00:00Z', 'demo_lp_023', 'data_location_point', 'demo'),
('lp_demo_24', 30.2750, -97.7300, 9.0, '2025-02-14T04:08:00Z', 'demo_lp_024', 'data_location_point', 'demo'),
('lp_demo_25', 30.2989, -97.7055, 5.0, '2025-02-14T04:15:00Z', 'demo_lp_025', 'data_location_point', 'demo');

-- =============================================================================
-- 7. WIKI ENTITIES (people, places, organizations)
-- =============================================================================

-- ─────────────────────────────────────────────────────────────────────────────
-- 7a. PEOPLE (wiki_people)
-- ─────────────────────────────────────────────────────────────────────────────

-- Maya Chen — design team lead, close colleague
INSERT OR IGNORE INTO wiki_people (
    id, canonical_name, emails, phones,
    relationship_category, notes,
    first_interaction, last_interaction, interaction_count
) VALUES (
    'person_demo_maya', 'Maya Chen',
    '["maya.chen@company.com"]', '[]',
    'colleague',
    'Design team lead. Sharp eye for UX patterns, always pushing for better onboarding flows. Lunch buddy — we hit Tatsu-ya at least once a week.',
    '2024-06-15', '2025-02-13', 215
);

-- David Okafor — design team, frontend-leaning
INSERT OR IGNORE INTO wiki_people (
    id, canonical_name, emails, phones,
    relationship_category, notes,
    first_interaction, last_interaction, interaction_count
) VALUES (
    'person_demo_david', 'David Okafor',
    '["david.okafor@company.com"]', '[]',
    'colleague',
    'Design engineer on the team. Great at bridging design and code. Always first to flag form validation issues.',
    '2024-06-15', '2025-02-13', 180
);

-- Rachel Torres — realtor
INSERT OR IGNORE INTO wiki_people (
    id, canonical_name, emails, phones,
    relationship_category, notes,
    first_interaction, last_interaction, interaction_count
) VALUES (
    'person_demo_rachel', 'Rachel Torres',
    '["rachel.torres@realty.com"]', '["512-555-0147"]',
    'professional',
    'Realtor helping with the house search. Found the Bouldin Creek place on S 3rd. Responsive and knows the Austin market well.',
    '2025-01-08', '2025-02-13', 24
);

-- Jess Landry — close friend
INSERT OR IGNORE INTO wiki_people (
    id, canonical_name, emails, phones,
    relationship_category, nickname, notes,
    first_interaction, last_interaction, interaction_count
) VALUES (
    'person_demo_jess', 'Jess Landry',
    '["jess.landry@email.com"]', '["512-555-0233"]',
    'friend', 'Jess',
    'One of my closest friends in Austin. Lives on South Lamar. Always down for game night — her Catan strategy is ruthless.',
    '2023-03-20', '2025-02-13', 340
);

-- Priya Mehta — close friend
INSERT OR IGNORE INTO wiki_people (
    id, canonical_name, emails, phones,
    relationship_category, notes,
    first_interaction, last_interaction, interaction_count
) VALUES (
    'person_demo_priya', 'Priya Mehta',
    '["priya.mehta@email.com"]', '["512-555-0891"]',
    'friend',
    'Part of the game night crew with Jess. Works in data science at a climate tech startup. Always brings good wine.',
    '2023-09-10', '2025-02-13', 145
);

-- Mom
INSERT OR IGNORE INTO wiki_people (
    id, canonical_name, phones,
    relationship_category, nickname, notes,
    first_interaction, last_interaction, interaction_count
) VALUES (
    'person_demo_mom', 'Linda',
    '["512-555-0012"]',
    'family', 'Mom',
    'Weekly calls, usually Friday evenings. Dad''s knee surgery coming up in March.',
    '1990-01-01', '2025-02-14', 9999
);

-- ─────────────────────────────────────────────────────────────────────────────
-- 7b. PLACES (wiki_places)
-- ─────────────────────────────────────────────────────────────────────────────

-- Home — Mueller, East Austin
INSERT OR IGNORE INTO wiki_places (
    id, name, category, address,
    latitude, longitude, radius_m,
    visit_count, first_visit, last_visit
) VALUES (
    'place_demo_home', 'Home', 'home',
    'Mueller, Austin, TX',
    30.2989, -97.7055, 50.0,
    365, '2024-02-01', '2025-02-14'
);

-- Office — Downtown Austin
INSERT OR IGNORE INTO wiki_places (
    id, name, category, address,
    latitude, longitude, radius_m,
    visit_count, first_visit, last_visit
) VALUES (
    'place_demo_office', 'Office', 'workplace',
    'Downtown Austin, TX',
    30.2672, -97.7431, 80.0,
    220, '2024-06-15', '2025-02-14'
);

-- Ramen Tatsu-ya
INSERT OR IGNORE INTO wiki_places (
    id, name, category, address,
    latitude, longitude, radius_m,
    visit_count, first_visit, last_visit,
    content
) VALUES (
    'place_demo_ramen', 'Ramen Tatsu-ya', 'restaurant',
    '8557 Research Blvd, Austin, TX 78758',
    30.2700, -97.7400, 40.0,
    18, '2024-07-02', '2025-02-13',
    'Go-to lunch spot with Maya. The original Tatsu-ya miso is unbeatable.'
);

-- Jo's Coffee — South Congress
INSERT OR IGNORE INTO wiki_places (
    id, name, category, address,
    latitude, longitude, radius_m,
    visit_count, first_visit, last_visit,
    content
) VALUES (
    'place_demo_jos', 'Jo''s Coffee', 'cafe',
    '1300 S Congress Ave, Austin, TX 78704',
    30.2510, -97.7490, 30.0,
    12, '2024-04-18', '2025-02-13',
    'South Congress classic. Good people-watching spot.'
);

-- 1847 S 3rd St — Bouldin Creek house showing
INSERT OR IGNORE INTO wiki_places (
    id, name, category, address,
    latitude, longitude, radius_m,
    visit_count, first_visit, last_visit,
    content
) VALUES (
    'place_demo_house', '1847 S 3rd St', 'residential',
    '1847 S 3rd St, Austin, TX 78704',
    30.2480, -97.7580, 30.0,
    1, '2025-02-13', '2025-02-13',
    'Bouldin Creek bungalow. Original tile in the kitchen, big backyard. Back on market Feb 13. Rachel showed it — sunlight was perfect in the afternoon.'
);

-- Jess's Place — South Lamar
INSERT OR IGNORE INTO wiki_places (
    id, name, category, address,
    latitude, longitude, radius_m,
    visit_count, first_visit, last_visit
) VALUES (
    'place_demo_jess', 'Jess''s Place', 'residential',
    'South Lamar, Austin, TX',
    30.2520, -97.7545, 40.0,
    28, '2023-04-10', '2025-02-13'
);

-- Lady Bird Lake
INSERT OR IGNORE INTO wiki_places (
    id, name, category, address,
    latitude, longitude, radius_m,
    visit_count, first_visit, last_visit
) VALUES (
    'place_demo_ladybird', 'Lady Bird Lake', 'park',
    'Lady Bird Lake, Austin, TX',
    30.2615, -97.7480, 500.0,
    35, '2023-06-01', '2025-02-14'
);

-- Mueller trails
INSERT OR IGNORE INTO wiki_places (
    id, name, category, address,
    latitude, longitude, radius_m,
    visit_count, first_visit, last_visit,
    content
) VALUES (
    'place_demo_mueller_trails', 'Mueller Trails', 'park',
    'Mueller, Austin, TX',
    30.3030, -97.7020, 300.0,
    48, '2024-02-15', '2025-02-13',
    'Regular running route. ~5K loop from home. Good mix of paved and gravel.'
);

-- ─────────────────────────────────────────────────────────────────────────────
-- 7c. ORGANIZATIONS (wiki_orgs)
-- ─────────────────────────────────────────────────────────────────────────────

-- Employer — product design company
INSERT OR IGNORE INTO wiki_orgs (
    id, canonical_name, organization_type,
    primary_place_id, relationship_type, role_title,
    start_date, interaction_count,
    first_interaction, last_interaction,
    content
) VALUES (
    'org_demo_employer', 'Canopy', 'company',
    'place_demo_office', 'employee', 'Senior UX Designer',
    '2024-06-15', 220,
    '2024-06-15', '2025-02-14',
    'B2B SaaS product. Small design team — Maya (lead), David, and me. Currently deep in a navigation redesign.'
);

-- Torres Realty — Rachel's agency
INSERT OR IGNORE INTO wiki_orgs (
    id, canonical_name, organization_type,
    relationship_type,
    interaction_count, first_interaction, last_interaction
) VALUES (
    'org_demo_realty', 'Torres Realty', 'company',
    'client',
    24, '2025-01-08', '2025-02-13'
);
