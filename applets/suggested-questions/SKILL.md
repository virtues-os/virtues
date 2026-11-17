---
name: suggested-questions
description: "Writes the question a new chat offers you, from what changed in your day."
metadata:
  invoke: none
  trigger: cron 0 5 * * * *
  run: suggested_questions
  limit: $0.25 per day
---

