# Entitlement & Billing Architecture (Spec)

> The technical spec for how a paying customer's box gets AI and utility calls
> paid for. For the *why* and the customer-facing language — also rewritten to
> the shipped model — see [`virtues-api.md`](virtues-api.md).
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

## 6. What the linked model gave up, and got back

The voucher model bought structural unlinkability at the cost of every
operation that needs to address one customer. Collapsing it reverses both
columns of that trade.

| Capability | Voucher model | Linked model (ships today) |
|---|---|---|
| Instant per-account revocation | Impossible — no shared key to address | **Available**: Atlas knows the `account_id` |
| Chargeback clawback | Couldn't find the bearer; bounded loss of one month | **Available**: debit the ledger |
| Targeted abuse ban from the billing side | Impossible | **Available** |
| Daily-cap change taking effect immediately | No — travelled only on the next voucher | **Yes**: Atlas credits/updates the account directly |
| Recovery without losing the wallet | Awkward — the bearer *was* the wallet | **Yes**: `device_keys` is a rotatable pointer |
| Subpoena of both DBs cannot join identity↔usage | **Yes** | **No** — this is what was given up |
| Blind signatures | Judged unnecessary; separation did the work | The documented v2 path back (RFC 9474) |

The reason the trade was accepted: at the time of the cutover the userbase was
approximately one box, and the operational capabilities in the top half of that
table were all missing while the privacy property in the bottom half protected
nobody in particular. That calculus changes as the userbase grows, which is why
v2 is written down rather than dropped.

---

## 7. Endpoints

**Atlas**
- `POST /claim {session_id}` — Stripe session → device api_key (registers the
  device + credits the wallet with virtues-api as a side effect)
- `POST /credits/topup` / `POST /credits/auto-topup` — card-funded wallet top-up
- `POST /billing/portal/sessions` — → Stripe Customer Portal `{url}`
- `POST /webhooks/stripe` — maintains subscription status; drives renewal
  crediting (signature-verified, idempotent)
- `POST /init/*`, `GET /init/poll` — box link/login session dance
- `POST /iroh/register`, `GET /relay/config`, `POST /relay/authorize` — reach
  control plane (see [`relay-control-plane.md`](relay-control-plane.md))
- `POST /diag/install`, `POST /diag/crash` — diagnostic beacons
- `GET /health`

**virtues-api**
- `POST /internal/device` — Atlas registers an api_key hash → account
  (internal-secret gated)
- `POST /internal/credit` — Atlas credits a wallet (internal-secret gated)
- `POST /internal/block` / `POST /internal/unblock` / `GET /internal/blocklist`
- `GET /v1/whoami`, `GET /v1/usage`, `POST /v1/charge-test` — api_key canaries
- `/v1/ai/*` (chat/completions, completions, embeddings, models),
  `/v1/places/*`, `/v1/exa/*`, `/v1/unsplash/*`,
  `/v1/services/plaid/*` — gated upstreams
- `/{provider}/start|callback|exchange|refresh` — the OAuth proxy leg
- `GET /health`, `GET /ready`

There is no `POST /voucher` and no `POST /v1/redeem`; both were deleted with the
voucher model.

---

## 8. Housekeeping

- **virtues-api sweeper** (`sweeper.rs`): hourly. Deletes `accounts` past
  `expires_at + ACCOUNT_GRACE_DAYS` (which cascades to `device_keys` and
  `ledger`), and expired `blocklist` rows. Space reclamation, not privacy work.
- **Atlas**: no sweeper. It stores no usage and no per-call state.

---

## 9. Home-server client

The home server (core) talks to the cloud through `virtues-core/src/virtues_api/`:

- **`renew.rs`** — keeps its name from the voucher era, but there is no renewal
  dance left in it. It is the api_key's lifecycle: `claim()` (Stripe session →
  api_key), `store_api_key()` / `read_api_key()` / `has_api_key()` against the
  credential vault (`source_id = "virtues_api"`), plus `auto_topup()` and
  `fetch_portal_session()`. The api_key is a single rotatable credential — there
  is no refresh/access token pair.
- **`client.rs`** — `BearerClient` attaches the api_key to every proxy call.
  `post_json` for buffered calls; `stream` for SSE. It no longer auto-renews on
  402: a 402 now means *the wallet is empty*, which retrying cannot fix, so it
  surfaces (and may trigger auto-topup) instead.
- **Onboarding** — core `POST /api/billing/claim {session_id}`
  (`server/api.rs::claim_billing_handler`) runs `renew::claim()` +
  `store_api_key()`. Atlas has already registered the device and credited the
  wallet by the time that returns, so there is no second step.

**Everything goes through `BearerClient`.** Every home-server path that hits a
Virtues AI or utility upstream authenticates with the device api_key:

- AI (buffered, `post_json`): `day_summary`, `day_illustration` scene prompt,
  `chats::generate_title`, `compaction` background summary.
- AI (streaming): `agent/stream.rs::stream_llm_response` uses
  `BearerClient::stream("/v1/ai/chat/completions", …)`, so `chat_handler` and
  every agent-loop call (including the applet runner) go through it.
  `LlmConfig` carries the `BearerClient` rather than url+user_id+secret.
- Utilities: `places.rs` → `/v1/places/*`, `exa.rs` → `/v1/exa/search`,
  `unsplash.rs` → `/v1/unsplash/search`; `tools/web_search.rs` delegates to
  `api::exa::search`.
