---
name: outlook-calendar-sync
description: Keeps your Outlook calendar events on your server.
metadata:
  invoke: none
  trigger: cron 0 */15 * * * *
  run: outlook_calendar_sync
  source: outlook
  writes: calendar_event
---
