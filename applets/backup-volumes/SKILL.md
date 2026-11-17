---
name: backup-volumes
description: Copies your whole archive onto every backup drive you have plugged in.
metadata:
  invoke: none
  trigger: cron 0 0 4 * * *
  run: backup_volumes
  limit: 21600 seconds
---

