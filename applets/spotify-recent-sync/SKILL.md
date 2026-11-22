---
name: spotify-recent-sync
description: "Adds what you play on Spotify between data downloads."
metadata:
  invoke: none
  trigger: "cron 0 */30 * * * *"
  run: spotify_recent_sync
  source: spotify
  writes: activity_listening
---
