---
name: ios-ingest
description: "Receives everything your iPhone collects - health, location, calendar, contacts, audio, finance, and what you share to it."
metadata:
  invoke: none
  trigger: webhook
  run: ios_ingest
  source: ios
  writes: "calendar_event, communication_transcription, content_bookmark, financial_account, financial_transaction, health_cycle_log, health_heart_rate, health_hrv, health_sleep, health_steps, health_symptom, health_workout, health_wrist_temperature, location_point"
---

