# Entitlement & Voucher Architecture (Spec)

> The technical spec. For the *why* and the marketing language, see
> [`Virtues-API.md`](../Virtues-API.md) in docs/.
>
> Supersedes the earlier `activation_handle` design. The load-bearing
> change: **no field is shared between Atlas and virtues-api.** The
> customer↔usage link exists only on the user's own home server.

---

## 1. The invariant

A government subpoena of **both** databases must not be able to tie any
usage to any person. Not by policy — by construction. The rule that
guarantees it:

**No column may exist in both the Atlas schema and the virtues-api schema.**

Lint 10 (`scripts/arch_lint.sh`) enforces it: identity columns from one
side are forbidden from appearing in the other's migrations.

---

## 2. Three parties

| Party | Owns | Knows | Never sees |
|---|---|---|---|
| **Home server** (VirtuesOS) | the user | billing token + usage bearer, linked locally | — (it's the user's own box) |
| **Atlas** (billing) | Virtues | customer, email, subscription, billing token | the usage bearer |
| **virtues-api** (gate) | Virtues | bearer, budget, expiry | the customer |

The bridge between Atlas and virtues-api is a **disposable voucher** that
neither retains as a link.

---

## 3. Schemas (no shared column)

### Atlas (`services/atlas/migrations/`)
```sql
customers(stripe_customer_id PK, email, billing_token_hash, last_voucher_issued_at, created_at)
subscriptions(stripe_subscription_id PK, stripe_customer_id FK, status, current_period_end, ...)
stripe_webhook_events(stripe_event_id PK, event_type, processed_at)
```

### virtues-api (`services/virtues-api/migrations/`)
```sql
entitlements(bearer_hash PK, daily_budget_micros, today_remaining_micros, today_reset_at, expires_at, ...)
vouchers(voucher_code_hash PK, budget_micros, valid_days, voucher_expires_at, redeemed_at)
blocklist(bearer_hash PK, reason_code, blocked_at, expires_at)
```

**Cross-check:** Atlas's identifier columns (`stripe_customer_id`, `email`,
`billing_token_hash`) appear nowhere in virtues-api. virtues-api's
identifiers (`bearer_hash`, `voucher_code_hash`) appear nowhere in Atlas.
The wall is the *absence* of a shared key.

Note `vouchers.redeemed_at` records **that** a voucher was spent, never
**which bearer** spent it. That discard is what keeps the chain from ever
living in one place.

---

## 4. Flows

### 4.1 Signup → claim (once)
1. Customer pays via Stripe Checkout → `success_url?session_id=cs_xxx`.
2. Home server `POST /claim {session_id}` to Atlas.
3. Atlas verifies `payment_status == "paid"`, upserts `customers` +
   `subscriptions`, mints a random **billing_token**, stores only its hash,
   returns the raw token.
4. Home server stores the billing_token locally.

Re-claim reissues a fresh billing_token (recovery is a billing-side
concern, which is allowed — the billing token carries no usage data).

### 4.2 Monthly renewal (lazy, first-request)
Triggered when the bearer is expired (virtues-api returns `bearer_expired`,
402). The home server:
1. Generates a fresh random bearer (only its hash ever leaves the device).
2. `POST /voucher {billing_token}` to Atlas → Atlas checks the
   subscription is active, anti-stacking rate-limit passes, mints a
   one-time **voucher_code**, registers `sha256(code)` + value with
   virtues-api via `POST /internal/voucher` (no customer, no bearer in
   that call), returns the raw code.
3. `POST /v1/redeem` to virtues-api with the code + bearer (in the
   Authorization header). virtues-api validates the voucher, loads the
   budget + cohort-aligned expiry onto the bearer, marks the voucher
   redeemed, and **discards which bearer redeemed it**.
4. Retries the original call.

This is the OAuth refresh-token pattern: `bearer_expired` → renew → retry.
One slightly slower call per month; otherwise invisible.

