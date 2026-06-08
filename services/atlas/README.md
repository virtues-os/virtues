# atlas

Identity, billing, and entitlement issuance for Virtues.

Atlas holds the Stripe customer side of the Chinese wall. It runs the Privacy Pass issuer, processes Stripe webhooks, and pushes entitlement updates to `virtues-api` keyed on `activation_handle`. **It never sees a raw bearer. It never receives a call from virtues-api.**

This is the design from [WS-6a](../../docs/entitlement.md).

## Status

Scaffold only (WS-7 implements the real flows).

Currently exposes:

- `GET /health` — liveness probe

## What lives here (per WS-6a)

- Stripe webhook handlers (signup, renewal, cancel, refund, chargeback)
- Privacy Pass issuer endpoint
- Customer + subscription schema (`customers`, `subscriptions`, `customer_handles`, `payment_links`, `issuer_keys`)
- The Mullvad-style `payment_links` TTL sweeper (20-day deletion)
- `POST /internal/abuse` receiver (called by virtues-api's behavioral blocklist)
- DIY self-host signup flow (email + captcha + verification)

## What does NOT live here

- Customer support agent dashboard, refund UI, fraud-threshold tuning, internal metrics — all of that goes in a separate **private** `atlas-admin` repo that reads the same DB. Atlas is the privacy-load-bearing public-OSS surface. Tooling that would tip off abusers stays closed.

## Env

```
VIRTUES_ATLAS_PORT=9100
VIRTUES_API_URL=http://localhost:9002
VIRTUES_API_INTERNAL_SECRET=<shared with virtues-api>
DATABASE_URL=postgres://...
STRIPE_SECRET_KEY=<sk_live_or_test>
STRIPE_WEBHOOK_SECRET=<whsec_...>
```
