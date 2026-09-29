# The open relay

Written 2026-09-29. Built 2026-08-31 to 2026-09-01. Reachability stopped being
something the subscription sells: the relay admits any endpoint, a box knows
its relay from first boot, and setup never mentions money.

The code carries the current behavior: `deploy/iroh-relay/config.toml` (the
relay), `services/virtues-atlas/src/routes/relay.rs` (what atlas still does and
what it deleted), `virtues-core/src/relay.rs` (`resolve_relay_url`,
`DEFAULT_RELAY_URL`), and the "setup never mentions money" block in
`apps/web/src-tauri/ui/connect.html`. This is the reasoning behind them.

## The incident

A beta owner on ordinary IPv4 home wifi set up her box and skipped the
subscription, whose copy promised "LAN-only by choice." What she got was a Mac
that could *discover* the box over mDNS and never connect to it, a code screen
asking for six digits that existed nowhere, and, after she changed her mind
and subscribed, a checkout screen that polled for the payment with no timeout
until she gave up. No screen could tell her what was wrong, because by the
product's own story nothing was: every screen did what it said. The story was
the defect.

## Why skipping money broke connectivity

In iroh the relay is not a fallback data path. It is the **rendezvous**: two
peers behind IPv4 NAT can only hole-punch after swapping address candidates,
and the swap goes through the relay. Relay access therefore gated three
things: remote reach, hole-punch coordination, and, on any network where local
discovery fails (AP/client isolation, mesh nodes on separate segments,
filtered multicast), any connection at all.

The box only learned a relay URL from atlas at link time, authenticated by its
api key, and the airlock only wrote the account grant when the owner was
entitled. So unpaid meant unlinked, unlinked meant `RelayMode::Disabled`, and
the free path degraded not to "LAN-only" but to "works if your router happens
to be friendly." That is a property of the owner's access point, which she
cannot inspect and no error message can name.

**The failure class: a paywall on the connectivity substrate is
indistinguishable from a bug.** The owner experiences it as one, and so does
everyone who tries to diagnose it.

## What the relay was actually doing

Inspection of the live host on 2026-08-31 found stock n0 `iroh-relay` on one
hostname, with admission enforced by an `[access.http]` hook: every client
connection triggered a real-time callout to atlas `/relay/authorize` carrying
the client's EndpointId, answered from account data. Two consequences.

- The per-box SNI and HMAC control plane described in older prose did not
  exist in the running system. The box's relay config was just a URL. Per-box
  SNI would have been worse anyway, since it puts box identity in plaintext on
  the wire.
- Atlas was observing which endpoints connected and when, joined to accounts,
  in real time. The relay could not read payload, but its front door reported
  every connection to the billing system.

So most of the work was deletion.

## The decision

- **The relay admits any endpoint.** Rendezvous is never metered: candidate
  exchange and hole-punch assist are a few packets, and they are what make
  pairing and reach work at all.
- **Relayed payload gets one flat per-client cap, the same for everyone.**
  Tiers were rejected because tiering requires the relay to know who
  subscribes, which rebuilds the linkage being deleted. The cap is set about
  thirty times above the heaviest real stream (iOS audio). Its job is to bound
  abuse, not to meter customers. If tiering is ever forced, the shape to use
  is an atlas-signed offline voucher verified at the relay with no callout,
  never the authorize hook.
- **Abuse needs no identity.** An iroh relay is not a proxy: it carries
  encrypted traffic between two endpoints that both chose to talk and cannot
  reach any third-party server. The worst abuse is two strangers using it as a
  free pipe, a bandwidth problem the cap bounds. Stock `iroh-relay` 1.0 has no
  per-IP knob, so the per-client bucket plus a global accept rate is the whole
  defense.
- **The subscription gates hosted AI and services, nothing else.**
  Reachability, like the record, is something the owner owns. Atlas
  entitlement still guards checkout, top-ups and the billing portal, where the
  money is.
- **The relay URL ships in the box binary, named out loud, with a real off
  switch.** Resolve order is stored config, then `VIRTUES_RELAY_URL`, then the
  baked default, the last only on a box install so a dev checkout never homes
  on production. The off switch is an explicit word (`off`, `none`,
  `disabled`), so an empty env var and "off" stay different states. Settings
  names the relay and toggles it. Rejected: defaulting DIY boxes to off
  (breaks out-of-the-box reach for the primary install path) and defaulting
  them to n0's public relays (moves connection metadata to a third party).

## What was built

- **The relay.** `[access.http]` removed, so access defaults to everyone;
  `[limits]` and a per-client receive bucket added; metrics bound to loopback.
  `config.toml` warns against re-adding an access block: `iroh-relay` treats a
  non-200 from a dead callout URL as deny-everyone, which closes the relay
  silently.
- **Atlas, accounts decoupled from Stripe** (migration `0017`). Account
  identity had been keyed on `stripe_customer_id`, so a never-subscribed
  sign-in had no row to attach a box to. An `accounts` table now mints
  identity at sign-in; grant, approve and redemption lost their 402s;
  `/relay/config` resolves the account with no subscription check. The
  alternative, creating an empty Stripe customer per sign-in, was rejected
  because Stripe should learn only about people who pay it.
- **`/relay/authorize` deleted**, with its bearer secret.
- **The registry deleted a day later** (migration `0018`). An audit found
  `/iroh/register`, the `iroh_endpoints` table and the box's reporting call
  all still running every reconcile tick: a refreshed inventory of every box
  and every paired device, keyed to a billing account, read by nothing. Older
  boxes that still post get a 404 and carry on, since the call was always
  best-effort.
- **The baked default** (`6c56b6ce`): `DEFAULT_RELAY_URL`, the off word,
  `/api/network/relay` behind Settings, and the manual's reach page stops
  claiming a subscription gate.
- **The airlock** (`7e68ffaa`): checkout deleted, the grant ungated so a
  signed-in owner links free, and the claimed-box code screen now leads with
  `virtues pair` instead of promising digits on a display that never prints
  one.

Linked boxes from before the change needed nothing: they already held a relay
URL, and removing admission was invisible to them.

## The rule this earned

**When admission logic is deleted, its registry goes in the same change.**
Deleting a gate and keeping its data is strictly worse than keeping both or
dropping both: the liability stays and the justification leaves. For one day
atlas held a map from every EndpointId to an account that no code needed.

## What atlas is now

Accounts, billing, hosted-AI keys and update manifests. It holds no map from
an EndpointId to an account and sees no traffic, volume or timing. With
admission gone and checkout out of the airlock, setup no longer depends on
atlas or Stripe webhooks being up.

Do not describe the relay as "blind" in copy. It cannot read content, but it
does see which EndpointIds talk, from which addresses, and how much. The true
wording lives in `docs/operate/reach.md`.

The subscription's other half, offering it at first hosted-AI use rather than
in setup, was superseded by the setup stepper's required Subscription step.
That contradiction, and the relay dashboard, pairing and phrase follow-ups
this work named, are tracked in `agents/plan/account-plan.md`.
