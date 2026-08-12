# Onboarding — the build plan

> How we get from what is built today to
> [the paradigm](onboarding-paradigm.md). Written 2026-08-12.
> The paradigm says *what and why* and is meant to be stable; this says *in what
> order and how much*, and is meant to be crossed off.
> [linking-plan.md](linking-plan.md) remains the deep dive on step 2.

## Where we actually are

Honest inventory. "Proven" means it ran on hardware, not that it compiles.

**Proven on hardware**
- BLE Improv service on the box; wifi scan + join, incl. 802.1X (`0x81`).
- Desktop Improv client (`virtues-improv`, btleplug). A Mac drove a box onto
  enterprise wifi end to end on 2026-08-12 — no phone, no QR, no portal.
- One airlock (`connect.html`) for desktop and mobile.
- Three-step display; captive-network honesty; codename identity.
- Device-authorization link flow, box side: start / poll / adopt-in-flight.

**Built, not yet observed working**
- Relay **in-process rebind** on link (`relay.rs` supervision loop). The claim
  is that a screen-2 link activates reach in seconds with no restart. Nobody
  has watched it happen; it needs one completed checkout.
- `0x83` PairConsume — pairing over BLE, box and app both written. Load-bearing
  at any office: a Mac and a box on the same WeWork wifi cannot reach each other
  (measured 2026-08-12), so LAN pairing is simply unavailable there.
- `0x82` ClaimGrant — box side only; the app and atlas legs do not exist.

**Missing**
- Atlas `/init`: no identity fork (goes straight to Stripe, so an existing
  subscriber pays twice), does not render the box identity the box now sends,
  and expires codes in ~2 minutes.
- The app holds no account session, so it cannot vouch for anyone.
- The setup device still types a pair code it has already earned the right to
  skip.
- **The entire "join a claimed box" story**: no recovery code, no power-cycle
  trigger, no emailed join code. A claimed box shows no code ever again, so an
  owner who loses their only device is locked out of data they possess.
- Replacing an existing pairing is silent and destructive.
- `virtues.com/link` 404s; the app and panel point at atlas directly as a
  stopgap.

## Phase 1 — collapse the flow with what we already have

No atlas changes, no account work. Gets the appliance + Mac path from nine steps
to about four, this week.

**1.1 `0x85 GetLinkUrl` — the box hands the app its verification URL.**
The link step's whole friction is that it spans three surfaces: the app says
"open the link page", the browser wants a code, the code is on the panel, and it
expires in two minutes. The box already *has*
`https://atlas.virtues.com/init?code=…` in `billing_link_inflight`. Hand it
across the BLE session the app is already holding and the app opens the browser
prefilled. Nothing is read, nothing is typed, no expiry window.
*Cheapest large win in the plan.*

**1.2 `0x84 PairDirect` — the setup device pairs with no code.**
Unclaimed boxes only, no code field: BLE range **is** the proximity proof, and
proximity alone is the correct bar to claim an empty box
([paradigm §3](onboarding-paradigm.md)). Kills the read-6-digits-and-type-them
round trip for the first device. A claimed box keeps requiring `0x83` + a code,
which is the tier-2 bar and stays untouched.

**1.3 App: stop asking questions it can answer.**
Auto-select when exactly one box needs setup; prefill the network this machine
is already on (keep the password field); neutral headline until discovery
decides ("Looking for your box", not "Set up your box" — the latter alarms
someone who has owned a box for months and is only installing the Mac app).

**1.4 Confirm before replacing a pairing.**
The store is single-box by design, and that is fine — but switching is currently
silent. "This Mac is connected to *Dragon*. Connecting to *Honest Kestrel* will
replace it." Two pairings have already been lost this way.

**1.5 Watch the relay rebind.** One completed checkout, then read the journal
for the rebind line. Until then it is an untested claim.

## Phase 2 — identity, so the account step stops being a browser trip

The strategic phase, and the one with real weeks in it.

**2.1 Atlas `/init` earns its fork.** One email field; render the box identity
the box already sends (`{name, label, model, endpoint_id}`) so the page can say
*"Link **Honest Kestrel** · Dragon Q6A"* — which is the anti-phishing property,
not decoration. Then branch: active subscription → one tap, **no payment screen
ever**; account without a sub, or no account → checkout with the email
prefilled.

**2.2 Raise the code TTL to ~15 minutes.** RFC 8628's normal window. The current
~2 minutes is shorter than the walk from the box to a laptop, and it killed
every attempt over two days.

**2.3 Account sessions in the app.** Sign in once, in the app. This is the
keystone for everything after it: the app becomes the authenticator.

**2.4 `0x82` ClaimGrant end to end.** With a session, the app asks atlas for a
pre-approved `device_code` and hands it to the box over BLE. The box redeems it
outbound; the browser disappears from the flow entirely for signed-in owners.
Box side is already built and tested.

**At the end of Phase 2 the intended flow is real:** install, sign in, one screen
with a Wi-Fi password, done.

## Phase 3 — join a claimed box

One mechanism, three proofs ([paradigm §4–5](onboarding-paradigm.md)). Nothing
here is a "recovery mode"; it is the same join flow with different evidence.

**3.1 Trusted device vouches** (proof 1). Mostly exists as Devices → Add; make
it the named, primary path and let it work remotely, since the vouching device
is already inside.

**3.2 Power-cycle trigger.** Three power cycles within thirty seconds opens a
two-minute window; BLE re-advertises. Detection is a small ring of boot
timestamps in the state root. **No secret appears on the panel** — it says only
"Recovery started, check the email on your account".

**3.3 Emailed join code** (proof 2). The box mints the code, asks atlas to
deliver it to the account email (outbound, as always), and verifies it itself.
Atlas carries; atlas never authorizes. Presence is still required, which is
exactly what makes atlas-carries acceptable.

**3.4 Recovery code** (proof 3). Generated at first pair, shown **in the app**,
once — never on the panel. Consequence-framed copy, *Save to password manager*
as the primary action, confirm by the last group. Regenerable from any trusted
device. Stored hashed. The only proof an unlinked DIY box has, and the only one
that survives atlas not existing.

**Ship gate:** Phase 3 is not optional before real customers own boxes. Lockout
is the one failure that cannot be fixed remotely, for anyone, ever.

## Phase 4 — the rest

- Defer the collectors screen; ask for a permission when a feature needs it, not
  six times before the product has shown anything.
- `virtues.com/link` → redirect to atlas `/init`.
- Store entitlement pre-provisioning, so a store buyer never sees a payment
  screen (persona 1 in [linking-plan.md](linking-plan.md)).

## Open decisions

1. **Recovery code on a printed card in the box?** The router convention. Good
   for the majority who never open Settings; bad when the box is stolen with the
   card in the same drawer. *Leaning no for this data.*
2. **Second box in a household — joinable by a trusted device with no physical
   act?** Makes multi-box trivial; widens what a stolen phone reaches.
3. **Does the ambient panel ever offer "add a device"?** Trusted-device vouching
   may cover it entirely, in which case the answer is no and the panel stays
   furniture.

## What done looks like

- A Mac, out of the box: install, sign in, one screen, box open. No codes.
- A phone joining that box later: one tap from the Mac. No physical act.
- The same owner, phone lost and Mac wiped: power-cycle, email code, back in.
- A DIY box that never linked: recovery code still works, with atlas switched
  off entirely.
- Every one of these exercised on hardware, on a hostile network, not just in a
  test.
