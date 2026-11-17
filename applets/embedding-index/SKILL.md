---
name: embedding-index
description: "Keeps search working on meaning, so you can find things you can't name exactly."
metadata:
  invoke: none
  trigger: cron 0 */15 * * * *
  run: embedding_index
  limit: 7500 seconds
---

