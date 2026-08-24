# One wire — finishing the setup flow's keystone

> Written 2026-08-24 after a full bench day and a four-agent audit of the
> atlas ↔ app ↔ box coordination. The verdict that produced this plan: the
> individual setup steps are justified and documented; the overengineering is
> the **wire interleave** — BLE → atlas → BLE → LAN → BLE → iroh — which
> exists only because the keystone [linking-plan.md](linking-plan.md) already
> prescribes (RPC 0x82 ClaimGrant) was built box-side on 2026-08-11 and never
> finished app/atlas-side. Every bench failure that day was a symptom: a BLE
> session dying under a minutes-long OTP, a pair write hanging on the dead
> link, a reach ticket frozen with `relay_url: null` against a
> client-isolating LAN.
>
> The target flow, stated once: **sign in first (Mac ↔ atlas, box not
> involved), then one Bluetooth session carries everything** — Wi-Fi, a
> pre-approved grant the box redeems outbound when it comes online, a
> codeless session-authorized pair, and the reach ticket back. Zero codes
> fetched or typed on the first-device path. Every code that is a device's
> *only* channel (second devices, CLI, LAN, browser checkout) survives
> untouched. Skip stays everywhere — prescribe, never enforce.

## Why sign-in can precede Wi-Fi

The Mac has its own internet; sign-in is Mac ↔ atlas only. The box needs
Wi-Fi only to *redeem* its credential — and with a grant delivered over BLE,
its first breath of internet already carries everything it needs. Nobody
polls, nobody waits. Today's flow has this backwards: the box must get
online first to mint a code the app reads over BLE and carries to atlas —
the round-trip the whole afternoon was lost in.

## Phase 0 — per-box keys (atlas) · the prerequisite · ~1–2 days

The landmine (documented in `routes/account.rs`): atlas holds **one**
`customers.api_key_hash` per customer, and every attach rotates it — a
second box linking silently kills the first box's credential. This exists
today; the grant just makes it the default path. virtues-api's half is
**already built** (`register_device` accepts `box_id` and scopes key
replacement per-box). Missing is atlas-side only:

- **Migration 0015**: `box_key (api_key_hash bytea PK, stripe_customer_id
  text, endpoint_id text, created_at)`. ALL FOUR auth-by-hash read sites
  (`billing_portal.rs:87`, `credits.rs:216`, `settings.rs:66`, `:117`) check
  `box_key` first, falling back to `customers.api_key_hash` (legacy rows
  keep working); both write sites (claim insert, link rotation) move to
  `box_key`.
- **`endpoint_id` is a rotation-scoping LABEL, never an authorization
  input** — it is self-reported on an unauthenticated call. A forged one can
  mislabel a key; nothing may ever be granted or denied because of it. State
  this in the code where the column is written.
- **Box identity at attach**: `/init/start` body gains the box's iroh
  `endpoint_id` (the box knows it pre-relay — relay.rs publishes it before
  bind); `device_link` stores it; `attach_link_to_customer` and
  `finalize_paid_session` pass it as `box_id` to `register_device` and write
  `box_key`, deleting only that endpoint's prior rows. `endpoint_id` is
  optional in the body — an old box degrades to the legacy whole-account
  rotation.
- **The killer test**: two boxes on one account; rotate one; assert the
  other's key still authenticates against both virtues-api and atlas
  billing-auth.

Do this now, while prod has effectively one customer and migration is free.

## Verified facts the plan bends around (2026-08-24 code check)

- **BLE dies 0–15 s after claim.** The Improv service tears down on the next
  15 s reconcile tick once a device pairs (`ble_provision.rs:204-238`;
  doctrine at :24). A post-pair RPC has only a race window — so nothing may
  be designed to run over BLE *after* pairing. The consequence: **pair goes
  last in the session, and its answer must be complete.**
- **0x82 is already fully built box-side** — parser + tests
  (`protocol.rs:188,240-245`), session-gated handler that stores the grant,
  redeems when online, and notifies `"linked"` back over BLE
  (`ble_provision.rs:593-622,860+`). Phase 2's box work is ~zero.
- **Unknown opcodes fail fast and clean** (`UnknownCommand` on the error
  characteristic, connection stays up — `protocol.rs:281`,
  `ble_provision.rs:472-477`), so capability fallback on old firmware is an
  immediate error, not a timeout.
- **Atlas auth-by-key has four read sites** to sweep in Phase 0:
  `billing_portal.rs:87`, `credits.rs:216`, `settings.rs:66` and `:117`
  (plus the two write sites, `claim.rs:222-231` and `link.rs` rotation).

## Phase 1 — complete-ticket-at-pair · stops the field trap · ~1.5 days

Root cause of the bench hang (see memory `reach-ticket-freeze`): the pair
consume ticket freezes `relay_url: null` when the box isn't relay-homed yet,
and the app has no channel to learn a later one on an isolating LAN — it
dials a dead direct IP forever, and `:7117` drops each connection with zero
bytes and zero messaging. Independent of everything else; ship first.

Because BLE dies 0–15 s after claim (verified above), the fix is NOT a
post-pair refresh RPC — it is making the pair answer itself complete, and
never pairing before the ticket can be:

