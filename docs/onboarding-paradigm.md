# Onboarding, identity, and trust — the paradigm

> The settled model for how a box gets online, whose it is, and which devices
> may read it. Written 2026-08-12 after two days of building the pieces on
> hardware and discovering the shape by hitting its walls. This document is the
> reason the flow looks the way it does; [onboarding.md](onboarding.md) is what
> is built, [linking-plan.md](linking-plan.md) is the account step in detail.
>
> Intended to be stable. If a change contradicts something here, the change is
> probably wrong — or this document needs a deliberate revision, not a quiet
> exception.

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
   inside.
2. **The account.** A magic link to the account email, for someone with no
   devices left.
3. **A recovery code**, generated at first pair and shown once. The one that
   still works if atlas ceases to exist — which is the promise the whole
   product rests on.

**Presence scales inversely with proof strength.** Proofs 2 and 3 are remote
secrets, so they additionally require physical presence at the box; proof 1
does not, because it already is presence-by-proxy.

**Atlas can never authorize alone.** An atlas compromise yields billing and
relay disruption, never record data — the same invariant that keeps the api key
out of the data path.

## 5. Recovery is not a feature

"Add my iPad", "replace my lost phone", and "I am locked out of my own box" are
**the same operation** — *join a claimed box* — differing only in which proof is
available. One mechanism, three names.

This is the collapse that makes the design tractable. There is no recovery mode
to build, no lockout special case, no support tool. There is one join flow that
accepts three proofs.

**Physical presence, with no input hardware.** The panel is output-only and
there is no button. Power-cycling three times within thirty seconds opens a
two-minute window: the panel shows a join code and BLE re-advertises. It is an
established appliance convention, needs no BOM change, and is essentially
impossible to do by accident.

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
2. Open it: "Set up **Honest Kestrel** on **your-wifi**?" — password, one button.
3. Done — the box opens.

The account grant and the pairing both ride BLE; no code is read or typed.

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
- **No forced subscription.** Prescribe, never enforce: the display sequences,
  but the app can always pair. An owner who refuses the subscription owns
  slower, LAN-only hardware — not a brick.
- **No confirmation or success screens.** Each reads as progress and costs a
  step. The box's own screen advancing *is* the confirmation.
