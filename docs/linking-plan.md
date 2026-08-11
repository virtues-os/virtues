# Linking — Step 2 of 3

> The plan for the account-link step of appliance onboarding: narrative, flow,
> UX, security, technology, design, and the decisions still open. Written
> 2026-08-11, the day linking's absence was discovered as a wall: a box
> onboarded and paired at an office was unreachable by the app in the same
> minute, because reach rides the relay and the relay rides the account.
> Companion to [onboarding.md](onboarding.md).

## Where it sits

```
1. WIFI    over BLE               the box gets a network       (built, proven)
2. LINK    to an account          the box gets reach + billing  (this doc)
3. PAIR    with the on-screen code the owner's device gets access (built, proven)
```

Linking comes **before** pairing, and the ordering is a discovery, not a
preference. Linking needs only box-outbound internet plus the owner's phone on
*any* network — it is immune to hostile LANs by construction. Pairing today
needs a shared LAN. Do them in the other order and a dorm/office box ends up
paired-but-unreachable (observed live); in this order, the relay exists by the
time anything needs it, and the local network's goodwill stops mattering.

## Core narrative

Plain words on every surface: **get online → link your account → pair your
phone.** Step 2 is where generic hardware becomes *someone's* box. The account is
**billing + reach, never data custody** — that sentence is doctrine, UI copy,
and security invariant all at once, and it appears verbatim on the link page:

> *Your data lives on your box. Your subscription pays for what connects it:
> remote reach, the AI wallet, integrations.*

Money appears exactly once in onboarding, here, and only for people who
actually owe it (see personas). Tone: dignified confirmation, not SaaS upsell.
The display asks for one verb per screen; this screen's verb is **Link**.

## The flow

Device authorization, RFC-8628-shaped, already implemented box-side
(`virtues_api::link`): the box asks atlas for a `device_code` (secret, stays in
the DB) + `user_code` (human, `58YE-E24F`); the display shows the code and a QR
of `verification_uri_complete`; the box polls until redeemed; success stores
the api key in the vault. **No callback exists and none may be added** — the
box speaks outbound only; atlas must never gain the ability to reach into a
box.

The human-facing closure is physical: within one poll interval of finishing on
the phone, **the box's screen advances to the pair code**. The thing you just
claimed visibly wakes up. The atlas success page points at it: *"Done — Honest
Kestrel is showing a pairing code now. Open the Virtues app."*

```
display S2: QR + code ──scan──▶ atlas /init?code=…
                                   │ names the box: "Honest Kestrel · Dragon Q6A"
                                   ▼
                             sign in (email-first, OTP; account creation implicit)
                                   ▼
                       ┌─── entitled? ────────────────┐
                 yes (store purchase,            no (gift, resale,
                  existing sub)                   fresh account)
                       │                               │
                 one-tap "Link"                   Stripe checkout
                       └──────────────┬────────────────┘
                                      ▼
                        success page: "look at your box"
                                      ▼
                box poll → Ready → api key in vault → relay binds
                                      ▼
                          display S3: the pair code
```

## Personas — the branch that decides everything

1. **Store buyer (the intended majority).** Bought box + subscription at
   virtues.com. They already have an account and already paid. Link = sign in →
   *"Link Honest Kestrel?"* → tap. **If this persona ever sees a payment
   screen, the step has failed** — it reads as being charged twice. Requires
   the store to pre-provision the entitlement at purchase (P2 work, atlas
   side).
2. **Fresh account (gift, resale, DIY-appliance).** Sign-in creates the account
   implicitly (email OTP — no password ceremony); checkout; link.
3. **Existing member, additional box.** Sign in → covered → tap.
   **Policy decision (open, recommendation below): one subscription covers the
   account's boxes.** Usage caps already meter the real marginal cost (AI
   spend); boxes are endpoints; per-box pricing punishes the best customers.
4. **BYO-AI members.** BYO is a setting inside the one subscription, not a
   cheaper tier ([pricing doctrine](../docs)); BYO users still link — the key
   still buys relay reach and integrations. Nothing branches here.
