---
name: bookmark-enrichment
description: "Reads the pages you saved, so you can find them again by what they were about."
metadata:
  invoke: none
  trigger: cron 0 */10 * * * *
  run: bookmark_enrichment
  limit: 1200 seconds
---

