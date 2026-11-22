---
name: amazon-orders-import
description: "Brings your Amazon order history into your history. A newer download adds only what is new."
metadata:
  invoke: none
  run: amazon_orders_import
  limit: 600 seconds
  writes: financial_purchase
---