5. **DIY free.** Never sees this step. No display, no forced sub; LAN + BYO
   transport (the doctrine's auto-noticed overlays) are their reach story.
   `virtues status` prints link state and the URL for the ones who want in.

## Prescribe, never enforce

The display *sequences* — no code is shown until linked — but the **app can
always pair regardless** (its pair path does not check `linked`). This is
deliberate and load-bearing: a soft-force. Support cases, refunds-in-progress,
dev boxes, and atlas outages must never brick a physically-owned machine at
screen 2. The screen is the guided path; the app is the sovereign fallback.
An appliance owner who refuses the subscription owns slower, LAN-only
hardware — not a brick.

## Security

- **Proximity remains the authority.** The `user_code` appears only on
  physical surfaces (display, box TTY). Seeing it = standing at the box — the
  same argument as the pair code.
- **Code-phishing, both directions.** Attacker shows *their* code to a victim
  (victim's card funds attacker's box): mitigated by the page **naming the
  box** — codename + model, sourced from the `/init/start` payload (P1: add
  identity to that payload) — plus short expiry and rate-limited code entry.
  Attacker links a *victim's* box: requires reading the victim's display;
  proximity again.
- **Box identity at atlas = the iroh EndpointId** (self-certifying pubkey).
  `deprovision` re-mints it, so a resold box is a *new* box to atlas — no
  lingering linkage. Atlas refuses to link an EndpointId already linked until
  the owning account releases it (P3: `virtues unlink` + account-page
  release).
- **Key custody + scope.** The api key lives in the vault (`box_secrets`,
  encrypted) and buys billing, wallet, relay config — it cannot read record
  data; no server surface exchanges it for data access. Atlas compromise
  blast radius: billing disruption and relay disruption. Not data. Keep it
  provably so.
- **Payments** are Stripe-hosted; card data never nears box or atlas beyond
  Stripe's own tokens.

## Technology — built vs. owed

**Built today (2026-08-11):** display screens resequenced 1/2/3; `linked`
gate (api key presence) in display state; screen-2 QR + code; the display's
2-second setup heartbeat lazily starts, caches, and polls the device-auth
session (rate-limited to atlas's requested interval; session rotates on
expiry; stops at linked/claimed).

**P1 — box-side (BUILT 2026-08-11, all three):**
- **Relay live-bind on link.** `relay.rs` is now a bind→serve→rebind
  supervision loop; `link::poll` requests a rebind after storing relay
  config, so a screen-2 link activates relay reach in seconds, in-process.
  Same EndpointId, same pinned port — only the homing changes.
- **Box identity in `/init/start`** — the box now POSTs `{name, label,
  model, endpoint_id, version}`; atlas-side rendering is P2.
- **Captive-network hint on screen 1** — display state carries the nmcli
  `connectivity` verdict + `wifi_ssid`, and screen 1 names portal/limited
  joins ("wants a browser sign-in") instead of silently reading as offline.

**P1.5 — the claim grant over BLE (box-side BUILT 2026-08-11):**
The keystone that merges wifi + link into one tap. Vendor RPC `0x82
ClaimGrant [grant]`: the signed-in app asks atlas for a pre-approved
`device_code` and hands it to the box over the same BLE session as the wifi
credentials (either order). The box stores it as its in-flight link
(`link::inject_grant`) and a redeem task polls it to `Ready` the moment the
box gets online — reusing the whole QR-path chain (api key → relay config →
endpoint rebind). The display loop *adopts* an existing in-flight link
instead of minting a new session over it (`link::inflight`), so the two
drivers can never orphan each other's device_code. Box stays outbound-only;
the grant inherits BLE's proximity argument.

Still owed for the grant to fire end-to-end (P2, app + atlas):
- atlas: app-authed `POST /init/grant` minting a pre-approved device_code
  (requires app sign-in / account sessions — the same work as the `/init`
  page personas).
- app: reach-plugin `improv_grant` command + ImprovClient 0x82 write, sent
  automatically after join when the app holds a session.

**P2 — atlas-side:**
- The `/init` page: personas, box naming, implicit account creation, the
  entitled/not branch, success page pointing back at glass + app deep link
  (`virtues://` scheme — the user provably has the app by step 2).
- Store checkout pre-provisions the entitlement (persona 1's no-payment
  guarantee).
- Two-sided confirmation (nice-to-have): atlas sees the box's redeeming poll,
  so the page can show "✓ your box received it" — closing the loop on both
  surfaces.

**P3 — lifecycle:**
- `virtues unlink`, account-page release, resale story documented atop
  deprovision.
- App awareness: chips could read `linked` from `/api/box/identity` and show
  "ready to link" vs "ready to pair" states.

## Design

The three screens must read as one system: split layout, one verb headline,
mono for things typed or scanned, the box's codename in the corner of every
screen (two-box lesson). No spinners, no animation — the panel is furniture
(24/7 doctrine). Screen 2's code renders large (`58YE-E24F` class strings);
the QR carries `verification_uri_complete` so scanning skips typing entirely,
and the URL + code beneath serve the no-camera case.

The atlas page inherits the product's restraint (white-on-white, hairlines,
serif headline, single accent — the established micro-craft direction), not a
generic checkout theme. It is the first Virtues *web* surface most owners see;
it should feel like the display's sibling.

## Testing plan (dev@virtues.com)

Set up as a Workspace test account; Stripe test mode. Walk, in order: fresh
account + checkout; existing-member second box (the multi-box policy branch —
today's real-world case); store-purchase simulation (entitlement
pre-provisioned, assert no payment screen); code expiry + rotation on glass;
deny/cancel; atlas-unreachable (screen 2's retry state); complete-on-desktop
(any browser, not just phone). Each run ends with the physical check: the
display advanced, the relay bound, the app reaches the box from a hostile
network.

## Open decisions

1. **Multi-box policy** — recommend: one sub covers the account's boxes; caps
   meter usage. Needs a real decision before P2.
2. **Store entitlement plumbing** — purchase-time account + entitlement
   creation; shape of the attach.
3. **Restart vs. in-process rebind** for relay activation (P1 either way).
4. **`virtues.com/link`** as the printed/memorable entry URL → redirects to
   atlas `/init`.
