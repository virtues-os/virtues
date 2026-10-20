# The entitlement split — a decoupling that no consumer was told about

*2026-09-03. Describes what was found and fixed on wave that day; the fixes
ship with the next box release and atlas deploy.*

## What happened

On 2026-08-31, atlas migration 0017 decoupled account identity from Stripe
(`d8d25162`). The purpose was right: relay access and second-box pairing had
been paywalled by accident, because the only account row was a Stripe
customer. After 0017, a sign-in mints an `account_id` and a box `api_key`
whether or not a subscription exists. **After 0017, a linked box proves
identity only; it no longer implies a subscription.**

Nothing downstream was told. Every reader of "does this box hold an api_key"
kept the old meaning — *subscribed*:

| reader | what it did with the bit |
|---|---|
| `virtues-core/src/api/subscription.rs` | reported `status: "active"` for any key |
| `BillingView.svelte` | rendered **Standing: Active** and a Stripe-portal button |
| `create_billing_portal_handler` | portal 402'd → "Couldn't open the billing portal. Try again." |
| `virtues_api/completion.rs` and two siblings | a free account's `wallet_empty` rendered as "Usage limit reached" |
| `AccountGate.svelte` | "No Virtues subscription on that email — create a new account instead" |
| `box_status.rs` doc comments | "the same key makes AI ready immediately (the wallet is funded at link)" |
| `open-relay-plan.md` §4 (now `agents/record/open-relay.md`) | the half of the plan meant to catch this — never built |

A beta owner signed in free, saw Active, pressed the one button that would
have corrected the picture, and kept retrying an error that could never
clear.

No money was ever charged wrongly. Enforcement lives in the wallet, so the
box's mistaken state only made the screen wrong. That is why it went
unnoticed for three days: no check or payment depended on it, so nothing
failed.

## The failure class

**Changing what a value means is a contract change.** When one bit stops
meaning two things, every consumer that read it as the other thing is now
wrong, and prose comments are the last thing anyone updates. The repo already has this rule for
column renames ("sweep: SQL strings, `row.get`, the catalog…"). It applies to
meanings as much as names.

Two other bugs made it worse, both already banned in CLAUDE.md and both present:

- `get_subscription_handler` answered a failed vault read with a hardcoded
  `"active"` — a blipped query granted a subscription.
- atlas `/settings` ran `.ok().flatten()` over the customer lookup, so a DB
  error returned `401 invalid_api_key`, which the box reads as "re-link".

## What was done

- **A new atlas endpoint the box can query.** atlas `POST /account/entitlement
  { api_key } → { linked, subscribed }` — read-only, Stripe-free. `is_entitled`
  and `key_owner` were already exact; only the endpoint was missing.
- **`/api/subscription` reports two facts** — `linked`, `subscribed`,
  `entitlement_known` — with `status: none | linked | active`. The box caches
  atlas's answer 5 minutes and keeps the last one indefinitely on outage,
  because atlas being unreachable does not change anyone's billing state. A never-answered state renders as
  *unknown*, never as *unsubscribed*.
- **A way to buy from a linked free box.** atlas `POST /billing/checkout/
  sessions { api_key }` → checkout with the account's email pre-filled,
  finalized by the existing `account_checkout_done`. Core `POST
  /api/billing/subscribe`. Settings offers **Subscribe**, not *Connect* —
  connecting again would mint a second account.
- **Metered 402s show the correct message.** `payment_required_message` reads
  the last-known entitlement: a never-subscribed account is told it needs a
  subscription (or a BYO key); a paying one whose balance ran out is told to
  top up. Every sentence carries its code.
- **Error responses carry `code`.** `{error, code}` on the portal and
  checkout endpoints; atlas's own codes pass through, plus `not_linked`,
  `vault_unreadable`, `atlas_unreachable` for failures that never reach it.
- **Trial code removed.** There is no trial and there never was one. Three places carried a
  countdown and an "expired" toast for a plan that did not ship.
- **Setup copy** describes the skip accurately. Account creation stays
  priced because atlas creates account and subscription in one checkout.

## Still open

Moved to [`../plan/account-plan.md`](../plan/account-plan.md) (OAuth proxy and
entitlement) on 2026-09-29.
