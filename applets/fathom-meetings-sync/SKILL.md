---
name: fathom-meetings-sync
description: Keeps your Fathom meetings on your server.
metadata:
  invoke: none
  trigger: cron 0 */30 * * * *
  run: fathom_meetings_sync
  source: fathom
  writes: communication_transcription
  limit: 1200 seconds
---
