---
name: day-summary-eod
description: "Writes the day's entry in your autobiography once the day is done."
metadata:
  invoke: assistant
  trigger: cron 0 0 * * * *
  run: day_summary_eod
---

