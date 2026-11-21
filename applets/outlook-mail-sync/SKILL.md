---
name: outlook-mail-sync
description: Keeps your Outlook mail on your server.
metadata:
  invoke: none
  trigger: cron 0 */15 * * * *
  run: outlook_mail_sync
  source: outlook
  writes: communication_email
  limit: 1800 seconds
---
