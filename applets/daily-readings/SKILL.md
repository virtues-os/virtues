---
name: daily-readings
description: "Keeps the next two weeks of readings on your server, so your morning doesn't wait on a website."
metadata:
  invoke: none
  trigger: cron 0 30 4 * * *
  run: daily_readings
  enabled: "false"
---

