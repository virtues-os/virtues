# Onboarding — the build plan

> How we get from what exists today to [the paradigm](onboarding-paradigm.md).
> The paradigm says *what and why*, and is meant to be stable. This says *in what
> order*, and is meant to be crossed off.
> [linking-plan.md](linking-plan.md) is the deep dive on step 2.
> Written 2026-08-12.

## Where we are

"Proven" means it ran on hardware.

**Proven.** BLE Improv on the box: wifi scan, join, 802.1X. A desktop Improv
client (`virtues-improv`, btleplug) — a Mac drove a box onto enterprise wifi end
to end, no phone, no QR. One airlock (`connect.html`) for both platforms. The
three-step display. Device-authorization link flow, box side.

**Built, never observed.** The relay's **in-process rebind** on link — the claim
that reach activates in seconds without a restart, waiting on one completed
checkout. **`0x83`** pairing over BLE, which is not a nicety: a Mac and a box on
the same WeWork wifi cannot reach each other, so LAN pairing is unavailable in
any office. **`0x82`** claim grant, box side only.

**Missing.** Atlas `/init` has no identity fork, so an existing subscriber pays
twice; it ignores the box identity the box now sends, and expires codes in ~2
minutes. The app holds no account session, so it can vouch for nobody. The whole
*join a claimed box* story — no recovery code, no trigger, no emailed code —
which means an owner who loses their only device is locked out of data they
physically possess. Replacing a pairing is silent. There is no consumer factory
reset. `virtues.com/link` 404s.

## Phase 1 — collapse the flow

No atlas work, no account work. Nine steps to roughly four, this week.

**1.1 `0x85 GetLinkUrl`.** The link step's friction is that it spans three
surfaces: the app says "open the link page", the browser wants a code, the code
is on the panel, and it expires in two minutes. The box already holds
`…/init?code=…`. Hand it over the BLE session the app is already using and the
browser opens prefilled. Nothing read, nothing typed, no expiry race. *Cheapest
large win in the plan.*

**1.2 Consumer factory reset.** Today the only reset is `virtues reset` over
SSH, which an appliance owner does not have. This blocks resale, hand-me-downs,
and every recovery-from-mistake path — including `0x84` below. Small, and it
unblocks more than its size suggests.

**1.3 The app stops asking what it can answer.** Auto-select when exactly one
box needs setup. Prefill the network this machine is on; keep the password
field. Neutral headline until discovery decides — "Looking for your box", not
"Set up your box", which alarms someone who has owned one for months and is only
installing the Mac app.

**1.4 Confirm before replacing a pairing.** Single-box is a sound design; silent
replacement is not. Two pairings have already been lost this way.

**1.5 Watch the relay rebind.** One checkout, then read the journal. Until then
it is an untested claim.

### Deferred, with conditions: `0x84 PairDirect`

Pairing the setup device with no code at all, on the grounds that BLE range is
the proximity proof. **Held back**, because BLE range is not "in the room" — it
passes through walls at 10–30 m. A neighbour running our app could claim a box
in the window between plugging in and setup, and once linked they hold *relay*
access to a box sitting in someone's home. Ships only when (a) consumer reset
exists and (b) the panel announces claims loudly enough to notice
("Claimed by Adam's Mac"). `0x85` carries most of the win at none of this risk.

## Phase 2 — identity

The strategic phase, and the only one measured in weeks.

**2.1 Atlas `/init` earns its fork.** One email field. Render the box identity
the box already sends, so the page says *"Link **Honest Kestrel** · Dragon
Q6A"* — the anti-phishing property, not decoration. Then branch: active
subscription → one tap and **no payment screen ever**; otherwise checkout with
the email prefilled.

**2.2 ~~Code TTL~~ — already correct.** `LINK_TTL_MINUTES = 15` in
`routes/link.rs`, returned as `expires_in`. The two days of failed links were
the box-side ghost-code bug (`api/display.rs`), not atlas expiry. Left here as a
correction: the diagnosis was wrong, so re-open anything that rested on it.

**2.3 Atlas grows a minimal account API.** Deliberately deferred until now;
it is the gate for everything below.

*Less is needed than it looks.* The box is **already** an authenticated client
of atlas, so anything the app wants — billing, usage, wallet, subscription
management — it asks the box, and the box asks atlas with its own key. No user
session involved. Recovery does not need one either: atlas already knows the
customer's email from Stripe and can simply mail them.

What genuinely requires a session is the window *before* a box is linked:
paying in-app, and vouching for a link without a browser.

- **Email + a 6-digit code. Not a magic link** — links bounce you into a
  browser, which is the exact hop this phase exists to delete. A code typed
  into the app keeps everything in one place. No passwords, and no "sign in
  with Apple/Google": for a sovereignty product, handing identity to a third
  party is off-brand, and email is the one identifier everyone has that nobody
  holds on their behalf.
- **Opaque session tokens, not JWT.** 256-bit random, stored hashed, with
  expiry and revocation. JWT buys statelessness we do not need at this scale
  and costs instant revocation, which we do. Atlas has a database already.
