---
name: weather-sync
description: "Records the weather where you are, hour by hour."
metadata:
  invoke: none
  trigger: cron 0 0 * * * *
  run: weather_sync
---

