# Onboarding, identity, and trust — the paradigm

> The settled model for how a box gets online, whose it is, and which devices
> may read it. Written 2026-08-12 after two days of building the pieces on
> hardware and discovering the shape by hitting its walls. This document is the
> reason the flow looks the way it does; [onboarding.md](onboarding.md) is what
> is built, [linking-plan.md](linking-plan.md) is the account step in detail.
>
> Intended to be stable. If a change contradicts something here, the change is
> probably wrong — or this document needs a deliberate revision, not a quiet
> exception. The build order that follows from it is
> [onboarding-plan.md](onboarding-plan.md).

## 1. Three relationships, hence three steps

Onboarding establishes three facts that have nothing to do with each other:

| Step | Fact | Between |
|---|---|---|
| **Get online** | the box can reach the internet | box ↔ network |
| **Your account** | someone pays for what connects it | box ↔ atlas |
| **Your devices** | these devices may read the record | box ↔ device |

Three steps is irreducible; the flow can only make each one cheap. Plain names
on every surface — a person who has just unboxed hardware should not have to
learn a vocabulary to turn it on.

**Order is forced, not chosen.** The account step needs the network. Reach
rides the relay and the relay rides the account, so pairing before linking
produces a box that is paired and unreachable — observed live at an office on
2026-08-11, which is how the ordering was discovered rather than decided.

## 2. Every code exists because two machines could not talk

This is the source of nearly all the friction, and it is worth stating baldly:
a code on a screen that a human retypes elsewhere is a **substitute for a
missing channel**. The pair code and the link code both exist because the box
historically had no way to speak to the thing being set up.

It does now. BLE (Improv, incl. our vendor RPCs) is a proven, proximate,
box-to-client channel.

> **The app is the courier. Codes are the fallback.**

- App present, Bluetooth available → **zero codes**. The app carries the
  account grant to the box (`0x82`), carries the pairing back (`0x83`), and
  opens the browser at a URL the box handed it — nothing is ever retyped.
- No app, no Bluetooth, a desktop-only household, or recovery → **the panel
  shows codes**, exactly as it does today. This path is not deprecated; it is
  the floor, and it must keep working.

A corollary that settles a recurring argument: **the app must never instruct
someone to go read something it could have fetched itself.** If a screen says
"type the code from your box", ask first whether the app could have obtained it.

## 2b. The setup session is the trust anchor

The corollary above asks whether the app could have fetched a code itself. The
answer, for a box being set up, is that it should not need one at all — and the
reason is stronger than proximity.

An app that has just walked a box onto a network is not merely *nearby*. It
**configured** the box, over a connection that is still open. That is a fact the
box can check, and it is much harder to forge than radio range.

> **The first session to complete a wifi join becomes THE SETUP SESSION.**
> Only that live connection may link and pair without codes.

Three properties make it work:

- **It is a connection, not a device.** Not "the first client that ever talked"
  — the one currently configuring this box. Drop the link and the privilege dies
  with it; the next attempt starts over.
- **It survives the network change.** Bluetooth is a separate radio from wifi,
  which is precisely why provisioning rides it. The conversation continues
  through the switchover that used to end it.
- **Beating it is loud.** An attacker cannot simply be in range; they must win
  the wifi step, and then the owner's own setup visibly fails in front of them.
  Contrast a stolen code, which fails silently and later.

**So all three steps become one conversation** — wifi, account grant (`0x82`),
pairing — and therefore one screen:

```
    Setting up Honest Kestrel
      ✓ Joined your-wifi
      ✓ Linked to your account
      ✓ Paired
    → opens
```

The three relationships of §1 still happen and are still distinct. They stop
being three *screens*.

**The panel narrates the session live** — *"Setting up with Adam's Mac…"* — so a
lost race is visible while it happens, on the owner's own hardware, rather than
discovered afterwards. This replaces the weaker "announce the claim after the
fact" idea.

**What it depends on.** Sign-in happens in the app *before* the box is touched,
so the grant is ready to hand over in the same breath — which is why account
sessions come first in the build order. And a consumer factory reset must exist,
because if a race is ever lost the owner needs to take their box back without a
shell.

**What still uses codes**, unchanged: an ethernet box (no wifi join, so no setup
session — it is online at first boot and the panel shows a code), any client
without Bluetooth, DIY boxes (which never advertise, and whose owner has a
terminal), and every *join* of an already-claimed box, which is a different tier
entirely (§3).

## 3. Two tiers of device trust

Proximity is sufficient to claim an **empty** box and insufficient to enter a
**full** one. Same gesture, wildly different stakes, so they get different bars.

| | Bar | Why |
|---|---|---|
| **Claim** an unclaimed box | proximity alone | the box holds nothing; the worst case is someone puts a stranger's box on their wifi |
| **Join** a claimed box | proximity **and** proof of ownership | it now holds continuous audio, location, health, finance |

The consumer-appliance norm (Home Assistant, Synology, Unifi) is that physical
access is root. That is right for a NAS and wrong for us: we hold phone-grade
data in an appliance form factor, and phones decided long ago that possession
is not enough. We follow the phone.

## 4. Three proofs of ownership

All owner-held. Listed in order of everyday use:

1. **An already-trusted device vouches.** The common case — adding an iPad from
   a paired phone. No physical act required; the vouching device is already
   inside, and this is what makes "I got a new phone" work from anywhere.
2. **A code emailed to the account**, for someone with no devices left.
3. **A recovery code**, generated at first pair and shown once — the **offline**
   proof. Verified by the box locally and submitted over BLE or the LAN, so it
   needs neither atlas nor internet. It covers a box that never linked an
   account (DIY has no email on file), a box that has lost its network, and a
   world where atlas no longer exists. Because that is its job, it must be a
   secret the **box** holds — never anything atlas could derive.

