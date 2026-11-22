---
name: discord-import
description: "Brings the messages in a Discord data package into your history. A newer package adds only what is new."
metadata:
  invoke: none
  run: discord_import
  limit: 900 seconds
  writes: communication_message
---
