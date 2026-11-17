---
name: plaid-investments-sync
description: Keeps your brokerage holdings current.
metadata:
  invoke: none
  trigger: cron 0 0 */12 * * *
  run: plaid_investments_sync
  source: plaid
---

