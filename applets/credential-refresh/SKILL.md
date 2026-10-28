---
name: credential-refresh
description: "Keeps your connected accounts signed in, renewing access before it lapses."
metadata:
  invoke: none
  trigger: cron 0 */15 * * * *
  run: credential_refresh
---

