---
name: fireflies-meetings-sync
description: Keeps your Fireflies.ai meetings on your server.
metadata:
  invoke: none
  trigger: cron 0 */30 * * * *
  run: fireflies_meetings_sync
  source: fireflies
  writes: communication_transcription
  limit: 1200 seconds
---
