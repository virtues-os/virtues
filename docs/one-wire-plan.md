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
  text, endpoint_id text, created_at)`. Billing auth
  (`resolve_active_customer`, credits.rs) checks `box_key` first, falls back
  to `customers.api_key_hash` (legacy rows keep working).
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

## Phase 1 — BLE reach-ticket refresh · stops the field trap · ~1.5 days

Root cause of the bench hang (see memory `reach-ticket-freeze`): the pair
consume ticket freezes `relay_url: null` when the box isn't relay-homed yet,
and the app has no channel to learn a later one on an isolating LAN — it
dials a dead direct IP forever, and `:7117` drops each connection with zero
bytes and zero messaging. Independent of everything else; ship first.

- **Box**: new Improv RPC **0x87 ReachTicket** — returns
  `box_reach_fields()` JSON via loopback `GET /api/devices/self/reach`
  (same pattern as 0x83's loopback consume in `ble_provision.rs`). Also:
  the 0x83 answer waits up to ~5 s for `ENDPOINT_UP` when the box is linked
  but mid-rebind, instead of answering with an absent ticket.
- **Box bugfix** (found in audit): `reconcile`'s late `ensure_relay_config`
  success never calls `request_rebind()` — a box that missed relay config at
  link stays `RelayMode::Disabled` until restart. One line.
- **Plugin**: `improv_reach_ticket` command; reach-client gains
  `update_reach(ticket)` — persist relay_url/addrs into the stored pairing
  and rebuild the warm client.
- **App**: on the post-pair "Opening your server" screen, when the stored
  ticket has no relay_url, poll 0x87 (bounded, alongside health) with honest
  stage copy; `reach_status` gains a reason string so "unreachable — ticket
  has no relay" can ever be *said*. proxy.rs stops silently accept-dropping:
  log + backoff.

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
- **Box**: verify the 2026-08-11 box-side 0x82 ClaimGrant state
  (`ble_provision.rs`); wire stored grant → outbound redemption on first
  connectivity.
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