### 4.3 Per-call charging
`bearer_auth` validates `expires_at > now()`. Paid routes call
`entitlement::charge(bearer_hash, cost)`: lazy daily reset → atomic
conditional decrement. AI cost is authoritative from Vercel AI Gateway's
`usage.cost`; fixed-cost routes (Places/Exa) use a constant; failed
upstreams refund.

### 4.4 Cancellation / refund
Stripe webhook → Atlas updates `subscriptions.status`. **No call to
virtues-api.** Revocation is by expiry: a canceled subscription stops
producing vouchers, so the bearer runs out at month end. Refund/dispute
sets status to `refunded`, same effect.

---

## 5. Cohort-aligned expiry

`voucher::redeem` sets `expires_at = first-of-month after
(max(now, current_expiry) + valid_days)`. Everyone who redeems lands on a
shared monthly boundary → uniform expiry timestamps, no per-user
fingerprint. This is *defense-in-depth*: the real unlinkability comes from
discarding the voucher↔bearer link and Atlas not logging voucher requests.

---

## 6. What we deliberately gave up

| Capability | Why it's gone | Mitigation |
|---|---|---|
| Instant per-user revocation | Would require Atlas to address a bearer → a shared key → breaks the wall | Expiry (month-end) |
| Chargeback clawback on a specific bearer | Can't find the bearer | Bounded loss (one month); economics cover it |
| Targeted abuse ban from billing side | Atlas can't reach a bearer | Per-bearer rate limits + behavioral blocklist, all on virtues-api side |
| Blind signatures | Unnecessary — issuance & redemption are already in separate trust domains | Architectural separation does the work |

---

## 7. Endpoints

**Atlas**
- `POST /claim {session_id}` — Stripe session → billing_token
- `POST /voucher {billing_token}` — → voucher_code (registers with virtues-api)
- `POST /webhooks/stripe` — maintains subscription status (signature-verified, idempotent)
- `GET /health`

**virtues-api**
- `POST /internal/voucher` — Atlas registers a voucher (internal-secret gated)
- `POST /v1/redeem` — device redeems voucher onto bearer
- `GET /v1/whoami`, `POST /v1/charge-test` — bearer-auth canaries
- `/v1/places/*`, `/v1/exa/*`, `/v1/unsplash/*`, `/v1/ai/*` — gated upstreams

---

## 8. Housekeeping

- **virtues-api sweeper** (`sweeper.rs`): hourly, deletes expired
  entitlements (past 7-day grace) + dead vouchers. No privacy weight — just
  space reclamation; there's no link to delete.
- **Atlas**: no sweeper needed in the voucher model (it stores no vouchers
  and no bearer links).

---

## 9. Home-server client

The home server (core) runs the voucher dance via `virtues-core/src/virtues_api/`:
- `renew.rs` — `claim()`, `store_billing_token()`, `renew()` (the voucher
  dance), `current_bearer()`. Secrets live in the credential vault
  (`source_id = "virtues_api"`): `billing_token` ≈ refresh token, `bearer`
  ≈ access token.
- `client.rs` — `BearerClient`: attaches the current bearer to bearer-route
  calls and auto-renews once on `bearer_expired` (402). `post_json` for
  buffered calls; `stream` for SSE, which renews *before* opening the stream
  (mid-stream renewal is impossible) and retries once on a connect-time 402.
- `actions/virtues_api_renew/` — the renewal as a visible, catalog-declared
  action (transparency motif).
- Onboarding: core `POST /api/billing/claim {session_id}`
  (`server/api.rs::claim_billing_handler`) runs `renew::claim()` +
  `store_billing_token()`, then eagerly mints the first bearer (best-effort).

**Migrated to BearerClient.** Every home-server path that hits a Virtues AI
or utility upstream now authenticates with the device bearer:
- AI (buffered, `post_json`): `day_summary`, `day_illustration` scene prompt,
  `chats::generate_title`, `compaction` background summary.