**Presence scales inversely with proof strength.** Proofs 2 and 3 are remote
secrets, so they additionally require physical presence at the box; proof 1 does
not, because it already is presence-by-proxy.

### The button triggers; something else authorizes

The board has a power button, so a physical press is the trigger
(`HandlePowerKey=ignore` + a udev rule on `KEY_POWER`). It opens a short window
and re-advertises BLE.

1. Press the button.
2. The box makes **one** outbound call — no standing poll.
3. Atlas emails a code to the **account address**.
4. Type it into the app; the box verifies.

The panel shows only *"Recovery started — check your email."* No secret ever
appears on glass, and it does not matter who pressed the button: the code goes
to the owner's inbox, so a houseguest achieves nothing but sending mail.

**Why a trigger and not a proof.** Presence alone would let anyone who can reach
the box into a machine holding someone's life. The button's job is to remove the
*polling*, not the second factor — a box that continuously asks our servers
whether someone would like to pair is both wasteful and a standing lever we
should not hold.

**Offline, the same press does proof 3 instead**, with atlas never involved: the
window opens, the owner submits the recovery code they already hold, and the box
verifies it locally.

### Why atlas relaying the code is safe, and granting it would not be

Atlas **carries** the email, so it can read the code in transit. That is
tolerable *only* because presence is also required: an attacker inside atlas
still has to be standing in your living room.

What must never be built is atlas **authorizing** a pairing on its own. If our
cloud can grant access to your box, then we functionally hold your keys — a
breach, a subpoena, or one rogue employee becomes your life record, and the
product's central claim quietly stops being true. Atlas's blast radius stays
billing and reach; never data. This is the same invariant that keeps the api key
out of the data path, and it is the line to defend hardest, because every
convenience argument pushes against it.

## 5. Recovery is not a feature

"Add my iPad", "replace my lost phone", and "I am locked out of my own box" are
**the same operation** — *join a claimed box* — differing only in which proof is
available. One mechanism, three names.

This is the collapse that makes the design tractable. There is no recovery mode
to build, no lockout special case, no support tool. There is one join flow that
accepts three proofs.

**The recovery code's ceremony.** It is shown in the **app**, once, straight
after the first pair — never on the panel, for the reason above. Frame the
consequence rather than the chore ("if you lose this Mac and your phone, this is
the way back in"), make *Save to password manager* the primary action rather
than a suggestion, and confirm with the last group instead of a full retype. It
is **regenerable from any trusted device**, which removes the one-shot anxiety
at no cost — regenerating already requires proof 1. Stored **hashed** on the
box: it is verified, never recovered, so a stolen disk must not yield it. Format
for hand transcription, since the true fallback is a piece of paper.

**The honest consequence, which setup must state plainly:** someone who loses
every trusted device, their account, and their recovery code is locked out of
data they physically possess. Everyone who refuses cloud custody lands here —
Apple included. The answer is to make the recovery code's ceremony ordinary
(save it to a password manager, like every other recovery code) and never to
quietly add an escrow that would make the promise untrue.

## 6. Surfaces have fixed jobs

| Surface | Job | Never |
|---|---|---|
| **Box display** | instruct when the owner has nothing else; confirm progress otherwise; show codes during a presence window | be the source of a fact the app could fetch |
| **App** | the wizard *and* the authenticator: holds the account session, couriers grants | send the user to another surface to read something |
| **Browser** | money and identity, one email field | anything else |
| **Email** | prove account control when no payment does | routine steps |

The box drives its own state and the app mirrors it — never the reverse. The
box is the authority on which step it is on (`/api/box/identity` carries
`linked`/`online` for exactly this), because the box is the thing that knows.

## 7. What falls out

The flow is not designed separately; it is a consequence of the above.

**Appliance owner, with the app (the intended majority):**

1. Install the app, sign in. *(Store buyers arrive signed in and paid.)*
2. Open it. One screen: "Set up **Honest Kestrel** on **your-wifi**?" — password,
   one button.
3. Done — the box opens.

One Bluetooth session carries the wifi credentials, the account grant, and the
pairing (§2b). Nothing is read off a screen or typed anywhere.

**Without the app, or without Bluetooth:** the panel's three screens, exactly as
built today — get the app, link (code + QR), pair (code). The floor.

**DIY / headless:** `virtues init` at a terminal, which asks the same identity
question the web does ([1] log in / [2] create new). A different person on a
different surface needing the same fact. They never see the appliance flow, and
the appliance owner never sees theirs.

## 8. What this rules out

Stated so the next person does not have to re-derive them:

- **No atlas-initiated anything.** No callback, no remote unlock, no remote
  wipe. The box speaks outbound only. (See also the box-theft model: recovery is
  owner-driven.)
- **No escrowed recovery key.** It would make the sovereignty claim false.
- **No "physical access = root".** The NAS convention, wrong for this data.
- **No forced subscription on DIY.** Prescribe, never enforce — but the line is
  between *products*, not screens. An **appliance** is a guided thing: its panel
  sequences the three steps and its owner bought hardware that assumes a
  subscription, so requiring the link there is intended. A **DIY** box is
  somebody's own server and must run with no account at all, forever.
  `box_status.rs` enforces exactly that split (`is_appliance()`); until
  2026-08-12 it required an account on both, and `/setup` offered no exit, which
  made this promise false for precisely the people it was written for.
- **No confirmation or success screens.** Each reads as progress and costs a
  step. The box's own screen advancing *is* the confirmation.
