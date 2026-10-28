---
name: plaid-accounts-sync
description: Keeps your bank balances current.
metadata:
  invoke: none
  trigger: cron 0 0 */6 * * *
  run: plaid_accounts_sync
  source: plaid
  writes: financial_account
---

