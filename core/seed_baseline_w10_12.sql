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

INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline)
VALUES
('day_2026-01-26', '2026-01-26', 'America/Chicago', 'America/Chicago', 0.48),
('day_2026-01-27', '2026-01-27', 'America/Chicago', 'America/Chicago', 0.52),
('day_2026-01-28', '2026-01-28', 'America/Chicago', 'America/Chicago', 0.50),
('day_2026-01-29', '2026-01-29', 'America/Chicago', 'America/Chicago', 0.45),
('day_2026-01-30', '2026-01-30', 'America/Chicago', 'America/Chicago', 0.53),
('day_2026-01-31', '2026-01-31', 'America/Chicago', 'America/Chicago', 0.55),
('day_2026-02-01', '2026-02-01', 'America/Chicago', 'America/Chicago', 0.47),
('day_2026-02-02', '2026-02-02', 'America/Chicago', 'America/Chicago', 0.50),
('day_2026-02-03', '2026-02-03', 'America/Chicago', 'America/Chicago', 0.44),
('day_2026-02-04', '2026-02-04', 'America/Chicago', 'America/Chicago', 0.51),
('day_2026-02-05', '2026-02-05', 'America/Chicago', 'America/Chicago', 0.46),
('day_2026-02-06', '2026-02-06', 'America/Chicago', 'America/Chicago', 0.54),
('day_2026-02-07', '2026-02-07', 'America/Chicago', 'America/Chicago', 0.58),
('day_2026-02-08', '2026-02-08', 'America/Chicago', 'America/Chicago', 0.49),
('day_2026-02-09', '2026-02-09', 'America/Chicago', 'America/Chicago', 0.42),
('day_2026-02-10', '2026-02-10', 'America/Chicago', 'America/Chicago', 0.50),
('day_2026-02-11', '2026-02-11', 'America/Chicago', 'America/Chicago', 0.48);

-- ─────────────────────────────────────────────────────────────────────────────
-- Wiki Events
-- ─────────────────────────────────────────────────────────────────────────────

-- =============================================================================
-- Monday, January 26, 2026 (10 events)
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES
('ev_b0631', 'day_2026-01-26', '2026-01-26T06:00:00Z', '2026-01-26T12:30:00Z',
 'Sleep', 'Home', '["sleep"]',
 0, 0, 1, 0, 0,
 'Overnight sleep, about 6.5 hours.', '["sleep"]', '[]',
 NULL, NULL, NULL, 'NEW', 57),

('ev_b0632', 'day_2026-01-26', '2026-01-26T12:30:00Z', '2026-01-26T13:15:00Z',
 'Morning routine', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Coffee and catching up on Slack messages before heading out.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 65),

('ev_b0633', 'day_2026-01-26', '2026-01-26T13:15:00Z', '2026-01-26T13:45:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 0, 1, 0, 0, 0,
 'Bike commute to the downtown office, 30 minutes.', '["commute", "cycling", "podcast"]', '[]',
 NULL, NULL, NULL, 'NEW', 126),

