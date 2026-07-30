# Entitlement & Billing Architecture (Spec)

> The technical spec for how a paying customer's box gets AI and utility calls
> paid for. For the *why* and the marketing language, see
> [`virtues-api.md`](virtues-api.md).
>
> **Rewritten 2026-07-29.** This document previously specified a *double-blind
> voucher* model — Atlas and virtues-api shared no column, and a disposable
> voucher passed between them through the user's own box so that neither side
> could join identity to usage. **That model is not what ships.** It was
> collapsed to a **linked prepaid ledger** in
> [`0005_accounts_ledger.sql`](../services/virtues-api/migrations/0005_accounts_ledger.sql),
> which drops the `vouchers` and `entitlements` tables outright. Section 1
> states plainly what was traded away. The voucher design is preserved as
> history in [§12](#12-the-voucher-model-superseded) because it is still the
> intended v2 destination.

---

## 1. The privacy posture — read this first

**As shipped, identity and usage are linkable by whoever holds both databases.**

Atlas assigns each customer an opaque `account_id`, and **shares that same
`account_id` with virtues-api**, which keys the wallet on it. A subpoena of both
databases joins on that column: Atlas turns `account_id` into a Stripe customer
and an email; virtues-api turns it into a spend history. There is no
cryptographic obstacle to that join — the wall the earlier spec described was
removed deliberately, not breached.

What remains true, and is the part worth defending:

- **`account_id` is an opaque random string** (`acct_<32hex>`) — never a Stripe
  id, never an email.
- **Content never leaves the box.** virtues-api stores token counts and costs.
  It never stores prompts, completions, or any personal data.
- **The proxy knows nothing about people** on its own. It needs Atlas to turn an
  account into a person.

What is *not* true today, and must not be claimed:

- ~~"No column exists in both schemas."~~ `account_id` does.
- ~~"A subpoena of both databases cannot tie usage to a person."~~ It can.

Blind unlinkability ([RFC 9474](https://www.rfc-editor.org/rfc/rfc9474.html))
remains the documented v2 goal. Until it ships, the honest claim is
*data minimization* — we hold little, and none of it is content — not
*structural unlinkability*.

**Lint 10** (`tools/arch-lint.sh`) still enforces the narrower rule that
survives: Atlas's *customer-identity* columns (`stripe_customer_id`,
`billing_token`, `email`) must never appear in virtues-api's schema. That keeps
the proxy free of direct identity, which is why the join needs both sides rather
than one.

---

## 2. Three parties

| Party | Owns | Knows | Never sees |
|---|---|---|---|
| **Home server** (VirtuesOS) | the user | the device `api_key`; all of the user's data | — (it's the user's own box) |
| **Atlas** (billing) | Virtues | customer, email, subscription, `api_key_hash`, `account_id` | any usage detail; any content |
| **virtues-api** (gate) | Virtues | `account_id`, balance, ledger, `api_key_hash` | who the customer is; any content |

`account_id` is the shared join key. Atlas mints it and hands it to virtues-api
when registering a device or crediting the wallet.

---

## 3. Schemas

### Atlas (`services/virtues-atlas/migrations/`)
```sql
customers(stripe_customer_id PK, email, api_key_hash, account_id UNIQUE, daily_cap_micros, created_at)
subscriptions(stripe_subscription_id PK, stripe_customer_id FK, status, current_period_end, ...)
device_link(...)                 -- box↔customer link sessions
stripe_webhook_events(stripe_event_id PK, event_type, processed_at)
```

### virtues-api (`services/virtues-api/migrations/`)
```sql
accounts(account_id PK, balance_micros, today_spent_micros, today_reset_at,
         daily_cap_micros, expires_at, created_at, updated_at)
device_keys(api_key_hash PK, account_id FK, created_at)
ledger(id PK, account_id FK, micros, kind, ts, ...)      -- append-only, source of truth
blocklist(key_hash PK, reason_code, blocked_at, expires_at)
```

**`accounts.balance_micros` is a projection**, not the truth: it must always
equal `SUM(ledger.micros)` for that account, and is rebuildable from the ledger
at any time. The ledger is append-only; nothing is ever updated in place.

**`device_keys` is a separate, rotatable pointer.** Rotating or losing an
api_key replaces a row there and never touches the balance. This is why recovery
does not cost the user their wallet.

---

## 4. Flows

### 4.1 Signup → link (once)
1. Customer pays via Stripe Checkout → `success_url?session_id=cs_xxx`.
2. Home server `POST /claim {session_id}` to Atlas.
3. Atlas verifies `payment_status == "paid"`, upserts `customers` +
   `subscriptions`, assigns an `account_id` if the customer has none, mints a
   random **api_key**, and stores only its hash.
4. Atlas registers the device with virtues-api (`POST /internal/device`) — the
   api_key hash plus the `account_id` — and credits the wallet
   (`POST /internal/credit`).
5. Atlas returns the raw api_key. The home server stores it in the credential
   vault under `source_id = "virtues_api"`.

Re-linking rotates the api_key and re-points the device at the **same**
`account_id`, so the balance survives.

### 4.2 Renewal and top-ups (server-side)
There is no client-side renewal dance. Renewal is a **Stripe webhook** to Atlas,
which SETs the month's allotment on the wallet via `POST /internal/credit`.
Top-ups are a card charge through Atlas (`POST /credits/topup`, and
`POST /credits/auto-topup` for the standing arrangement), which credits the same
way. The box is not involved and holds no refresh token.

### 4.3 Per-call charging
The box sends `Authorization: Bearer <api_key>` on every proxy call. virtues-api
SHA-256s it, resolves `device_keys → accounts`, checks the balance and the daily
cap (lazy reset on the first call after `today_reset_at`), then appends a debit
to `ledger` and decrements the projection. AI cost is authoritative from Vercel
AI Gateway's `usage.cost`; fixed-cost routes (Places/Exa/Unsplash) use a
constant; failed upstreams refund by appending a credit.

**402 means the wallet is empty** (surface it, or auto-topup). **401 means the
key is unknown** (re-link). These are the only two states the box must handle.

### 4.4 Cancellation / refund
Stripe webhook → Atlas updates `subscriptions.status` and stops crediting.
Revocation is by **expiry plus exhaustion**: `accounts.expires_at` is
cohort-aligned to the 1st of the month UTC, and leftover balance is
use-it-or-lose-it. Unlike the voucher model, Atlas *can* now act on a specific
account directly if it has to — that capability is a consequence of the shared
`account_id`.

---

## 5. Cohort-aligned expiry

`expires_at` is set to the first of the month (UTC) rather than a per-customer
anniversary, so wallets share a small number of expiry timestamps instead of
carrying a per-user one. Under the voucher model this was defense-in-depth for
unlinkability. **Under the linked model it no longer buys privacy** — the shared
`account_id` already links the two sides — and it is kept purely because uniform
month boundaries make renewal accounting and the sweeper simple.

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
- `POST /voucher {billing_token}` — → voucher_code (registers with virtues-api, carries `daily_cap_micros`)
- `POST /billing/portal/sessions {billing_token, return_url?}` — → Stripe Customer Portal `{url}`
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
`/v1/subscription`; gating is by bearer expiry. `/api/billing/portal` reads
the billing token from the vault and asks Atlas (`POST /billing/portal/sessions`)
to mint a Stripe-hosted Customer Portal session, returning `{url}` for the web
BillingView to open. Card/invoice/cancellation management all live on Stripe;
Atlas resolves billing_token → customer and never exposes a billing UI.

Day-illustration image generation (`day_illustration::generate_image_via_gateway`)
also routes through `/v1/ai/chat/completions` now — `AI_GATEWAY_API_KEY` is no
longer used anywhere in core, and image-gen cost is metered through the bearer.

Every home-server AI/utility path now authenticates with the device bearer;
no live core path uses `X-Internal-Secret`.

The streaming path forwards `provider_options` and `thought_signature` end to
end (`agent/stream.rs` adds both to the request body when present), so
extended-thinking / tool-signature continuity is preserved on the bearer route.

## 10. Open / deferred

- `/claim` real-Stripe smoke test (needs a live Stripe test account).
- Plaid per-Item cost model — Plaid data sync ships via the standard registry
  source model; a per-Item monthly-cost entitlement treatment is still open.
- Dunning recovery beyond `past_due` (full retry/grace flow).

**Daily-cap propagation latency (by design):** a change to
`customers.daily_cap_micros` takes effect at the customer's *next* voucher or
top-up, because the cap travels only on the voucher (the wall forbids Atlas
addressing a bearer directly). This is the natural, privacy-preserving latency
— not a bug, and we deliberately don't add an eager-refresh path for a spend
ceiling.

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
