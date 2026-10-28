---
name: notion-pages-sync
description: Keeps your Notion pages and databases on your server.
metadata:
  invoke: none
  trigger: cron 0 */30 * * * *
  run: notion_pages_sync
  source: notion
  writes: content_document
---

