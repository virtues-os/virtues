---
name: x-bookmarks-sync
description: Files the posts you bookmark on X with your bookmarks.
metadata:
  invoke: none
  trigger: "cron 0 0 9,21 * * *"
  run: x_bookmarks_sync
  source: x
  writes: content_bookmark
---

