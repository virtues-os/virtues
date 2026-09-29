# Accounts, billing and pairing — what is still open

**Status:** Open (2026-09-29). Collects items that were left orphaned when
`open-relay-plan.md` became `agents/record/open-relay.md` and
`onboarding-plan.md` and `linking-plan.md` were archived. Each one was
checked against the code on 2026-09-29; anything already built was dropped.
Delete an item when it ships and move its reasoning into a record.

## Billing defects

**Refunds are scoped to the customer, not the charge.** In
`services/virtues-atlas/src/routes/webhooks.rs`, `handle_webhook` sends both
`charge.refunded` and `charge.dispute.created` to
`set_status(pool, object, "refunded")`, which runs
`UPDATE subscriptions SET status = $1 WHERE stripe_customer_id = $2`. Top-ups
are charges against the same customer, so refunding a $10 goodwill top-up (or
losing a dispute on one) marks the $20/mo subscription `refunded`, and every
path that requires an active subscription then refuses: the billing portal,
manual and auto top-up. Refunds and disputes are not something we choose to
have. The fix separates the objects: a refund against a top-up debits the
wallet, and only a refund or dispute against a subscription invoice changes
subscription status.

**The way to fix a lapsed payment sits behind the payment.**
`billing_portal.rs::resolve_active_customer` refuses unless the latest
subscription status is exactly `active`, so a `past_due` owner cannot open the
Stripe portal to update their card. Un-gate the portal for any account with a
Stripe customer, and decide whether `past_due` gets a grace window before
hosted AI stops.

## Decisions nobody has made

- **What "delete my account" means.** There is no route for it in atlas.
  Proposed in the archived onboarding plan: atlas forgets you, and the box
  keeps working on the LAN and over the relay. It is the sovereignty claim in
  its most testable form, and cheap to settle before there are many accounts.
- **Multi-box policy.** Since atlas `0015`, keys are scoped per box, so a
  second box no longer rotates the first box's credential, and entitlement is
  checked per account. The de facto policy is therefore "one subscription
  covers every box on the account, with nothing capping the count." Ratify
  that or change it; nobody chose it.
- **Store entitlement pre-provisioning.** A buyer who pays for box and
  subscription at virtues.com should never see a payment screen during setup.
  Nothing creates an account or entitlement at purchase time today (atlas has
  only `/preorder/checkout`).
- **Where the subscription is offered.** Unresolved contradiction.
  `connect.html` ("setup never mentions money") and the open-relay decision say
  setup's only questions are network and ownership, and the subscription is
  offered the first time hosted AI is invoked. `agents/plan/setup-plan.md`
  and the built stepper (`routes/(onboarding)/setup/[[step]]/+page.svelte`,
  `StepSubscription.svelte`) make Subscription a required step with no skip,
  passable by subscribing, signing in, or bringing your own AI. Either the
  airlock comment and the record's principle are narrowed to "the airlock
  never mentions money," or the step gains a skip. Decide, then fix the loser.

## Lifecycle

- **`virtues unlink` does not exist**, and neither does an account-page
  release. A box whose owner wants to detach it from an account, or sell it,
  has only `deprovision` (which re-mints the EndpointId, so atlas sees a new
  box). Build the unlink command and the atlas release together.

## Setup failure modes

- **Fresh box, wrong clock.** Nothing in `virtues-core` checks that the clock
  is synced before talking to atlas. A wrong clock breaks TLS and every expiry
  check with errors that name neither. Detect it (`timedatectl`
  `NTPSynchronized`, or a large skew against an atlas `Date` header) and say so.

## Pairing

- **One pairing doctrine.** Four narratives still coexist: the BLE codeless
  session, the LAN six-digit form fed by `virtues pair`, the `virtues pair`
  URL, and the phone handoff QR. They are all one thing: a token minted where
  you already have access, carried to the new device. Make `virtues pair` mint
  one token rendered as QR, URL or short code, give the airlock a single
  "I have a code / scan" surface, and let vouching from a paired device cover
  the rest.
- **Offer to save the setup phrase to the password manager.** The save screen
  in `connect.html` (`blePhraseSaved`) now gates Continue on an attestation,
  with Copy and Print. Writing the phrase through the platform credential API
  would turn "did you save it" into "your Mac saved it." The app must still
  never persist or email the phrase itself.

## Relay

- **An aggregate-only relay dashboard.** `iroh-relay` already exposes metrics
  on `127.0.0.1:9090` and nothing reads them. Four numbers: concurrent
  connections, relayed bytes per day, unique endpoints per day, and the share
  of sessions relayed versus hole-punched (the product-health metric and the
  cost forecast in one). The rules: aggregates only, unique endpoints counted
  with a sketch such as HLL rather than stored IDs, nothing about who talked to
  whom.

## OAuth proxy and entitlement

Carried from [`../record/entitlement-split.md`](../record/entitlement-split.md);
each checked against `services/virtues-api/src/routes/oauth.rs` on 2026-09-29.

- **Bind an OAuth session to the box that started it.** `/start` is a browser
  navigation, so the box cannot put its api_key on it; the session is guarded
  only by the *shape* of its `return_url`. The box should register the session
  server-to-server first, then send the browser. Needs a box release.
- **Require `X-Virtues-Api-Key` on the proxy.** `caller_api_key` reads it and
  enforces nothing, because boxes that have not upgraded do not send it. Flip it
  to required once the logs show the fleet has moved — `/refresh` first (worst
  exposure, one known caller), then the rest.
- **Count linked-free accounts.** One query on atlas, not yet run.
