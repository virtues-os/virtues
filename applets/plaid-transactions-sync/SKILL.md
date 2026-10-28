---
name: plaid-transactions-sync
description: Brings in new transactions as your bank posts them.
metadata:
  invoke: none
  trigger: cron 0 */30 * * * *
  run: plaid_transactions_sync
  source: plaid
  writes: financial_transaction
---

