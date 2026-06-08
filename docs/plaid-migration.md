# Plaid Migration Plan

> Status: planned, not yet implemented.
> Owner: WS-6c (follow-on to WS-6b's per-call route migrations).
> Predecessor: WS-6a/b (entitlement design + bearer auth landed).

## Why Plaid is different

Plaid's pricing **isn't per-call.** They bill ~$0.25–$1 per **Item** per month, where one Item = one bank connection. Per-call API costs (transactions/sync, balance, identity) are negligible — the real cost is "this Item exists this month."

That breaks the WS-6b per-call charge model we use for AI/Places/Exa/Unsplash. Charging per-call would either:

- Under-charge (most calls are free; we eat the $1/mo overhead).
- Over-charge (every sync call hits the budget; users get billed multiple times for the same monthly cost).

We need a **per-Item-per-billing-period** model, not a per-call one.

## What needs to change

### 1. New `entitlement_plaid_items` table in virtues-api

```sql
-- One row per Plaid Item (one institution per row).
-- bearer_hash links the Item to its owning entitlement.
CREATE TABLE entitlement_plaid_items (
    plaid_item_id      text PRIMARY KEY,           -- Plaid's stable Item identifier
    bearer_hash        bytea NOT NULL REFERENCES entitlements (bearer_hash) ON DELETE CASCADE,
    institution_name   text,                       -- Display only; for support / UX
    connected_at       timestamptz NOT NULL DEFAULT now(),
    last_seen_at       timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX entitlement_plaid_items_bearer_idx
    ON entitlement_plaid_items (bearer_hash);
```

**Deliberately absent:** no transaction data, no balance data, no institution_id beyond what's needed for support. The Item ID is the only Plaid identifier we keep. Per-Plaid-call data flows through unproxied — we never log it.

### 2. Tier-based item-count limits

Resolve the cap from the bearer's tier (probably injected by Atlas as a column on `entitlements`, e.g., `plaid_item_limit`). At link time, check current count against the limit; reject with `plaid_item_limit_reached` if exceeded.

Initial v1 caps (TBD with Adam):

| Tier | Plaid items allowed |
|---|---|
| `none` (DIY $5, hardware-without-sub) | **0** — Plaid blocked entirely |
| `paid` base ($29/mo) | 3 |
| `paid` pro ($199/mo) | 10 |

These live in Atlas's tier-mapping code, get pushed into `entitlements.plaid_item_limit` (new column) on subscription create/update.

### 3. Monthly amortized cost accrual

On the first Plaid call within a billing period, deduct the monthly fee proportionally per Item count. Two reasonable models:

**Model A — Pre-deduct on link:**
- When a new Item is created (link exchange success), immediately charge $1.00 (or whatever Plaid bills us) from `today_remaining_micros`.
- Pros: simple, customer sees the cost up-front, mirrors charge-then-use pattern.
- Cons: spreads $1 across 30 days unfairly — connecting on day 28 still costs the full month.

**Model B — Daily charge (1/30th):**
- Background job in virtues-api walks `entitlement_plaid_items` daily and charges `$0.033 × current_item_count` per row.
- Pros: prorated, fair.
- Cons: a background cron in virtues-api (new operational surface), daily DB writes for every Plaid user.

**Recommendation: Model A** for v1. Mullvad ships time top-up math, not per-day amortization. Customers churning early lose the unused days — same as cancelling a monthly Netflix subscription mid-month. Simpler, fair-enough.

### 4. New `/v1/plaid/*` routes (bearer-auth)

| Route | Purpose | Pattern |
|---|---|---|
| `POST /v1/plaid/link-token` | Create Link token for client-side flow | Bearer + tier check + item-count check. No upstream cost. |
| `POST /v1/plaid/exchange-token` | Convert public token to access token | Bearer + tier check. **This is the charge moment** — creates `entitlement_plaid_items` row + deducts $1.00 from `today_remaining_micros`. |
| `POST /v1/plaid/sync` | Sync transactions | Bearer + tier check + Item ownership check (the Item must belong to this bearer). No per-call charge. |
| `DELETE /v1/plaid/item/:item_id` | Disconnect Item | Bearer + ownership check. Deletes the row. No refund (already paid for the month). |
| `GET /v1/plaid/items` | List user's Items | Bearer-gated. Returns institution names + connected dates. |

### 5. Atlas side

- **Tier-mapping code adds `plaid_item_limit`** to entitlement push payload.
- **No new tables in Atlas.** Plaid item count is virtues-api state, not customer state.
- **No webhook handling for Plaid** (Plaid's webhooks fire on data updates; those flow through the existing core integration, not Atlas).

### 6. Plaid item ownership enforcement

Every Plaid call must verify the Item belongs to the calling bearer. Otherwise a bearer with a list of stolen `plaid_item_id`s could read someone else's bank data. The check is one cheap DB query:

```sql
SELECT 1 FROM entitlement_plaid_items
WHERE plaid_item_id = $1 AND bearer_hash = $2
```

This is the single biggest correctness invariant in the migration. Easy to write a lint that flags any Plaid route handler that doesn't perform this check.

## Migration steps when ready to ship

1. New migration `0002_plaid_items.sql` in virtues-api (table + index).
2. Add `plaid_item_limit` column to `entitlements` (separate migration, defaults to 0).
3. New `services/virtues-api/src/routes/plaid_v1.rs` (or extend existing `plaid.rs`).
4. Update Atlas's tier-to-entitlement mapping to push `plaid_item_limit`.
5. Migrate one Plaid endpoint at a time (link-token → exchange-token → sync → list/delete).
6. Once all migrated, retire `/v1/services/plaid/*` legacy routes.
7. Add lint: every handler in `plaid_v1.rs` that takes a `plaid_item_id` must call a verifier helper.

## Open questions

- Which Plaid product tier do we use? Their pricing varies by product (Transactions $0.30/Item/mo, Identity $1+/Item, etc.). The cost we deduct should reflect what we actually pay.
- Should we ever issue refunds? E.g., user connects an Item, immediately realizes it's the wrong bank, disconnects within 5 minutes. v1 says no; v2 maybe a 24h grace window.
- Should we cap **total** Plaid spending per bearer per month (regardless of Item count)? E.g., "max $5 worth of Items per month even on Pro tier." Probably not — `plaid_item_limit` is sufficient.

## What this isn't

This isn't a replacement for the existing Plaid client logic in `services/virtues-api/src/routes/plaid.rs`. The Plaid SDK calls, error handling, and response forwarding all stay — we're only changing the auth + billing layer around them. The Plaid client code can be lifted nearly verbatim into `plaid_v1.rs`.
