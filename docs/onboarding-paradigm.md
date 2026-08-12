# Onboarding, identity, and trust — the paradigm

> The settled model for how a box gets online, whose it is, and which devices
> may read it. Written 2026-08-12 after two days of building the pieces on
> hardware and discovering the shape by hitting its walls. This document is the
> reason the flow looks the way it does; [onboarding.md](onboarding.md) is what
> is built, [linking-plan.md](linking-plan.md) is the account step in detail,
> and [onboarding-plan.md](onboarding-plan.md) is the build order.
>
> Intended to be stable. If a change contradicts something here, the change is
> probably wrong — or this document needs a deliberate revision, not a quiet
> exception.

## 1. One secret, one phrase

A box has exactly one secret that proves ownership: a **four-word phrase** —
`mango-burly-skull-dough`. It is the Bluetooth setup key and the recovery key,
because those were never two things.

Everything else here follows from *where that phrase is readable, and when*.

**While the box is empty, the phrase is on its screen.** There is nothing to
steal, so displaying it costs nothing — and reading it requires *seeing* the box,
which is exactly the bar we want. Radio range passes through walls; line of sight
does not.

**And while it is on the screen it rotates**, on the same cadence as the standing
pair code (15 minutes, 5 minutes of overlap, so a rotation mid-setup cannot
strand anyone). Without this, a box plugged in and left unclaimed for a week is a
permanent key on display: any houseguest photographs the words and can reset and
claim it months later. Rotation makes last week's photograph worthless.

**The moment the box is claimed, the phrase freezes and leaves the screen
forever.** It now holds a life. From then on it exists only where the owner saved
it — and what they saved is exactly what they typed, so there is no second secret
to explain.

That pair of rules is what makes the rest safe, and they are the sentences to
defend if any of this is revisited.

## 2. The flow, which is now one screen

**Panel — one screen, no steps, no counter:**

```
    ∴ Honest Kestrel

    Get Virtues for Mac
    virtues.com/downloads — then type these words.

    mango-burly-skull-dough
```

The name is a lockup in the corner, not the heading: a box announcing itself is
odd, and the heading's job is what the *person* does. Its real job is the
identity you check against the app before typing a secret into it.

**Mac, not "the app".** Setup is a desktop job — it wants the keyboard that
802.1X credentials and a four-word phrase want. A phone joins later as a second
device. There is no QR: it pointed a phone at the download page, so scanning it
would hand setup to the wrong machine.

The phrase must fit **one line**. It is read across a room while being typed on
another machine, and a phrase that wraps loses its shape. That is why the
wordlist is capped at seven-letter words (`setup_phrase::MAX_WORD_LEN`) — 50 of
400 words, 2^34.6 → 2^33.8, unmeasurable against a throttled online guess.

Two more panel states follow from the same rules:

- **Session live** — the moment the phrase is accepted it leaves the glass, and
  `Setting up · with Adam's Mac` takes its place. The words are spent, so nobody
  who wanders past can read them; and the owner sees on the box itself that what
  they typed landed *here*. It reverts to the phrase after 90 seconds of quiet,
  so a setup that dies halfway does not strand someone in front of a box that
  will not say how to start over.
- **Reset, not virgin** — a reset box still holds a life, so its phrase stays
  frozen and off the screen. The panel asks for *the words you saved*, and says
  **"your record is still here"** — the sentence that stops someone assuming the
  reset wiped them. Without this state the virgin layout renders with a blank
  where the words go, which reads as a fault at the worst possible moment.

**Then, in the app:**

1. It finds the box over Bluetooth and asks for those four words.
2. **"Save this — it's how you get back in."**  Copy · Save to password manager.
3. Setup runs in that one session: joined → linked → paired. The box opens.

Saving comes **before** the work, not after. Someone holding a phrase they just
typed will save it; the same person ten seconds after "your box is ready"
dismisses whatever is in front of them. It also means the way back in exists
before anything can go wrong.

**Ethernet does not skip a stage.** A wired box is online at first boot but is
still linked and paired through the app — the Wi-Fi picker simply offers "already
on ethernet". Wiring removes a password, not a step.

## 3. Reset is the only recovery, and the phrase is what makes it safe

The board has a power button, behind the case. Pressing it **resets setup**:
paired devices forgotten, account unlinked, network forgotten. **The data stays.
The phrase stays.**

The box returns to unclaimed — but *not* to virgin. It holds a life, so its
screen does **not** show the phrase again. Setting it up again requires the words
the owner saved.

That is the whole security argument, and it belongs as a pair:

- Anyone who can open the case can **reset** your box. A nuisance: you set it up
  again and your data is where you left it.
- Only someone with the **phrase** can *claim* it. That is the part that matters,
  and a screwdriver does not provide it.

**Erasing** — wiping the record for a resale or hand-me-down — is deliberately
*not* on the button. A physical gesture that destroys a life record is a
vandalism vector; one that clears pairings is merely annoying. Erase lives behind
an authenticated action in the app, or the CLI for DIY.

**The honest consequence, which setup must state plainly:** lose the phrase *and*
every paired device and there is no way in — erasing and starting over is all
that is left. Everyone who refuses cloud custody lands here, Apple included. The
answer is to make saving it ordinary (a password manager, like every other
recovery code), never to add an escrow that would make the promise untrue.

**What this does not defend against: the disk.** `VIRTUES_ENCRYPTION_KEY` lives
in an env file on the box's own disk — it protects stored credentials, not the
record, and it sits next to what it protects. Someone who carries the hardware
away can read it, phrase or no phrase. Making that untrue needs encryption at
rest with a key that is *not* on the box (a passphrase at boot, or a TPM), which
is a real decision with a real cost: a box that cannot reboot unattended. Worth
doing eventually; not worth pretending we have done.

