---
name: mac-ingest
description: "Takes what your Mac sees: the apps you use, the pages you visit, and your iMessage history."
metadata:
  invoke: none
  trigger: webhook
  run: mac_ingest
  source: mac
  writes: "activity_app_session, activity_web_browsing, communication_message, content_bookmark"
---

