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

## Phase 1 — one screen, one password

**The deliverable, stated as the thing a person does:** install the app, sign in
once, and then a single screen — *"Set up Honest Kestrel on your-wifi?"* — with
one password field and one button. No codes, no browser, no step counter. The
box opens.

Everything below exists to make that screen possible; nothing else is Phase 1.

**1.1 Consumer factory reset.** The gate for all of it. Today the only reset is
`virtues reset` over SSH, which an appliance owner does not have — so a lost
setup race, a resale, a hand-me-down, or any mistake has no way back. Small, and
it unblocks more than its size.

**1.2 Atlas: sessions.** *(done — `routes/account.rs`)* Email, six digits,
opaque revocable token. A session may exist before payment.

**1.3 Per-box keys.** *(virtues-api done — `device_keys.box_id`)* Atlas still
passes `box_id: None` because it learns a box's EndpointId through
`/iroh/register`, a different call than the one that mints the key. Join those
two and the grant can be scoped to a box, which is what stops a second box
evicting the first.

**1.4 `POST /init/grant`.** Session-authed, mints a pre-approved `device_code`
for a named box. Blocked on 1.3 — deliberately, since making a second link one
tap while it still destroys the first box would be worse than not building it.

**1.5 In-app payment.** Apple Pay / Stripe sheet in the app. Money appears once,
before the box is ever touched, and only for people who owe it.

**1.6 `0x84 PairDirect`, bound to the setup session.** See below.

**1.7 The app: one screen.** Sign-in, then the single setup screen with three
ticks as they land (joined / linked / paired). The Wi-Fi picker offers "already
on ethernet" as an option — wiring the box removes a password, not a stage. Auto-select when exactly one box
needs setup; prefill the network this machine is on; neutral headline until
discovery decides — "Looking for your box", not "Set up your box", which alarms
someone who has owned a box for months and is only installing the Mac app.

**1.8 The panel narrates the session** — "Setting up with Adam's Mac…" — so a
lost race is visible while it happens rather than discovered later.

**1.9 Confirm before replacing a pairing.** Single-box clients are a sound
design; silent replacement is not. Two pairings have already been lost this way.

### `0x85 GetLinkUrl` — cut

Handing the app the box's `…/init?code=…` so a browser opens prefilled. It was
the cheap fix for a three-surface link step that no longer exists: with sign-in
and the grant (1.4), no link URL is ever opened during setup. Dropped.

### The display loses its step counter

One setup screen — "Get the Virtues app" — then the live session narration, then
ambient. No 1/2/3, no link code, no pair code. The panel shows a code again only
for a *recovery* window (Phase 3), which is a different tier with different
stakes.

## Phase 2 — the account path, for people not setting up a box

Phase 1 removes the browser from setup entirely. What remains on the web is the
account itself: someone who bought hardware and a subscription together, someone
changing a card, someone who wants to sign in from a desktop. The `/init` page
survives for that, not as a setup fallback.

**2.1 Atlas `/init` earns its fork.** One email field. Render the box identity
the box already sends, so the page says *"Link **Honest Kestrel** · Dragon
Q6A"* — the anti-phishing property, not decoration. Then branch: active
subscription → one tap and **no payment screen ever**; otherwise checkout with
the email prefilled. Without this an existing subscriber pays twice.

**2.2 ~~Code TTL~~ — already correct.** `LINK_TTL_MINUTES = 15` in
`routes/link.rs`, returned as `expires_in`. The two days of failed links were
the box-side ghost-code bug (`api/display.rs`), not atlas expiry. Left here as a
correction: the diagnosis was wrong, so re-open anything that rested on it.

**2.3 Decide what "delete my account" means.** Proposed — atlas forgets you and
your box keeps working on the LAN forever. It is the sovereignty claim in its
most testable form, and it is cheap to settle now and expensive later.

*Data minimization holds throughout.* Sessions added a session table and a code
table — no new personal data, since atlas already holds the email for Stripe.
The invariant is unchanged: **atlas knows who pays and which boxes are theirs,
never anything from inside a box.**

## Phase 3 — the phrase · **ship gate**

Not a recovery system. One secret, generated at first boot, that gates every
claim after the first ([paradigm §1–3](onboarding-paradigm.md)).

**3.1 The phrase.** Four words from a wordlist, minted on first boot, stored
**hashed** on the box (verified, never recovered). Shown on the panel *only while
the box is unclaimed and empty*; never again after the first claim, including
after a reset — that asymmetry is the entire security argument.

**3.2 Setup requires it.** The BLE setup session is claimed with the phrase; only
that live connection may link and pair. Rate-limited, because four words over a
radio is guessable if you let someone guess forever.

**3.3 Reset setup, on the button.** `HandlePowerKey=ignore` + a udev rule on
`KEY_POWER` → forget paired devices, unlink the account, forget the network.
**Data and phrase survive.** So a resetter gets a box they cannot claim, and the
owner gets their box back by typing what they saved.

**3.4 Erase, not on the button.** Wiping the record for resale is an
authenticated action in the app (CLI for DIY). A physical gesture that destroys a
life record is a vandalism vector.

**3.5 The app's save ceremony.** Straight after the phrase is entered and
*before* setup runs: "Save this — it's how you get back in", with copy and
save-to-password-manager as the primary action.

**Why this gates shipping:** without it, anyone who can open the case owns the
box. With it, they can only inconvenience you.

**Deleted by this phase**, and left here so it is not rediscovered: the
three-proof model, the emailed recovery code, the power-cycle trigger, and the
separate recovery code. One mechanism replaced four.

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
