---
name: photo-enrichment
description: "Describes your photo favorites, so you can find one by what is in it."
metadata:
  invoke: none
  trigger: cron 0 */10 * * * *
  run: photo_enrichment
  limit: 1200 seconds
---
