-- 045: Autonomic scoring + Readiness fields
--
-- Adds per-event autonomic state tracking and morning readiness score.
-- See /DAYLINE_AUTONOMIC_DESIGN.md for full design rationale.
--
-- wiki_events gains:
--   avg_hr       — average heart rate during event window (from Apple Watch HR data)
--   hr_z         — HR z-score vs embedding-similar past events
--   hrv_z        — HRV z-score vs embedding-similar past events (when HRV reading available)
--   autonomic_z  — context-gated composite:
--                   physical events (HR > resting+2σ): hr_z only
--                   sedentary events: 0.3*hr_z + 0.7*(-hrv_z)
--                   sleep events: -hrv_z only
--
-- wiki_days gains:
--   readiness_score   — 0-100 morning autonomic readiness (computed from overnight HRV, RHR, sleep)
--   readiness_details — JSON breakdown of readiness components

ALTER TABLE wiki_events ADD COLUMN avg_hr REAL;
ALTER TABLE wiki_events ADD COLUMN hr_z REAL;
ALTER TABLE wiki_events ADD COLUMN hrv_z REAL;
ALTER TABLE wiki_events ADD COLUMN autonomic_z REAL;

ALTER TABLE wiki_days ADD COLUMN readiness_score INTEGER;
ALTER TABLE wiki_days ADD COLUMN readiness_details TEXT;
