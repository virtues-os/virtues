---
name: plaid-liabilities-sync
description: "Keeps your credit cards, mortgage, and loans current."
metadata:
  invoke: none
  trigger: cron 0 0 */12 * * *
  run: plaid_liabilities_sync
  source: plaid
---

