---
name: google-gmail-sync
description: Keeps your Gmail on your server.
metadata:
  invoke: none
  trigger: cron 0 */15 * * * *
  run: google_gmail_sync
  source: google
  writes: communication_email
---