- Day-illustration image generation routes through `/v1/ai/chat/completions`;
  `AI_GATEWAY_API_KEY` is not used anywhere in core.
- The standalone `llm/client.rs` (`VirtuesApiClient`) was removed.

No live core path uses `X-Internal-Secret` — that header is Atlas↔virtues-api
only.

`subscription.rs` is **local-first**: `/api/subscription` reads the vault
(api_key presence) rather than calling the cloud. `/api/billing/portal` reads
the api_key from the vault and asks Atlas (`POST /billing/portal/sessions`) to
mint a Stripe-hosted Customer Portal session, returning `{url}` for the web
BillingView. Card, invoice, and cancellation management all live on Stripe;
Atlas never exposes a billing UI.

The streaming path forwards `provider_options` and `thought_signature` end to
end (`agent/stream.rs` adds both when present), so extended-thinking and
tool-signature continuity are preserved.

---

## 10. Open / deferred

- `/claim` real-Stripe smoke test (needs a live Stripe test account).
- Plaid per-Item cost model — Plaid data sync ships via the standard registry
  source model; a per-Item monthly-cost entitlement treatment is still open.
- Dunning recovery beyond `past_due` (full retry/grace flow).

- **Blind unlinkability (v2).** Restoring the property §1 says we gave up, via
  [RFC 9474](https://www.rfc-editor.org/rfc/rfc9474.html) blind signatures
  rather than the old disposable-voucher scheme. This is the one deferred item
  with a user-visible promise attached, so it should not drift quietly.

**Daily-cap propagation** is no longer deferred or latent: Atlas addresses the
account directly, so a change to `customers.daily_cap_micros` can be pushed with
the next `POST /internal/credit`. The voucher model's "privacy-preserving
latency" was a consequence of the wall and went away with it.

## 11. Behavioral blocklist

Implemented in [`blocklist.rs`](../services/virtues-api/src/blocklist.rs). Keyed
on `key_hash` — the SHA-256 of the api_key, i.e. the credential being rate-limited,
never a customer record. (The column was `bearer_hash` until migration 0005
renamed it.) The in-memory `DashMap` is
the hot path; the `blocklist` table is a restart snapshot, reloaded on boot and
swept hourly. Blocks are **TTL'd cooldowns**, not permanent bans — usage is
anonymous with no appeal channel, so a permanent false-positive would be
unrecoverable.

- **Enforcement**: the api_key auth layer checks `is_blocked` after resolving
  the account; blocked → 403 `blocked`.
- **Auto (rate) — observe-only by default**: each authenticated request hits a
  per-key fixed-window counter; exceeding `BLOCKLIST_RATE_LIMIT_PER_MIN`
  (default 600/min) *flags + logs* the key but does **not** block unless
  `BLOCKLIST_RATE_AUTOBLOCK=true`. The reason it's off by default: an api_key is
  *per-home-server*, aggregating background jobs + chat + parallel tool calls +
  per-keystroke autocomplete onto one counter, and cost is already capped by the
  daily budget — so a false positive would lock out a whole paying household
  (no appeal channel) for little benefit. We watch the flagged watchlist first,
  then enable enforcement with a threshold we know clears real peaks. When
  enabled, exceeding the ceiling triggers a `BLOCKLIST_BLOCK_TTL_SECS` cooldown
  (default 15 min).
- **Introspection**: `GET /internal/blocklist` (internal-secret gated) returns
  `autoblock_enabled`, the rate ceiling, active blocks, and the flagged
  watchlist (keys over the ceiling, with trip counts + peak) — so we can see
  the would-block signal without enforcing.
- **Manual (ops)**: `POST /internal/block` / `POST /internal/unblock`
  (internal-secret gated) take a `key_hash` learned from virtues-api's *own*
  abuse logs.

---

## 12. The voucher model (superseded)

Kept because it is the intended v2 destination, not because it describes
anything running today. Every table and endpoint named here is deleted.

Atlas and virtues-api shared **no column**. The bridge between them was a
**disposable voucher** that neither side retained as a link:

1. The box generated a fresh random bearer; only its hash ever left the device.
2. `POST /voucher {billing_token}` to Atlas. Atlas checked the subscription was
   active, passed an anti-stacking rate limit, minted a one-time `voucher_code`,
   and registered `sha256(code)` + value + `daily_cap_micros` with virtues-api
   via `POST /internal/voucher` — **a call containing no customer and no
   bearer**. It returned the raw code.
3. `POST /v1/redeem` to virtues-api with the code plus the bearer. virtues-api
   validated the voucher, loaded the budget and cohort-aligned expiry onto the
   bearer, marked the voucher redeemed, and **discarded which bearer redeemed
   it**.

The voucher was a relay baton: it passed from billing to the gate *through the
user's own home server*, and the instant it was spent both sides forgot it. The
two halves touched a shared object for a moment and kept no record of the touch.
`vouchers.redeemed_at` recorded **that** a voucher was spent, never **which
bearer** spent it, and that discard is what kept the chain from ever living in
one place.

Renewal was the OAuth refresh pattern — `bearer_expired` (402) → renew → retry —
which is why the client had a renew-and-retry path and why `renew.rs` still
carries that name.

Dropped in
[`0005_accounts_ledger.sql`](../services/virtues-api/migrations/0005_accounts_ledger.sql).
The reasoning is in [§6](#6-what-the-linked-model-gave-up-and-got-back).
