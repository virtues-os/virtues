---
name: google-calendar-sync
description: Keeps your Google Calendar events on your server.
metadata:
  invoke: none
  trigger: cron 0 */15 * * * *
  run: google_calendar_sync
  source: google
  writes: calendar_event
---

