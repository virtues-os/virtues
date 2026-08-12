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

**2.2 Code TTL to ~15 minutes.** RFC 8628's normal window. Two minutes is
shorter than the walk from the box to a laptop, and it killed every attempt over
two days.

**2.3 Account sessions in the app.** Sign in once. This is the keystone: the app
becomes the authenticator.

**2.4 `0x82` end to end.** With a session, the app gets a pre-approved
`device_code` from atlas and hands it to the box over BLE; the box redeems it
outbound. The browser leaves the flow entirely for signed-in owners. Box side is
already built.

*After Phase 2 the intended flow is real: install, sign in, one screen with a
Wi-Fi password, done.*

## Phase 3 — join a claimed box · **ship gate**

One mechanism, three proofs ([paradigm §4–5](onboarding-paradigm.md)). Not a
recovery mode — the same join flow with different evidence.

**3.1 Trusted device vouches** (proof 1). Largely exists as Devices → Add. Make
it the named primary path, and let it work remotely: the vouching device is
already inside.

**3.2 Email, then panel** (proof 2). From a fresh app: enter the account email →
atlas sends a code → clicking it marks a recovery pending → the box learns by
polling → the panel shows a join code for a few minutes → type it. Ownership
first, presence second. **Ride the existing atlas heartbeat** (key renewal,
relay reconcile) rather than adding a poll: recovery-pending is a field in a
response the box already fetches. **No physical trigger** — an earlier draft used three
power cycles to open the window, which a houseguest could perform themselves and
which forced recovery to *begin* at the box. This ordering is strictly better
and deletes the mechanism.

**3.3 Recovery code** (proof 3) — the **offline** proof, and the reason 3.2 can
depend on the network. Verified locally, submitted over BLE, needing neither
atlas nor internet: it covers a box that moved house, one that never linked, and
a world without atlas. Generated at first pair, shown **in the app**, once.
Consequence-framed copy, *Save to password manager* as the primary action,
confirm by the last group. Regenerable from any trusted device. Stored hashed,
and never derivable by atlas.

**Why this gates shipping:** lockout is the single failure nobody can fix
remotely, for anyone, ever.

## Phase 4 — the rest

Defer the collectors screen; ask for a permission when a feature needs it, not
six times before the product has shown anything. `virtues.com/link` → redirect
to atlas. Store entitlement pre-provisioning, so a store buyer never sees a
payment screen.

## Ship-blockers outside the flow

None of this is onboarding design; all of it stops a real owner cold.

**The published apps predate everything here.** The App Store build and the
`mac-latest` DMG are from before the unified airlock, the desktop BLE client,
and the three-step display. Step 1 of the flow — "get the app" — currently hands
someone software that cannot perform steps 2 and 3. The ~95-commit `wave` →
`staging` backlog is the actual blocker, and it grows daily.

**The downloads page offers four Mac DMGs** (`mac-latest`, `1.0.17`, `1.0.20`,
`v0.1.0`). One link, current, or people install a random one.

**Atlas has no reviewable source of truth.** The deployed `/init` is ahead of
the repo checkout, which contains no `/init` at all — so its identity fork, its
2-minute TTL, and its error copy cannot be read, reviewed, or fixed from git.
Every atlas item in Phase 2 is blocked on this.

**The ambient display is a placeholder.** "Your box is keeping the record" is
the screen an owner sees ten thousand times.

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
