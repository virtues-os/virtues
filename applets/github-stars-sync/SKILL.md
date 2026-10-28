---
name: github-stars-sync
description: Files the repositories you star on GitHub with your bookmarks.
metadata:
  invoke: none
  trigger: "cron 0 0 8,20 * * *"
  run: github_stars_sync
  source: github
  writes: content_bookmark
---