- **Box**: the 0x83 answer waits (up to ~10 s) for `ENDPOINT_UP` when the
  box is linked but mid-rebind, instead of answering with an absent or
  relay-less ticket. New RPC **0x87 ReachTicket** (loopback
  `GET /api/devices/self/reach`, 0x83's pattern) exists for *pre-pair* use —
  the app confirming the ticket is complete before it commits the pair —
  and is session-gated like 0x84.
- **Box teardown grace**: gate the claimed-teardown on "no live setup
  session" with a ~10-minute cap, so a session that paired seconds ago isn't
  cut mid-conversation by the 15 s tick. (Belt; pair-last is the braces.)
- **Box bugfix** (found in audit): `reconcile`'s late `ensure_relay_config`
  success never calls `request_rebind()` — a box that missed relay config at
  link stays `RelayMode::Disabled` until restart. One line.
- **Plugin**: `improv_reach_ticket` command; reach-client gains
  `update_reach(ticket)` — persist relay_url/addrs into the stored pairing
  and rebuild the warm client. Used before pair, and by the today's-flow
  retrofit below.
- **App (retrofit for today's flow)**: before auto-pair fires, if the box
  reports linked but the pending ticket lacks a relay, poll 0x87 briefly and
  only then pair. `reach_status` gains a reason string so "unreachable —
  ticket has no relay" can ever be *said*; proxy.rs stops silently
  accept-dropping (log + backoff).
- Skipped-link boxes get no relay by definition (LAN-only is their story);
  nothing here waits on one for them.

## Phase 2 — the 0x82 grant · sign-in first · ~2–3 days

- **Atlas**: `POST /init/grant` (account_session bearer; entitled required;
  reuse approve's guards + the 0014 rate limit). Mints a `device_code`
  **bound to the customer, attach deferred to redemption**: the box redeems
  via `/init/poll {device_code, endpoint_id}`, and *that* is when atlas runs
  the attach (per-box key minted with the redeeming box's endpoint_id — this
  is why Phase 0 comes first, and why grant-time not knowing the box is
  fine). Grant TTL generous (~24 h): it is pre-authorized to a specific
  account and delivered over a proven line-of-sight channel; a box slow to
  get online must not strand anyone.
- **Box**: nothing — 0x82 is verified complete (stores the grant, redeems
  outbound via the standard `/init/poll` when online, notifies `"linked"`
  over BLE). The session ordering writes itself: wifi + grant → watch the
  join → watch the `"linked"` notify → **pair last**, whose answer now
  carries a complete reach ticket (Phase 1's ENDPOINT_UP wait).
- **Entitlement re-checked at redemption**, not only at grant: a 24 h grant
  can outlive a refund. The poll-side flip runs the same entitled guard the
  approve endpoint uses.
- **Checkout-without-code** (new atlas work the flow needs for first-time
  buyers): today's checkout binds to a box link code that no longer exists
  at sign-in time. Add a session-authed checkout that just creates the
  subscription; the webhook flips `entitled`, the app notices via
  `/account/session`, then proceeds to grant. Until store pre-provisioning
  lands, new buyers still make one browser trip — earlier and cleaner, but
  present.
- **Plugin**: `improv_grant` command (already listed as owed in
  linking-plan).
- **Airlock**: sign-in screens (email → OTP, unchanged visuals) move to
  right after the save ceremony, **before Wi-Fi**, with a quiet "Skip for
  now" (skipped = no grant written; box works LAN-only; linkable later from
  app, browser, or `virtues link`). After the Wi-Fi join: write grant over
  the live session, done. The link step screen, 0x84 fetch/cache/watchers,
  `state.linkCode`/`signinCode`, and the approve/retry/rephrase screens are
  all deleted.

## Phase 3 — codeless session pair · ~1 day

The 6-digit code on the BLE path is vestige: the phrase already proved
line-of-sight, and today the app fetches the code over 0x85 only to hand it
straight back over 0x83. New session-authorized pair (0x83 variant or 0x88):
no code, same consume JSON + reach ticket in the answer. **Keep the 6-digit
code everywhere it is a device's only channel**: second devices, CLI, LAN
pair — unchanged. Add the deliberate revision note to
onboarding-paradigm.md: one session-start proof now authorizes the session's
whole conversation (same room, same minutes; a modest, named widening).

## Phase 4 — airlock cleanup · ~0.5 day

Open the SPA immediately after pair (no 95 s health gate — the loopback
provider already holds and retries; the SPA shows its own connecting state).
Demote the AP-breakglass probing out of the hot `start()` path (reachable
via "Connect by address" only). Delete the dead machinery Phase 2 orphaned.

## Compatibility

Old firmware in the field (and the v0.1.2 master) lacks 0x87/0x82/codeless
pair. The app probes per-RPC and falls back to today's flow — the current
code paths stay behind capability checks until the fleet catches up. Old
app + new box: unchanged; every existing RPC survives.

## Order, releases, verification

P0 → P1 → cut a staging.N (P1 alone fixes the field trap) → P2 → P3 → P4 →
staging.N+1. After every phase: the full bench rehearsal — deprovision over
UART, power-cycle, complete setup — plus the isolating-LAN scenario at P1
(link via console → 0x87 refresh → relay reach) and the two-box key test at
P0. The atlas pieces deploy via `make deploy-atlas` + SSM recreate
(deployment.md); airlock pieces ride the app build.

## What this deletes, in numbers

~250 lines of link-code rotation scar tissue in connect.html, two
completion-watch loops, four re-read-the-code call sites, the 0x85→0x83
hand-the-secret-back dance, and every failure mode of the form "a radio
session died while a human was reading email."
