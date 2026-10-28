---
name: instagram-saves-sync
description: Files the posts you save on Instagram with your bookmarks.
metadata:
  invoke: none
  trigger: "cron 0 0 10,22 * * *"
  run: instagram_saves_sync
  source: instagram
  limit: 600 seconds
  writes: content_bookmark
---

