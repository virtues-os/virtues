---
name: spotify-import
description: "Brings a Spotify data download into your history. A newer download adds only what is new."
metadata:
  invoke: none
  run: spotify_import
  limit: 900 seconds
  writes: activity_listening
---
