---
name: grok-chats-sync
description: Keeps a copy of your Grok chats with your other AI conversations.
metadata:
  invoke: none
  trigger: "cron 0 0 9,21 * * *"
  run: grok_chats_sync
  source: grok
  writes: content_conversation
---