- **Codes** hashed at rest, 5–10 minute TTL, single use, rate-limited per
  email. They are the front door.
- **Sessions** long-lived on mobile, revocable from the account page, never in
  a URL.
- Half of this exists: `/init/login` already does email → magic link.

*Data minimization holds.* This adds a session table and a code table — no new
personal data. Atlas already holds the email, the Stripe customer, the linked
box keys, and the wallet. The invariant is unchanged: **atlas knows who pays and
which boxes are theirs, never anything from inside a box.**

**Decide now: what "delete my account" means.** Proposed — atlas forgets you and
your box keeps working on the LAN forever. It is the sovereignty claim in its
most testable form.

**2.4 In-app payment.** Apple Pay / Stripe sheet inside the app rather than a
browser redirect.

**2.5 Grant over Bluetooth.** With a session, the app asks atlas for a
pre-approved `device_code` and hands it to the box over the Bluetooth session it
already holds; the box redeems it outbound. Box side is built and tested — it
has been waiting for an app with an account to vouch with.

**What Phase 2 buys, holistically: the browser appears once, to deliver the
app, and never again.** Today attention goes display → browser → app → browser →
app → display → app: seven hops across four surfaces, most of them errands to
fetch a string. After this it is display → browser → app → done. The display
says one thing, the browser does one thing, the app does everything else, and
every remaining handoff is a real change of purpose.

## Phase 3 — join a claimed box · **ship gate**

One mechanism, three proofs ([paradigm §4–5](onboarding-paradigm.md)). Not a
recovery mode — the same join flow with different evidence.

**3.1 Trusted device vouches** (proof 1). Largely exists as Devices → Add. Make
it the named primary path, and let it work remotely: the vouching device is
already inside.

**3.2 Power-button trigger.** The Dragon's board has a power button (#6).
`HandlePowerKey=ignore` in `logind.conf` plus a udev rule on `KEY_POWER` hands
the event to a recovery script — a ~15 minute spike to confirm. Long-press must
remain a real shutdown, and the button has to be reachable once the board is in
its enclosure.

The button is the **trigger**, never the proof: presence alone would let a
houseguest into a box holding someone's life. What it buys is that the box
stops polling — one outbound call when pressed, instead of a standing query
against our servers forever. That was the real weakness of the polling design
and the reason to prefer this.

**3.3 Emailed code** (proof 2). Press → one outbound call → atlas emails the
code to the **account address** → type it into the app → the box verifies.
Whoever pressed the button is irrelevant; the code goes to the owner's inbox, so
a houseguest achieves nothing but sending mail. The panel shows only "Recovery
started — check your email": no secret on glass.

**3.4 Recovery code** (proof 3) — the **offline** proof, used when the box has
no network or never linked. Same button, same window, but the code is one the
owner already holds and the box verifies locally: **atlas is not involved at
all**. Generated at first pair, shown **in the app**, once. Consequence-framed
copy, *Save to password manager* as the primary action, confirm by the last
group. Regenerable from any trusted device. Stored hashed, and never derivable
by atlas.

**Why this gates shipping:** lockout is the single failure nobody can fix
remotely, for anyone, ever.

## Phase 4 — the rest

Defer the collectors screen; ask for a permission when a feature needs it, not
six times before the product has shown anything. `virtues.com/link` → redirect
to atlas. Store entitlement pre-provisioning, so a store buyer never sees a
payment screen.

## Distribution

No users yet, so nothing here is about breakage or migration — only about what a
new owner is handed.

**`virtues.com/downloads` needs one link per platform.** It listed four Mac DMGs
(`mac-latest`, `1.0.17`, `1.0.20`, `v0.1.0`), which is how someone installs a
build from before any of this existed. One current Mac link, one App Store link.
The stale point releases and old staging prereleases were deleted on 2026-08-12;
what remains is the rolling channels (`mac-edge`, `mac-latest`, `win-edge`,
`linux-desktop-edge`, `edge`), the stable box tags, and `models-1`.

**Atlas deploy provenance.** Atlas *is* in source — `services/virtues-atlas/`,
with `/init/start|poll|done|login` in `routes/link.rs`. (An earlier draft of
this plan claimed otherwise; it was looking at a different repo, and nothing in
Phase 2 was ever blocked.) The real gap is that `make` builds `:latest` from the
working tree with no tag, sha, or CI — nobody can tell which build is running.

**Ship the backlog.** ~95 commits sit on `wave`; the published apps predate the
unified airlock, the desktop BLE client, and the three-step display. Not urgent
without users, but the gap only grows.

## Billing correctness

Found by review, 2026-08-12; none of it is onboarding, all of it is the money
path onboarding sells.