- AI (streaming): `agent/stream.rs::stream_llm_response` uses
  `BearerClient::stream("/v1/ai/chat/completions", …)`, so `chat_handler` and
  every agent-loop call (incl. `action_runner`) go through the bearer.
  `LlmConfig` carries the `BearerClient` instead of url+user_id+secret.
- Utilities: `places.rs` → `/v1/places/*` (`post_json` + `get_json`),
  `exa.rs` → `/v1/exa/search`, `unsplash.rs` → `/v1/unsplash/search`,
  `tools/web_search.rs` now delegates to `api::exa::search` (the duplicate
  Exa client was deleted).
- The standalone `llm/client.rs` (`VirtuesApiClient`, legacy
  `/v1/chat/completions`) was **removed** — its only caller (compaction) now
  uses `BearerClient`.

`subscription.rs` is now **local-first**: `/api/subscription` reads the vault
(billing-token presence) instead of calling the deleted virtues-api
`/v1/subscription`; gating is by bearer expiry. `/api/billing/portal` returns
a clean "managed in billing, not yet wired" message (the portal is an Atlas
concern — §10).

Day-illustration image generation (`day_illustration::generate_image_via_gateway`)
also routes through `/v1/ai/chat/completions` now — `AI_GATEWAY_API_KEY` is no
longer used anywhere in core, and image-gen cost is metered through the bearer.

**Still on legacy `X-Internal-Secret` (deliberately deferred):**
- `chat.rs::_create_chat_stream_legacy` — dead code (`#[allow(dead_code)]`),
  kept for reference; not a live path.

Note: the streaming path forwards model/messages/max_tokens/temperature/
tools/tool_choice only — `provider_options` and `thought_signature` are
dropped at the virtues-api `StreamingRequest` boundary. This matches the
prior legacy behavior (no regression); restoring them is a separate fix.

## 10. Open / deferred

- `/claim` real-Stripe smoke test (needs a live Stripe test account).
- Billing-portal-based billing_token reissue (currently via re-claim).
- Plaid (see [`plaid-migration.md`](plaid-migration.md)) — its per-Item
  monthly cost model needs separate treatment.

## 11. Behavioral blocklist

Implemented in [`blocklist.rs`](../../services/virtues-api/src/blocklist.rs). Keyed
on the anonymous `bearer_hash` (never a customer). The in-memory `DashMap` is
the hot path; the `blocklist` table is a restart snapshot, reloaded on boot and
swept hourly. Blocks are **TTL'd cooldowns**, not permanent bans — usage is
anonymous with no appeal channel, so a permanent false-positive would be
unrecoverable.

- **Enforcement**: `bearer_auth` checks `is_blocked` after expiry; blocked →
  403 `blocked`.
- **Auto (rate) — observe-only by default**: each authenticated request hits a
  per-bearer fixed-window counter; exceeding `BLOCKLIST_RATE_LIMIT_PER_MIN`
  (default 600/min) *flags + logs* the bearer but does **not** block unless
  `BLOCKLIST_RATE_AUTOBLOCK=true`. The reason it's off by default: a bearer is
  *per-home-server*, aggregating background jobs + chat + parallel tool calls +
  per-keystroke autocomplete onto one counter, and cost is already capped by the
  daily budget — so a false positive would lock out a whole paying household
  (no appeal channel) for little benefit. We watch the flagged watchlist first,
  then enable enforcement with a threshold we know clears real peaks. When
  enabled, exceeding the ceiling triggers a `BLOCKLIST_BLOCK_TTL_SECS` cooldown
  (default 15 min).
- **Introspection**: `GET /internal/blocklist` (internal-secret gated) returns
  `autoblock_enabled`, the rate ceiling, active blocks, and the flagged
  watchlist (bearers over the ceiling, with trip counts + peak) — so we can see
  the would-block signal without enforcing.
- **Manual (ops)**: `POST /internal/block` / `POST /internal/unblock`
  (internal-secret gated) take a `bearer_hash` learned from virtues-api's *own*
  abuse logs. Atlas is never involved — it has no bearer to send.
