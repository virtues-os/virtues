---
name: slack-import
description: "Brings a Slack workspace export into your history. A newer export adds only what is new."
metadata:
  invoke: none
  run: slack_import
  limit: 900 seconds
  writes: communication_message
---