**Refunds are scoped to the customer, not the subscription.** `charge.refunded`
and `charge.dispute.created` both land in `set_status(..., "refunded")`, which is
`UPDATE subscriptions … WHERE stripe_customer_id = $1`. Top-ups are off-session
charges against that same customer, so refunding a $10 goodwill top-up marks the
$20/mo subscription refunded — 402ing the portal and top-ups, and dropping the
box off the relay. We do not choose to have refunds; disputes and support
gestures arrive regardless. Separate the objects: a refund against a **top-up**
debits the wallet, a refund or dispute against a **subscription invoice** changes
subscription status.

**Credits roll over** indefinitely while the subscription is active. Prepaid
money that expires is unfriendly, and in several US states legally fraught for
anything gift-card-shaped.

**Auto-top-up's off switch is unwired on the streaming path** — the main AI path
— so "off" still charges the card. `client.rs`'s `stream()` calls
`renew::auto_topup` with no `auto_topup_allowed()` guard, unlike the post/get
paths.

**Every recovery path for a lapsed subscription is gated on the subscription
being active**, including the billing portal — so the way to fix your payment
sits behind your payment. Add a grace window on `past_due` and un-gate the
portal.

**A second link destroys the first box.** `/init/login` resolving an existing
email deletes that account's device keys and rotates the api key, so box #1 goes
dark; a fresh link instead mints a *new* Stripe customer, which silently
double-charges. Multi-box is not an open question — it is a destructive default.

## Failure modes and degradation

The plan is BLE-centric; these are the paths it does not describe, and each one
is currently discovered in the field rather than designed.

| Condition | What should happen |
|---|---|
| Bluetooth off or blocked by policy | fall to panel codes — the documented floor |
| Android | no Improv client exists; panel codes only |
| Ethernet | step 1 disappears; box is online at first boot |
| Two unclaimed boxes in range | never auto-select; show both by codename |
| Subscription lapses | LAN keeps working. "Prescribe, never enforce" says so; nothing implements or tests it |
| Setup abandoned after wifi | the box currently mints a device-auth session every 15 min against atlas, forever. Needs backoff |
| Fresh box, no NTP | wrong clock breaks TLS to atlas and every expiry check. Guard explicitly |

**Observability.** Everything diagnosed in two days came from a live SSH
session. An owner who gets stuck produces nothing we can ask for. A local setup
log, readable in the app, would change every support conversation — and is the
difference between a bug report and a shrug.

## Known defects

Verified by the review pass of 2026-08-12 (five reviewers, each findings set
handed to a skeptic told to refute it). Nothing here has users; none of it is
urgent. Listed so it is not rediscovered.

**Fixed same day.** The Bluetooth-only pair path threw a TypeError and froze the
airlock — the client-isolated-office path `0x83` exists for, dead since it was
written. A failed rebind `return`ed out of the reach supervision task, leaving a
linked box with reach dead until a restart. `ClaimGrant` was accepted on an
already-linked box, so anyone in radio range could rebind it to their own
account. The account gate applied to DIY boxes.

**Phase 3 is further away than "largely exists" implied.**
- The Add-device modal already holds the six-digit token and renders only a QR
  and URL; the receiving field is a numeric input with no camera. The only
  working path is reading the digits out of the URL — which violates the
  paradigm's own rule about never asking someone to fetch what we already have.
- `format_pair_url` builds a LAN address, so "works from anywhere" does not.
- The reconciler drops the BLE service ~15s after first pair and has **no
  re-advertise entry point**, and it never runs on DIY boxes at all
  (`is_appliance()` guard). So the paradigm's "opens a window and re-advertises
  BLE" has nothing behind it, and proof 3 is LAN-only on DIY.

**Box / panel.**
- No retry state at screen 2: an atlas outage leaves "Reaching Virtues to start
  the link — a moment" on the glass indefinitely while the 2s heartbeat retries
  with no backoff.
- Abandoned setup polls ~17k times/day, not the ~96 mints the failure table
  implies. Back off the poll, not the mint.
- Auto-select takes the first box in setup state with no count check — the plan's
  own "never auto-select two unclaimed boxes" row is already violated.
- Screen 1 computes and serializes `ap_ssid`/`ap_passphrase` that nothing
  renders, while `setup_ap::spawn` still runs: a network whose credentials
  appear nowhere. Dead SoftAP residue.
- `subscription.rs` derives `is_active` from api-key *presence*, so the box
  reports "active" through lapse, rotation, deletion, and hijack.

## Open decisions

1. **Recovery code printed on a card in the box?** The router convention. Good
   for people who never open Settings; bad when the box is stolen with the card
   in the same drawer. *Leaning no for this data.*
2. **Second box in a household — joinable by a trusted device with no physical
   act?** Makes multi-box trivial; widens what a stolen phone reaches.
3. **Does the ambient panel ever offer "add a device"?** Trusted-device vouching
   may cover it, in which case the panel stays furniture.

## Done looks like

- A Mac, out of the box: install, sign in, one screen, box open. No codes.
- A phone joining later: one tap from the Mac, no physical act.
- Same owner, phone lost and Mac wiped: power-cycle, emailed code, back in.
- A DIY box that never linked: recovery code works with atlas switched off.
- Each exercised on hardware, on a hostile network.
