# The entitlement split — a decoupling that no consumer was told about

*2026-09-03. Describes what was found and fixed on wave that day; the fixes
ship with the next box release and atlas deploy.*

## What happened

On 2026-08-31, atlas migration 0017 decoupled account identity from Stripe
(`d8d25162`). The purpose was right: relay access and second-box pairing had
been paywalled by accident, because the only account row was a Stripe
customer. After 0017, a sign-in mints an `account_id` and a box `api_key`
whether or not a subscription exists. **Linking became identity, not
billing.**

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
| `open-relay-plan.md` §4 | the half of the plan meant to catch this — never built |

A beta owner signed in free, saw Active, pressed the one button that would
have corrected the picture, and retried a permanent state in circles.

No money was ever wrong. Enforcement lives in the wallet, so the box's
mistaken picture cost nothing except the truth on the screen — which is
exactly why it survived three days unnoticed. Nothing tripped because nothing
depended on it.

## The failure class

**A semantic decoupling is a contract change.** When one bit stops meaning two
things, every consumer that read it as the other thing is now wrong, and
prose comments are the last to know. The repo already has this rule for
column renames ("sweep: SQL strings, `row.get`, the catalog…"). It applies to
meanings as much as names.

Two accelerants, both already banned in CLAUDE.md and both present:

- `get_subscription_handler` answered a failed vault read with a hardcoded
  `"active"` — a blipped query granted a subscription.
- atlas `/settings` ran `.ok().flatten()` over the customer lookup, so a DB
  error returned `401 invalid_api_key`, which the box reads as "re-link".

## What was done

- **A door the box can knock on.** atlas `POST /account/entitlement
  { api_key } → { linked, subscribed }` — read-only, Stripe-free. `is_entitled`
  and `key_owner` were already exact; only the door was missing.
- **`/api/subscription` reports two facts** — `linked`, `subscribed`,
  `entitlement_known` — with `status: none | linked | active`. The box caches
  atlas's answer 5 minutes and holds the last one indefinitely on outage: an
  unreachable atlas is not a billing event. A never-answered state renders as
  *unknown*, never as *unsubscribed*.
- **A way to buy from a linked free box.** atlas `POST /billing/checkout/
  sessions { api_key }` → checkout with the account's email pre-filled,
  finalized by the existing `account_checkout_done`. Core `POST
  /api/billing/subscribe`. Settings offers **Subscribe**, not *Connect* —
  connecting again would mint a second account.
- **Metered 402s say the right sentence.** `payment_required_message` reads
  the last-known entitlement: a never-subscribed account is told it needs a
  subscription (or a BYO key); a paying one whose balance ran out is told to
  top up. Every sentence carries its code.
- **Error responses carry `code`.** `{error, code}` on the portal and
  checkout doors; atlas's own codes pass through, plus `not_linked`,
  `vault_unreadable`, `atlas_unreachable` for failures that never reach it.
- **Trials purged.** There is no trial and never was. Three places carried a
  countdown and an "expired" toast for a plan that did not ship.
- **Setup copy** names the skip in accurate words. Account creation stays
  priced because atlas creates account and subscription in one checkout.

## Still open

- Bind an OAuth session to the box that started it (needs the box to
  register the session server-to-server before sending the browser).
- Flip the OAuth proxy's `X-Virtues-Api-Key` from logged to required once the
  fleet has upgraded; `/refresh` first.
- How many accounts are linked-free — one query on atlas, not yet run.