## 4. Why there are no other codes

Every code on a screen that a human retypes elsewhere is a **substitute for a
missing channel**. Bluetooth is that channel now, so the phrase is the only one
left — and it exists not to move information but to prove line of sight.

> **The app is the courier.**

Setup is one Bluetooth conversation: the app carries the Wi-Fi credentials,
carries the account grant, takes the pairing back, and opens no browser. Nothing
else is ever read off the panel and retyped.

A corollary that settles a recurring argument: **the app must never instruct
someone to go read something it could have fetched itself.** If a screen says
"type the code from your box", ask whether the app could have obtained it — and
the answer is yes for everything except the phrase, whose entire purpose is that
the app *cannot*.

**The app is therefore mandatory for setup**, a deliberate trade:

- Android has no Improv client, so an Android-only household cannot set up an
  appliance until one exists.
- A machine with Bluetooth disabled by policy has no path either.
- DIY boxes are unaffected — they have a terminal, a better interface than any of
  this.

The alternative was a parallel code path on the panel forever: a second flow to
build, test and explain, taken by almost nobody, rotting because nobody uses it.
One good path beats two, one of which is a fiction.

## 5. The setup session

The phrase authorizes; the **session** carries the work.

> **A setup session is claimed explicitly, with the phrase, and only one may be
> live.** Only that connection may link and pair.

- **It is a connection, not a device.** Not "whoever knows the phrase, forever" —
  the one currently configuring this box. Drop the link and it dies; the next
  attempt starts over.
- **Guessing is budgeted by the BOX, not by the caller.** A BLE central can
  change its address between attempts, so per-device throttling is theatre. Only
  one legitimate setup ever happens at a time, so a global budget with
  exponential backoff costs a real owner nothing and stops a patient attacker in
  range. A tighter per-connection cap sits on top, so a single session cannot
  burn the global budget in one breath.
- **It survives the network change.** Bluetooth is a separate radio from Wi-Fi,
  which is precisely why provisioning rides it. The conversation continues
  through the switchover that used to end it.
- **The panel narrates it live** — "Setting up with Adam's Mac…" — so anything
  unexpected is visible while it happens, on the owner's own hardware.

## 6. Three relationships, still

Onboarding establishes three facts that have nothing to do with each other:

| | Fact | Between |
|---|---|---|
| **Network** | the box can reach the internet | box ↔ network |
| **Account** | someone pays for what connects it | box ↔ atlas |
| **Devices** | these devices may read the record | box ↔ device |

They remain three relationships. They are no longer three *screens*, because one
session establishes all three.

**Order is forced, not chosen.** The account step needs the network; reach rides
the relay and the relay rides the account. Pairing before linking produces a box
that is paired and unreachable — observed live at an office on 2026-08-11, which
is how the ordering was discovered rather than decided.

## 7. Atlas may carry, never grant

The account buys **billing and reach, never data custody**. That sentence is
doctrine, UI copy, and security invariant at once.

The app holds an account session and asks atlas for a **grant**; it hands that to
the box over Bluetooth; **the box redeems it outbound** with its own credential.
Atlas never gains a path into a box, and the box's key belongs to the box.

What must never be built is atlas **authorizing** access to a box. If our cloud
can grant that, we functionally hold your keys — a breach, a subpoena, or one
rogue employee becomes someone's life record, and the product's central claim
quietly stops being true. Atlas's blast radius stays billing and reach. This is
the line to defend hardest, because every convenience argument pushes on it.

After pairing the app barely touches atlas: for usage, wallet or billing it asks
the **box**, and the box asks atlas with its own key. A user session is not a
second door to the same data.

## 8. Surfaces have fixed jobs

| Surface | Job | Never |
|---|---|---|
| **Box display** | one setup screen (app + phrase) while virgin; narrate the live session; then ambient | show the phrase once claimed, or count steps |
| **App** | the wizard *and* the authenticator: holds the account session, couriers grants, offers copy / save-to-password-manager / print for the phrase | persist the phrase itself, or email it |
| **Browser** | deliver the app; account and card management | appear during setup |

The box drives its own state and the app mirrors it — never the reverse.
`/api/box/identity` carries `linked`/`online` for exactly this, because the box
is the thing that knows.

## 9. What this rules out

Stated so the next person does not have to re-derive them:

- **No atlas-initiated anything.** No callback, no remote unlock, no remote wipe.
  The box speaks outbound only.
- **No escrowed phrase.** It would make the sovereignty claim false.
- **The app never persists the phrase**, and never emails it. Storing it would
  mean a stolen laptop holds not just a revocable pairing but the thing that
  survives every reset; emailing it would route the permanent key through the
  most-attacked channel a person owns. Copy, save to a password manager, print.
- **No second setup path.** One good flow, not two.
- **No phrase on the panel after claim**, and none on a sticker inside the case —
  the button is in there too, and a secret readable by whoever can press it is
  not a secret.
- **No erase on the button.** Clearing pairings is annoying; destroying a record
  is unrecoverable.
- **No forced subscription on DIY.** An appliance is a guided product and may
  require the link; a DIY box is somebody's own server and must run with no
  account at all, forever. `box_status.rs` enforces the split via
  `is_appliance()`.
- **No confirmation or success screens.** Each reads as progress and costs a
  step. The box's own screen changing *is* the confirmation.

---

*Superseded by this document (2026-08-12): the three-proof model (trusted device
vouches / emailed code / separate recovery code), the power-cycle trigger, the
"presence triggers, email authorizes" ordering, and the panel's 1/2/3 step
sequence. Each was a reasonable answer to "how do we prove ownership without
cloud custody"; the phrase answers it once instead of three times.*
