---
name: granola-meetings-sync
description: Keeps your Granola meetings on your server.
metadata:
  invoke: none
  trigger: cron 0 */30 * * * *
  run: granola_meetings_sync
  source: granola
  writes: communication_transcription
  limit: 1800 seconds
---