('ev_b0634', 'day_2026-01-26', '2026-01-26T13:45:00Z', '2026-01-26T14:15:00Z',
 'Coffee and Slack', 'Office', '["app_usage", "message"]',
 0, 0, 0, 0, 0,
 'Settled in at the office with coffee and cleared Slack notifications.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 71),

('ev_b0635', 'day_2026-01-26', '2026-01-26T14:15:00Z', '2026-01-26T15:00:00Z',
 'Design standup', 'Office', '["calendar", "message", "transcription"]',
 0, 0, 0, 0, 0,
 'Monday standup with Maya and David reviewing onboarding funnel metrics from last week.', '["meeting", "standup", "design", "onboarding"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 73),

('ev_b0636', 'day_2026-01-26', '2026-01-26T15:00:00Z', '2026-01-26T17:30:00Z',
 'Focused design work', 'Office', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Deep work on the onboarding step-progress component in Figma.', '["design", "figma", "focus", "deep-work", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 66),

('ev_b0637', 'day_2026-01-26', '2026-01-26T17:30:00Z', '2026-01-26T18:30:00Z',
 'Lunch', 'Office', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Ate lunch at desk while reading design articles.', '["food", "browsing"]', '["place_demo_office"]',
 NULL, NULL, NULL, 'NEW', 73),

('ev_b0638', 'day_2026-01-26', '2026-01-26T18:30:00Z', '2026-01-26T22:30:00Z',
 'Afternoon work', 'Office', '["app_usage", "message"]',
 0, 0, 0, 0, 0,
 'Continued onboarding flow wireframes and responded to design feedback on Slack.', '["work", "design", "figma", "onboarding", "messaging"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 68),

('ev_b0639', 'day_2026-01-26', '2026-01-26T22:30:00Z', '2026-01-26T23:00:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 0, 1, 0, 0, 0,
 'Bike commute home from the office.', '["commute", "cycling"]', '[]',
 NULL, NULL, NULL, 'NEW', 122),

('ev_b0640', 'day_2026-01-26', '2026-01-26T23:00:00Z', '2026-01-27T04:00:00Z',
 'Dinner and reading', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Made stir fry for dinner and read a few chapters of a novel before bed.', '["food", "leisure", "reflection"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 63);

-- =============================================================================
-- Tuesday, January 27, 2026 (10 events)
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES
('ev_b0641', 'day_2026-01-27', '2026-01-27T06:00:00Z', '2026-01-27T12:30:00Z',
 'Sleep', 'Home', '["sleep"]',
 0, 0, 1, 0, 0,
 'Overnight sleep, about 6.5 hours.', '["sleep"]', '[]',
 NULL, NULL, NULL, 'NEW', 62),

('ev_b0642', 'day_2026-01-27', '2026-01-27T12:30:00Z', '2026-01-27T13:15:00Z',
 'Morning routine', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Morning coffee and checked texts from Jess about weekend plans.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 67),

('ev_b0643', 'day_2026-01-27', '2026-01-27T13:15:00Z', '2026-01-27T13:45:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 0, 1, 0, 0, 0,
 'Bike commute to the office.', '["commute", "cycling", "podcast"]', '[]',
 NULL, NULL, NULL, 'NEW', 124),

('ev_b0644', 'day_2026-01-27', '2026-01-27T13:45:00Z', '2026-01-27T14:15:00Z',
 'Coffee and Slack', 'Office', '["app_usage", "message"]',
 0, 0, 0, 0, 0,
 'Grabbed coffee and synced up on Slack threads about the onboarding redesign.', '["messaging", "work", "coffee", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 66),

('ev_b0645', 'day_2026-01-27', '2026-01-27T14:15:00Z', '2026-01-27T15:00:00Z',
 'Design standup', 'Office', '["calendar", "message", "transcription"]',
 0, 0, 0, 0, 0,
 'Tuesday standup with Maya and David, discussed funnel drop-off at the email verification step.', '["meeting", "standup", "design", "onboarding", "form-validation"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 73),

('ev_b0646', 'day_2026-01-27', '2026-01-27T15:00:00Z', '2026-01-27T16:00:00Z',
 'Design review', 'Office', '["calendar", "app_usage"]',
 0, 0, 0, 0, 0,
 'Design review session with David on onboarding form validation patterns.', '["meeting", "design-review", "design", "onboarding", "form-validation"]', '["person_demo_david", "place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 73),

('ev_b0647', 'day_2026-01-27', '2026-01-27T16:00:00Z', '2026-01-27T17:30:00Z',
 'Focused design work', 'Office', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Iterated on the form validation error states in Figma.', '["design", "figma", "focus", "deep-work", "onboarding", "form-validation"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 62),

('ev_b0648', 'day_2026-01-27', '2026-01-27T17:30:00Z', '2026-01-27T18:30:00Z',
 'Lunch', 'Office', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Quick lunch at the office, ate at desk.', '["food"]', '["place_demo_office"]',
 NULL, NULL, NULL, 'NEW', 75),

('ev_b0649', 'day_2026-01-27', '2026-01-27T18:30:00Z', '2026-01-27T22:30:00Z',
 'Afternoon work', 'Office', '["app_usage", "message"]',
 0, 0, 0, 0, 0,
 'Wrapped up onboarding wireframes and posted updates to the design channel.', '["work", "design", "figma", "onboarding", "messaging"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 64),

('ev_b0650', 'day_2026-01-27', '2026-01-27T23:00:00Z', '2026-01-28T01:00:00Z',
 'Evening run', 'Mueller Trails', '["steps", "location_visit"]',
 0, 0, 0, 0, 0,
 'Evening 3-mile run on Mueller trails, good pace.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
 NULL, NULL, NULL, 'NEW', 68);

-- =============================================================================
-- Wednesday, January 28, 2026 (10 events)
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES
('ev_b0651', 'day_2026-01-28', '2026-01-28T06:00:00Z', '2026-01-28T12:30:00Z',
 'Sleep', 'Home', '["sleep"]',
 0, 0, 1, 0, 0,
 'Overnight sleep, about 6.5 hours.', '["sleep"]', '[]',
 NULL, NULL, NULL, 'NEW', 58),

('ev_b0652', 'day_2026-01-28', '2026-01-28T12:30:00Z', '2026-01-28T13:15:00Z',
 'Morning routine', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Morning coffee and browsed design blogs.', '["routine", "morning", "coffee", "browsing"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 67),

('ev_b0653', 'day_2026-01-28', '2026-01-28T13:15:00Z', '2026-01-28T13:45:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 0, 1, 0, 0, 0,
 'Bike commute to the office.', '["commute", "cycling", "podcast"]', '[]',
 NULL, NULL, NULL, 'NEW', 117),

('ev_b0654', 'day_2026-01-28', '2026-01-28T13:45:00Z', '2026-01-28T14:15:00Z',
 'Coffee and Slack', 'Office', '["app_usage", "message"]',
 0, 0, 0, 0, 0,
 'Coffee at the office and caught up on Slack.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 65),

('ev_b0655', 'day_2026-01-28', '2026-01-28T14:15:00Z', '2026-01-28T15:00:00Z',
 'Design standup', 'Office', '["calendar", "message", "transcription"]',
 0, 0, 0, 0, 0,
 'Wednesday standup, Maya flagged a navigation redesign issue in the onboarding flow.', '["meeting", "standup", "design", "onboarding", "navigation"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 71),

('ev_b0656', 'day_2026-01-28', '2026-01-28T15:00:00Z', '2026-01-28T17:30:00Z',
 'Focused design work', 'Office', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Worked on the navigation redesign for the onboarding sidebar in Figma.', '["design", "figma", "focus", "deep-work", "onboarding", "navigation", "sidebar"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 67),

('ev_b0657', 'day_2026-01-28', '2026-01-28T17:30:00Z', '2026-01-28T18:30:00Z',
 'Lunch at Ramen Tatsu-ya', 'Ramen Tatsu-ya', '["location_visit"]',
 0, 0, 0, 0, 0,
 'Weekly lunch with Maya at Tatsu-ya, talked about the onboarding sprint timeline.', '["food", "social", "ramen"]', '["person_demo_maya", "place_demo_ramen"]',
 NULL, NULL, NULL, 'NEW', 70),

('ev_b0658', 'day_2026-01-28', '2026-01-28T18:30:00Z', '2026-01-28T22:30:00Z',
 'Afternoon work', 'Office', '["app_usage", "message"]',
 0, 0, 0, 0, 0,
 'Afternoon heads-down on the onboarding navigation prototype.', '["work", "focus", "deep-work", "onboarding", "navigation", "figma"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 67),

('ev_b0659', 'day_2026-01-28', '2026-01-28T22:30:00Z', '2026-01-28T23:00:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 0, 1, 0, 0, 0,
 'Bike commute home.', '["commute", "cycling"]', '[]',
 NULL, NULL, NULL, 'NEW', 112),

('ev_b0660', 'day_2026-01-28', '2026-01-28T23:00:00Z', '2026-01-29T04:00:00Z',
 'Evening at home', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Cooked a simple pasta dinner and watched a documentary.', '["food", "leisure"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 60);

-- =============================================================================
-- Thursday, January 29, 2026 (10 events — WFH afternoon)
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES
('ev_b0661', 'day_2026-01-29', '2026-01-29T06:00:00Z', '2026-01-29T12:45:00Z',
 'Sleep', 'Home', '["sleep"]',
 0, 0, 1, 0, 0,
 'Slept in a bit, about 6 hours 45 minutes.', '["sleep"]', '[]',
 NULL, NULL, NULL, 'NEW', 60),

('ev_b0662', 'day_2026-01-29', '2026-01-29T12:45:00Z', '2026-01-29T13:15:00Z',
 'Morning routine', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Quick morning routine and coffee.', '["routine", "morning", "coffee"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 63),

('ev_b0663', 'day_2026-01-29', '2026-01-29T13:15:00Z', '2026-01-29T13:45:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 0, 1, 0, 0, 0,
 'Bike commute to the office.', '["commute", "cycling"]', '[]',
 NULL, NULL, NULL, 'NEW', 126),

('ev_b0664', 'day_2026-01-29', '2026-01-29T13:45:00Z', '2026-01-29T14:15:00Z',
 'Coffee and Slack', 'Office', '["app_usage", "message"]',
 0, 0, 0, 0, 0,
 'Morning coffee and reviewing overnight Slack threads.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 68),

('ev_b0665', 'day_2026-01-29', '2026-01-29T14:15:00Z', '2026-01-29T15:00:00Z',
 'Design standup', 'Office', '["calendar", "message", "transcription"]',
 0, 0, 0, 0, 0,
 'Thursday standup, discussed form validation edge cases for the onboarding flow with David.', '["meeting", "standup", "design", "onboarding", "form-validation"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 74),

('ev_b0666', 'day_2026-01-29', '2026-01-29T15:00:00Z', '2026-01-29T17:30:00Z',
 'Focused design work', 'Office', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Morning design session on onboarding error handling screens.', '["design", "figma", "focus", "deep-work", "onboarding", "form-validation"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 67),

('ev_b0667', 'day_2026-01-29', '2026-01-29T17:30:00Z', '2026-01-29T18:15:00Z',
 'Lunch', 'Office', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Grabbed a sandwich from the cafe downstairs.', '["food"]', '["place_demo_office"]',
 NULL, NULL, NULL, 'NEW', 77),

('ev_b0668', 'day_2026-01-29', '2026-01-29T18:30:00Z', '2026-01-29T22:30:00Z',
 'WFH afternoon', 'Home', '["app_usage", "message"]',
 0, 0, 0, 0, 0,
 'Worked from home in the afternoon, polishing the onboarding prototype and writing spec notes.', '["work", "design", "figma", "onboarding", "focus"]', '["place_demo_home", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 65),

('ev_b0669', 'day_2026-01-29', '2026-01-29T23:30:00Z', '2026-01-30T01:00:00Z',
 'Evening run', 'Mueller Trails', '["steps", "location_visit"]',
 0, 0, 0, 0, 0,
 'Evening run on Mueller trails, 3 miles at easy pace.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
 NULL, NULL, NULL, 'NEW', 68),

('ev_b0670', 'day_2026-01-29', '2026-01-30T01:00:00Z', '2026-01-30T04:00:00Z',
 'Evening wind-down', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Showered after the run, read and browsed the internet before bed.', '["leisure", "browsing", "reflection"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 62);

-- =============================================================================
-- Friday, January 30, 2026 (10 events — no game night, Mom call Saturday instead)
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES
('ev_b0671', 'day_2026-01-30', '2026-01-30T06:00:00Z', '2026-01-30T12:30:00Z',
 'Sleep', 'Home', '["sleep"]',
 0, 0, 1, 0, 0,
 'Overnight sleep, about 6.5 hours.', '["sleep"]', '[]',
 NULL, NULL, NULL, 'NEW', 62),

('ev_b0672', 'day_2026-01-30', '2026-01-30T12:30:00Z', '2026-01-30T13:15:00Z',
 'Morning routine', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Coffee and scrolled through messages, quiet Friday morning.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 64),

('ev_b0673', 'day_2026-01-30', '2026-01-30T13:15:00Z', '2026-01-30T13:45:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 0, 1, 0, 0, 0,
 'Bike commute to the office.', '["commute", "cycling", "podcast"]', '[]',
 NULL, NULL, NULL, 'NEW', 135),

('ev_b0674', 'day_2026-01-30', '2026-01-30T13:45:00Z', '2026-01-30T14:15:00Z',
 'Coffee and Slack', 'Office', '["app_usage", "message"]',
 0, 0, 0, 0, 0,
 'Friday coffee and Slack catch-up.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 72),

('ev_b0675', 'day_2026-01-30', '2026-01-30T14:15:00Z', '2026-01-30T15:00:00Z',
 'Design standup', 'Office', '["calendar", "message", "transcription"]',
 0, 0, 0, 0, 0,
 'Friday standup with Maya and David, wrapped up the week on onboarding progress.', '["meeting", "standup", "design", "onboarding"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 76),

('ev_b0676', 'day_2026-01-30', '2026-01-30T15:00:00Z', '2026-01-30T17:30:00Z',
 'Focused design work', 'Office', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Worked on polishing the onboarding email verification screen.', '["design", "figma", "focus", "onboarding", "form-validation"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 63),

('ev_b0677', 'day_2026-01-30', '2026-01-30T17:30:00Z', '2026-01-30T18:30:00Z',
 'Lunch', 'Office', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Lunch at desk, leftover soup from home.', '["food"]', '["place_demo_office"]',
 NULL, NULL, NULL, 'NEW', 71),

('ev_b0678', 'day_2026-01-30', '2026-01-30T18:30:00Z', '2026-01-30T21:30:00Z',
 'Afternoon work', 'Office', '["app_usage", "message"]',
 0, 0, 0, 0, 0,
 'Shorter Friday afternoon, finished up design documentation for the onboarding handoff.', '["work", "design", "onboarding", "code-review"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 65),

('ev_b0679', 'day_2026-01-30', '2026-01-30T21:30:00Z', '2026-01-30T22:00:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 0, 1, 0, 0, 0,
 'Bike commute home, left a bit early on Friday.', '["commute", "cycling"]', '[]',
 NULL, NULL, NULL, 'NEW', 131),

('ev_b0680', 'day_2026-01-30', '2026-01-30T22:00:00Z', '2026-01-31T04:30:00Z',
 'Quiet evening', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Quiet Friday night at home, made tacos and watched a movie.', '["food", "leisure"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 66);

-- =============================================================================
-- Saturday, January 31, 2026 (7 events — Lady Bird Lake, Mom call)
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES
('ev_b0681', 'day_2026-01-31', '2026-01-31T06:00:00Z', '2026-01-31T13:30:00Z',
 'Sleep', 'Home', '["sleep"]',
 0, 0, 1, 0, 0,
 'Slept in on Saturday, about 7.5 hours.', '["sleep"]', '[]',
 NULL, NULL, NULL, 'NEW', 60),

('ev_b0682', 'day_2026-01-31', '2026-01-31T13:30:00Z', '2026-01-31T15:00:00Z',
 'Slow morning', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Slow Saturday morning, made pancakes and read the news.', '["routine", "morning", "coffee", "food"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 66),

('ev_b0683', 'day_2026-01-31', '2026-01-31T15:00:00Z', '2026-01-31T17:00:00Z',
 'Walk at Lady Bird Lake', 'Lady Bird Lake', '["steps", "location_visit"]',
 0, 0, 0, 0, 0,
 'Long walk around Lady Bird Lake, clear and cool morning.', '["exercise", "outdoors"]', '["place_demo_ladybird"]',
 NULL, NULL, NULL, 'NEW', 98),

('ev_b0684', 'day_2026-01-31', '2026-01-31T17:00:00Z', '2026-01-31T19:00:00Z',
 'Errands', NULL, '["location_visit"]',
 0, 0, 0, 0, 0,
 'Ran errands, grocery store and Target.', '["routine", "driving"]', '[]',
 NULL, NULL, NULL, 'NEW', 79),

('ev_b0685', 'day_2026-01-31', '2026-01-31T19:00:00Z', '2026-01-31T20:00:00Z',
 'Phone call with Mom', 'Home', '["transcription"]',
 0, 0, 0, 0, 0,
 'Weekly phone call with Mom, caught up on family news and Dad''s golf trip.', '["family", "phone-call"]', '["person_demo_mom", "place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 65),

('ev_b0686', 'day_2026-01-31', '2026-01-31T23:00:00Z', '2026-02-01T02:00:00Z',
 'Dinner and movie', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Cooked a stew and watched a thriller at home.', '["food", "leisure"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 66),

('ev_b0687', 'day_2026-01-31', '2026-02-01T02:00:00Z', '2026-02-01T04:00:00Z',
 'Wind down', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Browsed Pinterest for apartment decor ideas before bed.', '["leisure", "browsing", "reflection"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 58);

-- =============================================================================
-- Sunday, February 1, 2026 (7 events — slow day)
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES
('ev_b0688', 'day_2026-02-01', '2026-02-01T06:00:00Z', '2026-02-01T14:00:00Z',
 'Sleep', 'Home', '["sleep"]',
 0, 0, 1, 0, 0,
 'Long Sunday sleep, about 8 hours.', '["sleep"]', '[]',
 NULL, NULL, NULL, 'NEW', 61),

('ev_b0689', 'day_2026-02-01', '2026-02-01T14:00:00Z', '2026-02-01T15:30:00Z',
 'Slow morning', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Lazy Sunday morning, made coffee and journaled.', '["routine", "morning", "coffee", "reflection"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 68),

('ev_b0690', 'day_2026-02-01', '2026-02-01T15:30:00Z', '2026-02-01T17:00:00Z',
 'Mueller trails run', 'Mueller Trails', '["steps", "location_visit"]',
 0, 0, 0, 0, 0,
 'Sunday morning run on Mueller trails, 4 miles.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
 NULL, NULL, NULL, 'NEW', 150),

('ev_b0691', 'day_2026-02-01', '2026-02-01T17:00:00Z', '2026-02-01T19:00:00Z',
 'Reading and relaxing', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Spent the afternoon reading and catching up on design newsletters.', '["leisure", "browsing", "reflection"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 59),

('ev_b0692', 'day_2026-02-01', '2026-02-01T19:00:00Z', '2026-02-01T21:00:00Z',
 'Meal prep', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Meal prepped for the week — chicken and rice bowls.', '["food", "routine"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 71),

('ev_b0693', 'day_2026-02-01', '2026-02-01T21:00:00Z', '2026-02-02T01:00:00Z',
 'Evening at home', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Watched a design talk on YouTube and texted with Maya about Monday plans.', '["leisure", "messaging", "browsing"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 63),

('ev_b0694', 'day_2026-02-01', '2026-02-02T01:00:00Z', '2026-02-02T04:00:00Z',
 'Wind down', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Read in bed for a while before falling asleep.', '["leisure", "reflection"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 59);

-- =============================================================================
-- Monday, February 2, 2026 (10 events)
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES
('ev_b0695', 'day_2026-02-02', '2026-02-02T06:00:00Z', '2026-02-02T12:30:00Z',
 'Sleep', 'Home', '["sleep"]',
 0, 0, 1, 0, 0,
 'Overnight sleep, about 6.5 hours.', '["sleep"]', '[]',
 NULL, NULL, NULL, 'NEW', 62),

('ev_b0696', 'day_2026-02-02', '2026-02-02T12:30:00Z', '2026-02-02T13:15:00Z',
 'Morning routine', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Morning coffee and checking Slack and email.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 64),

('ev_b0697', 'day_2026-02-02', '2026-02-02T13:15:00Z', '2026-02-02T13:45:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 0, 1, 0, 0, 0,
 'Bike commute to the office on a chilly Monday morning.', '["commute", "cycling", "podcast"]', '[]',
 NULL, NULL, NULL, 'NEW', 123),

('ev_b0698', 'day_2026-02-02', '2026-02-02T13:45:00Z', '2026-02-02T14:15:00Z',
 'Coffee and Slack', 'Office', '["app_usage", "message"]',
 0, 0, 0, 0, 0,
 'Office coffee and clearing out weekend Slack messages.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 67),

('ev_b0699', 'day_2026-02-02', '2026-02-02T14:15:00Z', '2026-02-02T15:00:00Z',
 'Design standup', 'Office', '["calendar", "message", "transcription"]',
 0, 0, 0, 0, 0,
 'Monday standup, kicked off the week with onboarding navigation redesign status update.', '["meeting", "standup", "design", "onboarding", "navigation"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 74),

('ev_b0700', 'day_2026-02-02', '2026-02-02T15:00:00Z', '2026-02-02T17:30:00Z',
 'Focused design work', 'Office', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Deep focus on the onboarding navigation prototype, building out the sidebar flow.', '["design", "figma", "focus", "deep-work", "onboarding", "navigation", "sidebar"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 65),

('ev_b0701', 'day_2026-02-02', '2026-02-02T17:30:00Z', '2026-02-02T18:30:00Z',
 'Lunch', 'Office', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Ate the meal-prepped chicken bowl at desk.', '["food"]', '["place_demo_office"]',
 NULL, NULL, NULL, 'NEW', 73),

('ev_b0702', 'day_2026-02-02', '2026-02-02T18:30:00Z', '2026-02-02T22:00:00Z',
 'Afternoon work', 'Office', '["app_usage", "message"]',
 0, 0, 0, 0, 0,
 'Finished the onboarding navigation prototype and shared with the team for async review.', '["work", "design", "onboarding", "navigation", "code-review"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 65),

('ev_b0703', 'day_2026-02-02', '2026-02-02T22:00:00Z', '2026-02-02T22:30:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 0, 1, 0, 0, 0,
 'Bike commute home.', '["commute", "cycling"]', '[]',
 NULL, NULL, NULL, 'NEW', 124),

('ev_b0704', 'day_2026-02-02', '2026-02-02T22:30:00Z', '2026-02-03T04:00:00Z',
 'Evening at home', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Heated up leftovers and spent the evening reading.', '["food", "leisure", "reflection"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 68);

-- =============================================================================
-- Tuesday, February 3, 2026 (10 events — run in evening)
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES
('ev_b0705', 'day_2026-02-03', '2026-02-03T06:00:00Z', '2026-02-03T12:30:00Z',
 'Sleep', 'Home', '["sleep"]',
 0, 0, 1, 0, 0,
 'Overnight sleep, about 6.5 hours.', '["sleep"]', '[]',
 NULL, NULL, NULL, 'NEW', 56),

('ev_b0706', 'day_2026-02-03', '2026-02-03T12:30:00Z', '2026-02-03T13:15:00Z',
 'Morning routine', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Coffee and caught up on texts.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 63),

('ev_b0707', 'day_2026-02-03', '2026-02-03T13:15:00Z', '2026-02-03T13:45:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 0, 1, 0, 0, 0,
 'Bike commute to the office.', '["commute", "cycling"]', '[]',
 NULL, NULL, NULL, 'NEW', 130),

('ev_b0708', 'day_2026-02-03', '2026-02-03T13:45:00Z', '2026-02-03T14:15:00Z',
 'Coffee and Slack', 'Office', '["app_usage", "message"]',
 0, 0, 0, 0, 0,
 'Grabbed coffee and reviewed feedback on the nav prototype from yesterday.', '["messaging", "work", "coffee", "code-review", "navigation"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 65),

('ev_b0709', 'day_2026-02-03', '2026-02-03T14:15:00Z', '2026-02-03T15:00:00Z',
 'Design standup', 'Office', '["calendar", "message", "transcription"]',
 0, 0, 0, 0, 0,
 'Tuesday standup with Maya and David, reviewed async nav prototype feedback.', '["meeting", "standup", "design", "onboarding", "navigation"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 71),

('ev_b0710', 'day_2026-02-03', '2026-02-03T15:00:00Z', '2026-02-03T16:00:00Z',
 'Design review', 'Office', '["calendar", "app_usage"]',
 0, 0, 0, 0, 0,
 'Design review with David on the onboarding form validation iteration.', '["meeting", "design-review", "design", "onboarding", "form-validation"]', '["person_demo_david", "place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 78),

('ev_b0711', 'day_2026-02-03', '2026-02-03T16:00:00Z', '2026-02-03T17:30:00Z',
 'Focused design work', 'Office', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Applied review feedback to the onboarding error states.', '["design", "figma", "focus", "onboarding", "form-validation"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 68),

('ev_b0712', 'day_2026-02-03', '2026-02-03T17:30:00Z', '2026-02-03T18:30:00Z',
 'Lunch', 'Office', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Meal-prepped chicken bowl for lunch at desk.', '["food"]', '["place_demo_office"]',
 NULL, NULL, NULL, 'NEW', 73),

('ev_b0713', 'day_2026-02-03', '2026-02-03T18:30:00Z', '2026-02-03T22:00:00Z',
 'Afternoon work', 'Office', '["app_usage", "message"]',
 0, 0, 0, 0, 0,
 'Afternoon of onboarding flow refinements and Slack conversations with engineering.', '["work", "design", "onboarding", "messaging", "code-review"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 66),

('ev_b0714', 'day_2026-02-03', '2026-02-03T23:30:00Z', '2026-02-04T01:00:00Z',
 'Evening run', 'Mueller Trails', '["steps", "location_visit"]',
 0, 0, 0, 0, 0,
 'Evening 3.5-mile run on Mueller trails, felt strong.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
 NULL, NULL, NULL, 'NEW', 66);

-- =============================================================================
-- Wednesday, February 4, 2026 (10 events — Ramen with Maya)
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES
('ev_b0715', 'day_2026-02-04', '2026-02-04T06:00:00Z', '2026-02-04T12:30:00Z',
 'Sleep', 'Home', '["sleep"]',
 0, 0, 1, 0, 0,
 'Overnight sleep, about 6.5 hours.', '["sleep"]', '[]',
 NULL, NULL, NULL, 'NEW', 62),

('ev_b0716', 'day_2026-02-04', '2026-02-04T12:30:00Z', '2026-02-04T13:15:00Z',
 'Morning routine', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Morning coffee and checking Slack.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 66),

('ev_b0717', 'day_2026-02-04', '2026-02-04T13:15:00Z', '2026-02-04T13:45:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 0, 1, 0, 0, 0,
 'Bike commute to the office.', '["commute", "cycling", "podcast"]', '[]',
 NULL, NULL, NULL, 'NEW', 116),

('ev_b0718', 'day_2026-02-04', '2026-02-04T13:45:00Z', '2026-02-04T14:15:00Z',
 'Coffee and Slack', 'Office', '["app_usage", "message"]',
 0, 0, 0, 0, 0,
 'Morning coffee and caught up on Slack.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 71),

('ev_b0719', 'day_2026-02-04', '2026-02-04T14:15:00Z', '2026-02-04T15:00:00Z',
 'Design standup', 'Office', '["calendar", "message", "transcription"]',
 0, 0, 0, 0, 0,
 'Wednesday standup, discussed onboarding funnel conversion improvements with Maya and David.', '["meeting", "standup", "design", "onboarding"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 70),

('ev_b0720', 'day_2026-02-04', '2026-02-04T15:00:00Z', '2026-02-04T17:30:00Z',
 'Focused design work', 'Office', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Built out the onboarding success confirmation screens in Figma.', '["design", "figma", "focus", "deep-work", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 63),

('ev_b0721', 'day_2026-02-04', '2026-02-04T17:30:00Z', '2026-02-04T18:30:00Z',
 'Lunch at Ramen Tatsu-ya', 'Ramen Tatsu-ya', '["location_visit"]',
 0, 0, 0, 0, 0,
 'Wednesday ramen lunch with Maya, talked about upcoming user research sessions.', '["food", "social", "ramen"]', '["person_demo_maya", "place_demo_ramen"]',
 NULL, NULL, NULL, 'NEW', 76),

('ev_b0722', 'day_2026-02-04', '2026-02-04T18:30:00Z', '2026-02-04T22:30:00Z',
 'Afternoon work', 'Office', '["app_usage", "message"]',
 0, 0, 0, 0, 0,
 'Afternoon session refining the onboarding page layout with micro-interactions.', '["work", "design", "figma", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 64),

('ev_b0723', 'day_2026-02-04', '2026-02-04T22:30:00Z', '2026-02-04T23:00:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 0, 1, 0, 0, 0,
 'Bike commute home.', '["commute", "cycling"]', '[]',
 NULL, NULL, NULL, 'NEW', 122),

('ev_b0724', 'day_2026-02-04', '2026-02-04T23:00:00Z', '2026-02-05T04:00:00Z',
 'Evening at home', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Made a salad for dinner and watched a couple episodes of a show.', '["food", "leisure"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 64);

-- =============================================================================
-- Thursday, February 5, 2026 (10 events — WFH afternoon, run)
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES
('ev_b0725', 'day_2026-02-05', '2026-02-05T06:00:00Z', '2026-02-05T12:30:00Z',
 'Sleep', 'Home', '["sleep"]',
 0, 0, 1, 0, 0,
 'Overnight sleep, about 6.5 hours.', '["sleep"]', '[]',
 NULL, NULL, NULL, 'NEW', 62),

('ev_b0726', 'day_2026-02-05', '2026-02-05T12:30:00Z', '2026-02-05T13:15:00Z',
 'Morning routine', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Coffee and quick Slack check.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 65),

('ev_b0727', 'day_2026-02-05', '2026-02-05T13:15:00Z', '2026-02-05T13:45:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 0, 1, 0, 0, 0,
 'Bike commute to the office.', '["commute", "cycling", "podcast"]', '[]',
 NULL, NULL, NULL, 'NEW', 123),

('ev_b0728', 'day_2026-02-05', '2026-02-05T13:45:00Z', '2026-02-05T14:15:00Z',
 'Coffee and Slack', 'Office', '["app_usage", "message"]',
 0, 0, 0, 0, 0,
 'Morning coffee and Slack threads about the onboarding eng handoff.', '["messaging", "work", "coffee", "onboarding", "code-review"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 72),

('ev_b0729', 'day_2026-02-05', '2026-02-05T14:15:00Z', '2026-02-05T15:00:00Z',
 'Design standup', 'Office', '["calendar", "message", "transcription"]',
 0, 0, 0, 0, 0,
 'Thursday standup, David raised edge cases in the onboarding form validation.', '["meeting", "standup", "design", "onboarding", "form-validation"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 72),

('ev_b0730', 'day_2026-02-05', '2026-02-05T15:00:00Z', '2026-02-05T17:30:00Z',
 'Focused design work', 'Office', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Worked through David''s edge case list for the form validation screens.', '["design", "figma", "focus", "deep-work", "onboarding", "form-validation"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 63),

('ev_b0731', 'day_2026-02-05', '2026-02-05T17:30:00Z', '2026-02-05T18:15:00Z',
 'Lunch', 'Office', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Quick sandwich at the office.', '["food"]', '["place_demo_office"]',
 NULL, NULL, NULL, 'NEW', 74),

('ev_b0732', 'day_2026-02-05', '2026-02-05T18:30:00Z', '2026-02-05T22:30:00Z',
 'WFH afternoon', 'Home', '["app_usage", "message"]',
 0, 0, 0, 0, 0,
 'Worked from home in the afternoon, finishing onboarding edge case designs and writing Jira tickets.', '["work", "design", "onboarding", "form-validation", "focus"]', '["place_demo_home", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 65),

('ev_b0733', 'day_2026-02-05', '2026-02-05T23:30:00Z', '2026-02-06T01:00:00Z',
 'Evening run', 'Mueller Trails', '["steps", "location_visit"]',
 0, 0, 0, 0, 0,
 'Ran 3 miles on Mueller trails at sunset.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
 NULL, NULL, NULL, 'NEW', 60),

('ev_b0734', 'day_2026-02-05', '2026-02-06T01:00:00Z', '2026-02-06T04:00:00Z',
 'Evening wind-down', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Showered, ate leftovers, and read before bed.', '["leisure", "food", "reflection"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 68);

-- =============================================================================
-- Friday, February 6, 2026 (10 events — Mom call, Game night at Jess's)
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES
('ev_b0735', 'day_2026-02-06', '2026-02-06T06:00:00Z', '2026-02-06T12:30:00Z',
 'Sleep', 'Home', '["sleep"]',
 0, 0, 1, 0, 0,
 'Overnight sleep, about 6.5 hours.', '["sleep"]', '[]',
 NULL, NULL, NULL, 'NEW', 55),

('ev_b0736', 'day_2026-02-06', '2026-02-06T12:30:00Z', '2026-02-06T13:15:00Z',
 'Morning routine', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Morning coffee and texts with Jess confirming game night tonight.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 68),

('ev_b0737', 'day_2026-02-06', '2026-02-06T13:15:00Z', '2026-02-06T13:45:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 0, 1, 0, 0, 0,
 'Bike commute to the office.', '["commute", "cycling"]', '[]',
 NULL, NULL, NULL, 'NEW', 120),

('ev_b0738', 'day_2026-02-06', '2026-02-06T13:45:00Z', '2026-02-06T14:15:00Z',
 'Coffee and Slack', 'Office', '["app_usage", "message"]',
 0, 0, 0, 0, 0,
 'Friday morning coffee and Slack.', '["messaging", "work", "coffee"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 65),

('ev_b0739', 'day_2026-02-06', '2026-02-06T14:15:00Z', '2026-02-06T15:00:00Z',
 'Design standup', 'Office', '["calendar", "message", "transcription"]',
 0, 0, 0, 0, 0,
 'Friday standup, wrapped up the week on onboarding with Maya and David.', '["meeting", "standup", "design", "onboarding"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 70),

('ev_b0740', 'day_2026-02-06', '2026-02-06T15:00:00Z', '2026-02-06T17:30:00Z',
 'Focused design work', 'Office', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Morning focus on the onboarding progress indicator component.', '["design", "figma", "focus", "deep-work", "onboarding"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 66),

('ev_b0741', 'day_2026-02-06', '2026-02-06T17:30:00Z', '2026-02-06T18:30:00Z',
 'Lunch', 'Office', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Lunch at desk, salad from the cafe.', '["food"]', '["place_demo_office"]',
 NULL, NULL, NULL, 'NEW', 77),

('ev_b0742', 'day_2026-02-06', '2026-02-06T22:00:00Z', '2026-02-06T23:00:00Z',
 'Phone call with Mom', 'Home', '["transcription"]',
 0, 0, 0, 0, 0,
 'Weekly call with Mom, talked about her book club and weekend plans.', '["family", "phone-call"]', '["person_demo_mom", "place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 67),

('ev_b0743', 'day_2026-02-06', '2026-02-07T00:00:00Z', '2026-02-07T00:30:00Z',
 'Drive to Jess''s', NULL, '["location_visit"]',
 0, 1, 0, 0, 0,
 'Drove to Jess''s place on South Lamar for game night.', '["commute", "driving"]', '[]',
 NULL, NULL, NULL, 'NEW', 68),

('ev_b0744', 'day_2026-02-06', '2026-02-07T00:30:00Z', '2026-02-07T05:00:00Z',
 'Game night', 'Jess''s Place', '["location_visit"]',
 0, 0, 0, 0, 0,
 'Game night at Jess''s with Jess and Priya, played Catan and Codenames.', '["social", "games"]', '["person_demo_jess", "person_demo_priya", "place_demo_jess"]',
 NULL, NULL, NULL, 'NEW', 69);

-- =============================================================================
-- Saturday, February 7, 2026 (7 events — Mom call already done Fri, quiet day)
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES
('ev_b0745', 'day_2026-02-07', '2026-02-07T06:00:00Z', '2026-02-07T14:00:00Z',
 'Sleep', 'Home', '["sleep"]',
 0, 0, 1, 0, 0,
 'Slept in after game night, about 8 hours.', '["sleep"]', '[]',
 NULL, NULL, NULL, 'NEW', 57),

('ev_b0746', 'day_2026-02-07', '2026-02-07T14:00:00Z', '2026-02-07T15:30:00Z',
 'Slow morning', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Slow Saturday morning, coffee and catching up on news.', '["routine", "morning", "coffee", "browsing"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 63),

('ev_b0747', 'day_2026-02-07', '2026-02-07T15:30:00Z', '2026-02-07T17:00:00Z',
 'Errands', NULL, '["location_visit"]',
 0, 0, 0, 0, 0,
 'Grocery run and picked up a new book at BookPeople.', '["routine", "driving"]', '[]',
 NULL, NULL, NULL, 'NEW', 81),

('ev_b0748', 'day_2026-02-07', '2026-02-07T17:00:00Z', '2026-02-07T19:00:00Z',
 'Mueller trails run', 'Mueller Trails', '["steps", "location_visit"]',
 0, 0, 0, 0, 0,
 'Saturday afternoon run on Mueller trails, 4 miles.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
 NULL, NULL, NULL, 'NEW', 146),

('ev_b0749', 'day_2026-02-07', '2026-02-07T19:00:00Z', '2026-02-07T21:00:00Z',
 'Afternoon reading', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Read the new book for a couple hours on the couch.', '["leisure", "reflection"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 61),

('ev_b0750', 'day_2026-02-07', '2026-02-07T23:00:00Z', '2026-02-08T02:00:00Z',
 'Dinner and movie', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Made curry for dinner and watched a movie at home.', '["food", "leisure"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 71),

('ev_b0751', 'day_2026-02-07', '2026-02-08T02:00:00Z', '2026-02-08T04:00:00Z',
 'Wind down', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Browsed the internet and texted with Jess about last night''s game.', '["leisure", "messaging", "browsing"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 58);

-- =============================================================================
-- Sunday, February 8, 2026 (7 events — Lady Bird Lake walk, relaxing)
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES
('ev_b0752', 'day_2026-02-08', '2026-02-08T06:00:00Z', '2026-02-08T13:30:00Z',
 'Sleep', 'Home', '["sleep"]',
 0, 0, 1, 0, 0,
 'Overnight sleep, about 7.5 hours.', '["sleep"]', '[]',
 NULL, NULL, NULL, 'NEW', 58),

('ev_b0753', 'day_2026-02-08', '2026-02-08T13:30:00Z', '2026-02-08T15:00:00Z',
 'Slow morning', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Sunday morning, made eggs and read the new book.', '["routine", "morning", "coffee", "food"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 67),

('ev_b0754', 'day_2026-02-08', '2026-02-08T15:00:00Z', '2026-02-08T17:00:00Z',
 'Walk at Lady Bird Lake', 'Lady Bird Lake', '["steps", "location_visit"]',
 0, 0, 0, 0, 0,
 'Long walk at Lady Bird Lake, sunny and mild afternoon.', '["exercise", "outdoors"]', '["place_demo_ladybird"]',
 NULL, NULL, NULL, 'NEW', 86),

('ev_b0755', 'day_2026-02-08', '2026-02-08T17:00:00Z', '2026-02-08T19:00:00Z',
 'Reading', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Continued the new book at home with tea.', '["leisure", "reflection"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 59),

('ev_b0756', 'day_2026-02-08', '2026-02-08T19:00:00Z', '2026-02-08T21:00:00Z',
 'Meal prep', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Prepped lunches for the week, roasted vegetables and grain bowls.', '["food", "routine"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 76),

('ev_b0757', 'day_2026-02-08', '2026-02-08T21:00:00Z', '2026-02-09T01:00:00Z',
 'Evening at home', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Watched a couple episodes of a show and planned the work week ahead.', '["leisure", "reflection"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 68),

('ev_b0758', 'day_2026-02-08', '2026-02-09T01:00:00Z', '2026-02-09T04:00:00Z',
 'Wind down', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Browsed the web and headed to bed early.', '["leisure", "browsing"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 60);

-- =============================================================================
-- Monday, February 9, 2026 (10 events)
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES
('ev_b0759', 'day_2026-02-09', '2026-02-09T06:00:00Z', '2026-02-09T12:30:00Z',
 'Sleep', 'Home', '["sleep"]',
 0, 0, 1, 0, 0,
 'Overnight sleep, about 6.5 hours.', '["sleep"]', '[]',
 NULL, NULL, NULL, 'NEW', 59),

('ev_b0760', 'day_2026-02-09', '2026-02-09T12:30:00Z', '2026-02-09T13:15:00Z',
 'Morning routine', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Coffee and checking Slack for Monday updates.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 64),

('ev_b0761', 'day_2026-02-09', '2026-02-09T13:15:00Z', '2026-02-09T13:45:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 0, 1, 0, 0, 0,
 'Bike commute to the office.', '["commute", "cycling", "podcast"]', '[]',
 NULL, NULL, NULL, 'NEW', 131),

('ev_b0762', 'day_2026-02-09', '2026-02-09T13:45:00Z', '2026-02-09T14:15:00Z',
 'Coffee and Slack', 'Office', '["app_usage", "message"]',
 0, 0, 0, 0, 0,
 'Grabbed coffee and caught up on engineering threads about onboarding implementation.', '["messaging", "work", "coffee", "onboarding", "code-review"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 70),

('ev_b0763', 'day_2026-02-09', '2026-02-09T14:15:00Z', '2026-02-09T15:00:00Z',
 'Design standup', 'Office', '["calendar", "message", "transcription"]',
 0, 0, 0, 0, 0,
 'Monday standup with Maya and David, planning the onboarding user research sessions for this week.', '["meeting", "standup", "design", "onboarding", "ux-research"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 73),

('ev_b0764', 'day_2026-02-09', '2026-02-09T15:00:00Z', '2026-02-09T17:30:00Z',
 'Focused design work', 'Office', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Prepared onboarding user research discussion guide and prototype walkthrough.', '["design", "ux-research", "usability-testing", "onboarding", "focus"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 64),

('ev_b0765', 'day_2026-02-09', '2026-02-09T17:30:00Z', '2026-02-09T18:30:00Z',
 'Lunch', 'Office', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Ate the meal-prepped grain bowl at desk.', '["food"]', '["place_demo_office"]',
 NULL, NULL, NULL, 'NEW', 76),

('ev_b0766', 'day_2026-02-09', '2026-02-09T18:30:00Z', '2026-02-09T22:30:00Z',
 'Afternoon work', 'Office', '["app_usage", "message"]',
 0, 0, 0, 0, 0,
 'Continued onboarding research prep and coordinated participant scheduling with Maya.', '["work", "ux-research", "usability-testing", "onboarding"]', '["person_demo_maya", "place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 66),

('ev_b0767', 'day_2026-02-09', '2026-02-09T22:30:00Z', '2026-02-09T23:00:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 0, 1, 0, 0, 0,
 'Bike commute home.', '["commute", "cycling"]', '[]',
 NULL, NULL, NULL, 'NEW', 131),

('ev_b0768', 'day_2026-02-09', '2026-02-09T23:00:00Z', '2026-02-10T04:00:00Z',
 'Dinner and relaxing', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Heated up soup for dinner and watched a documentary about architecture.', '["food", "leisure"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 69);

-- =============================================================================
-- Tuesday, February 10, 2026 (10 events — run in evening)
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES
('ev_b0769', 'day_2026-02-10', '2026-02-10T06:00:00Z', '2026-02-10T12:30:00Z',
 'Sleep', 'Home', '["sleep"]',
 0, 0, 1, 0, 0,
 'Overnight sleep, about 6.5 hours.', '["sleep"]', '[]',
 NULL, NULL, NULL, 'NEW', 62),

('ev_b0770', 'day_2026-02-10', '2026-02-10T12:30:00Z', '2026-02-10T13:15:00Z',
 'Morning routine', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Morning coffee and checking messages.', '["routine", "morning", "coffee", "messaging"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 65),

('ev_b0771', 'day_2026-02-10', '2026-02-10T13:15:00Z', '2026-02-10T13:45:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 0, 1, 0, 0, 0,
 'Bike commute to the office.', '["commute", "cycling"]', '[]',
 NULL, NULL, NULL, 'NEW', 134),

('ev_b0772', 'day_2026-02-10', '2026-02-10T13:45:00Z', '2026-02-10T14:15:00Z',
 'Coffee and Slack', 'Office', '["app_usage", "message"]',
 0, 0, 0, 0, 0,
 'Coffee and Slack, reviewed participant confirmations for user research.', '["messaging", "work", "coffee", "ux-research"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 66),

('ev_b0773', 'day_2026-02-10', '2026-02-10T14:15:00Z', '2026-02-10T15:00:00Z',
 'Design standup', 'Office', '["calendar", "message", "transcription"]',
 0, 0, 0, 0, 0,
 'Tuesday standup with Maya and David, finalized the onboarding research plan.', '["meeting", "standup", "design", "onboarding", "ux-research"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 70),

('ev_b0774', 'day_2026-02-10', '2026-02-10T15:00:00Z', '2026-02-10T16:00:00Z',
 'Design review', 'Office', '["calendar", "app_usage"]',
 0, 0, 0, 0, 0,
 'Design review with David on the final onboarding flow before user testing.', '["meeting", "design-review", "design", "onboarding", "usability-testing"]', '["person_demo_david", "place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 75),

('ev_b0775', 'day_2026-02-10', '2026-02-10T16:00:00Z', '2026-02-10T17:30:00Z',
 'Focused design work', 'Office', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Last round of polish on the onboarding prototype before research sessions.', '["design", "figma", "focus", "onboarding", "usability-testing"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 66),

('ev_b0776', 'day_2026-02-10', '2026-02-10T17:30:00Z', '2026-02-10T18:30:00Z',
 'Lunch', 'Office', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Grain bowl at desk.', '["food"]', '["place_demo_office"]',
 NULL, NULL, NULL, 'NEW', 71),

('ev_b0777', 'day_2026-02-10', '2026-02-10T18:30:00Z', '2026-02-10T22:00:00Z',
 'Afternoon work', 'Office', '["app_usage", "message"]',
 0, 0, 0, 0, 0,
 'Afternoon spent writing the user research session scripts and sharing with the team.', '["work", "ux-research", "usability-testing", "onboarding", "recording"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 65),

('ev_b0778', 'day_2026-02-10', '2026-02-10T23:30:00Z', '2026-02-11T01:00:00Z',
 'Evening run', 'Mueller Trails', '["steps", "location_visit"]',
 0, 0, 0, 0, 0,
 'Evening 3-mile run on Mueller trails.', '["exercise", "running", "cardio", "mueller-trails"]', '["place_demo_mueller_trails"]',
 NULL, NULL, NULL, 'NEW', 68);

-- =============================================================================
-- Wednesday, February 11, 2026 (10 events — Ramen with Maya, LAST DAY)
-- =============================================================================

INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, topic_novelty, entity_novelty, agent_action, avg_hr
) VALUES
('ev_b0779', 'day_2026-02-11', '2026-02-11T06:00:00Z', '2026-02-11T12:30:00Z',
 'Sleep', 'Home', '["sleep"]',
 0, 0, 1, 0, 0,
 'Overnight sleep, about 6.5 hours.', '["sleep"]', '[]',
 NULL, NULL, NULL, 'NEW', 58),

('ev_b0780', 'day_2026-02-11', '2026-02-11T12:30:00Z', '2026-02-11T13:15:00Z',
 'Morning routine', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Coffee and Slack, checking on user research logistics.', '["routine", "morning", "coffee", "messaging", "ux-research"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 67),

('ev_b0781', 'day_2026-02-11', '2026-02-11T13:15:00Z', '2026-02-11T13:45:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 0, 1, 0, 0, 0,
 'Bike commute to the office.', '["commute", "cycling", "podcast"]', '[]',
 NULL, NULL, NULL, 'NEW', 118),

('ev_b0782', 'day_2026-02-11', '2026-02-11T13:45:00Z', '2026-02-11T14:15:00Z',
 'Coffee and Slack', 'Office', '["app_usage", "message"]',
 0, 0, 0, 0, 0,
 'Morning coffee and Slack, reviewed final research session plans.', '["messaging", "work", "coffee", "ux-research"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 67),

('ev_b0783', 'day_2026-02-11', '2026-02-11T14:15:00Z', '2026-02-11T15:00:00Z',
 'Design standup', 'Office', '["calendar", "message", "transcription"]',
 0, 0, 0, 0, 0,
 'Wednesday standup with Maya and David, confirmed the first user research session for Thursday.', '["meeting", "standup", "design", "onboarding", "ux-research", "usability-testing"]', '["person_demo_maya", "person_demo_david", "place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 75),

('ev_b0784', 'day_2026-02-11', '2026-02-11T15:00:00Z', '2026-02-11T17:30:00Z',
 'Focused design work', 'Office', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Final tweaks to the onboarding prototype for the user research walkthrough.', '["design", "figma", "focus", "onboarding", "usability-testing"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 62),

('ev_b0785', 'day_2026-02-11', '2026-02-11T17:30:00Z', '2026-02-11T18:30:00Z',
 'Lunch at Ramen Tatsu-ya', 'Ramen Tatsu-ya', '["location_visit"]',
 0, 0, 0, 0, 0,
 'Wednesday ramen with Maya, talked about being nervous for the first onboarding research session.', '["food", "social", "ramen"]', '["person_demo_maya", "place_demo_ramen"]',
 NULL, NULL, NULL, 'NEW', 73),

('ev_b0786', 'day_2026-02-11', '2026-02-11T18:30:00Z', '2026-02-11T22:30:00Z',
 'Afternoon work', 'Office', '["app_usage", "message"]',
 0, 0, 0, 0, 0,
 'Afternoon preparing research materials and coordinating with the product team.', '["work", "ux-research", "usability-testing", "onboarding", "recording"]', '["place_demo_office", "org_demo_employer"]',
 NULL, NULL, NULL, 'NEW', 69),

('ev_b0787', 'day_2026-02-11', '2026-02-11T22:30:00Z', '2026-02-11T23:00:00Z',
 'Bike commute', NULL, '["location_visit", "steps"]',
 0, 1, 0, 0, 0,
 'Bike commute home.', '["commute", "cycling"]', '[]',
 NULL, NULL, NULL, 'NEW', 119),

('ev_b0788', 'day_2026-02-11', '2026-02-11T23:00:00Z', '2026-02-12T04:00:00Z',
 'Evening at home', 'Home', '["app_usage"]',
 0, 0, 0, 0, 0,
 'Made stir fry for dinner and read before bed, looking forward to the research session tomorrow.', '["food", "leisure", "reflection"]', '["place_demo_home"]',
 NULL, NULL, NULL, 'NEW', 62);
