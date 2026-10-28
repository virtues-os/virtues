---
name: document-extraction
description: Reads the text out of your PDFs and documents so you can search inside them.
metadata:
  invoke: none
  trigger: cron 30 */5 * * * *
  run: document_extraction
  limit: 3600 seconds
---

