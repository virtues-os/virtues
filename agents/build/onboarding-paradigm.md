# Onboarding, identity, and trust — the design model

> The settled model for how a box gets online, whose it is, and which devices
> may read it. Written 2026-08-12 after two days of building the pieces on
> hardware and finding the design by running into its limits. This document
> explains why the flow works the way it does; [onboarding.md](onboarding.md) is what
> is built, [linking-plan.md](../archive/linking-plan.md) is the account step in detail,
> and [onboarding-plan.md](../archive/onboarding-plan.md) is the build order.
>
> Intended to be stable. If a change contradicts something here, the change is
> probably wrong, or this document needs an explicit revision rather than an
> unrecorded exception.
>
> **Audited against the code 2026-08-28.** §1, §3, §7 and §9 matched the code
> exactly, including the grant flow, which is the most important rule in this
> document. Four corrections are marked inline where they sit: the panel's string
> and its state count (§2), the browser hop that setup really does make (§4),
> and a throttle described in more detail than was built (§5). The model itself
> did not change.

## 1. A single secret: the four-word phrase

A box has exactly one secret that proves ownership: a **four-word phrase** —
`mango-burly-skull-dough`. It is both the Bluetooth setup key and the recovery
key; one secret serves both purposes.

Everything else here follows from *where that phrase can be read, and when*.

**While the box is empty, the phrase is on its screen.** There is no data to
steal, so displaying it costs nothing, and reading it requires *seeing* the box,
which is the requirement we want. Radio signals pass through walls; line of
sight does not.

**And while it is on the screen it rotates**, on the same cadence as the standing
pair code (15 minutes, 5 minutes of overlap, so a rotation mid-setup cannot
leave anyone stuck). Without rotation, a box plugged in and left unclaimed for a
week displays a permanent key: any houseguest could photograph the words and
reset and claim the box months later. Rotation makes last week's photograph
useless.

**The moment the box is claimed, the phrase freezes and is never shown on the
screen again.** The box now contains the owner's data. From then on the phrase
exists only where the owner saved it, and what they saved is exactly what they
typed, so there is no second secret to explain.

These two rules are what make the rest of the design safe. Keep them if any of
this is revisited.

## 2. The setup flow on a single screen

**Panel — one screen, no steps, no counter:**

```
    ∴ Quaint Tern

    Get Virtues for your computer
    virtues.com/downloads — then type these words.

    mango-burly-skull-dough
```

The name sits in the corner rather than in the heading, because the heading
should tell the *person* what to do. The name is there so you can check it
against the app before typing a secret into it.

**The panel says "your computer", not "the app".** Setup is a desktop task,
because 802.1X credentials and a four-word phrase both need a keyboard. A phone joins later as a
second device. There is no QR: it pointed a phone at the download page, so
scanning it would hand setup to the wrong machine.

> **Revised 2026-08-24 (Pemberley pass).** This said *Mac*, and the panel
> rendered "Get Virtues for Mac". Windows became a first-device platform —
> `btleplug` speaks WinRT and `tauri.windows.conf.json` ships the bundle — so
> the string is platform-neutral on the glass, in `DisplayView`, and in the
> airlock. The rule is unchanged: setup uses a computer with a keyboard, not a phone.

The phrase must fit on **one line**. It is read across a room while being typed
on another machine, and a phrase that wraps is harder to read. That is why the
wordlist is capped at seven-letter words (`MAX_WORD_LEN` in
`api/setup_phrase.rs`) — 50 of
400 words, 2^34.6 → 2^33.8, a negligible difference against a rate-limited online guess.

Two more panel states follow from the same rules — these three are the *setup*
screens, and they are what this section governs:

- **Session live** — as soon as the phrase is accepted it disappears from the
  screen, and `Setting up · with Nick's Mac` replaces it. The words have been
  used, so nobody who walks past can read them, and the owner can see on the box
  itself that what they typed reached *this* box. The screen reverts to the
  phrase after 90 seconds of inactivity, so a setup that fails halfway does not
  leave someone in front of a box that will not show how to start over.
- **Reset, not factory-new** — a reset box still contains the owner's data, so
  its phrase stays frozen and off the screen. The panel asks for *the words you
  saved*, and says **"your record is still here"**, so the owner does not assume
  the reset erased their data. Without this state the factory-new layout renders
  with a blank where the words go, which looks like a fault at the worst
  possible moment.

Three more were added later and **take priority over all of these**: the
button-hold countdown, "updating", and "no data disk". They are interruptions
rather than steps: each reports something happening to the box right now that an
ordinary screen would hide. This matters most for the last one: the OS lives on
the eMMC precisely so the box can report a missing data disk, and an ambient
"REACHABLE · 3 devices syncing" screen over a missing disk would hide that. Seven states in five ranks, in
`routes/(public)/display/+page.svelte`.

**Then, in the app:**

1. It finds the box over Bluetooth and asks for those four words.
2. **"Save this — it's how you get back in."**  Copy · Save to password manager.
3. Setup runs in that one session: joined → linked → paired. The box opens.

Saving comes **before** the rest of setup. Someone who has just typed a phrase
will save it; the same person ten seconds after "your box is ready" will dismiss
whatever is on screen. Saving first also means the recovery path exists before
anything can go wrong.

**Ethernet does not skip a stage.** A wired box is online at first boot but is
still linked and paired through the app; the Wi-Fi picker simply offers "already
on ethernet". A wired connection removes the need for a Wi-Fi password, but the
step is still there.

## 3. Reset is the only recovery method, and the phrase keeps it safe

The board has a power button, behind the case. **Held for three seconds** it
forgets this box's **paired devices**. Nothing else: the data stays, the phrase
stays, the network stays, the account stays.

> **Revised 2026-08-17; the revision reduced what reset does.** This used to also
> unlink the account and forget the network. Both were wrong. Neither adds any
> security — the phrase is the entire gate, and a stranger with a screwdriver
> still cannot claim the box without four words that are frozen and shown
> nowhere — while forgetting the network actively harms the case the button
> exists for. The owner who has lost their laptop presses it and now has a box
> that is *also* offline and unlinked, able to reach neither the relay nor
> atlas: recovery became harder with no security benefit.
>
> So the button and the app's "start over" are now literally the same action
> (`api::pair::revoke_all_devices`). A physical control that behaves slightly
> differently from the equivalent software action would be a bug in itself.

**It takes a three-second hold, not a press.** It is the power button, and on a
board with no other control, a short press can also come from a cable, a shelf
or a cat.
Unpairing every device in a household by accident is not recoverable in any
useful sense — the owner has to redo setup on all of them. The panel counts the
hold down while it happens, so someone holding it on purpose knows it is working and
an accidental hold is visible while there is still time to stop it.

`logind` owns the power key by default, so the installer sets
`HandlePowerKey=ignore`. Without it the first press powers the box off, which
is the worst possible result for someone who opened the case because they could
not reach their box.

The box returns to unclaimed, but *not* to factory-new. It still contains the
owner's data, so its screen does **not** show the phrase again. Setting it up again requires the words
the owner saved.

The whole security argument is these two points together:

- Anyone who can open the case can **reset** your box. That is an inconvenience:
  you set it up again and your data is where you left it.
- Only someone with the **phrase** can *claim* it. That is the part that matters,
  and physical access to the case does not provide it.

**Erasing** — wiping the data for a resale or hand-me-down — is intentionally
*not* on the button. A physical action that destroys someone's personal data
could be used for vandalism; one that clears pairings is only an inconvenience. Erase lives behind
an authenticated action in the app, or the CLI for DIY.

**A consequence that setup must state plainly:** if the owner loses the phrase
*and* every paired device, there is no way in, and erasing and starting over is
the only option. Every product that refuses to hold keys in the cloud has this
limitation, Apple included. The answer is to make saving the phrase routine (a
password manager, like any other recovery code), never to add an escrow that
would make the no-custody promise untrue.

**What this does not defend against: the disk.** `VIRTUES_ENCRYPTION_KEY` lives
in an env file on the box's own disk. It protects stored credentials, not the
owner's data, and it is stored next to what it protects. Someone who carries the hardware
away can read it, phrase or no phrase. Making that untrue needs encryption at
rest with a key that is *not* on the box (a passphrase at boot, or a TPM), which
is a real decision with a real cost: a box that cannot reboot unattended. It is
worth doing eventually, but we must not claim it is done until it is.

## 4. Why there are no other codes

Every code on a screen that a person retypes elsewhere is a **substitute for a
missing data channel**. Bluetooth now provides that channel, so the phrase is
the only code left, and its purpose is to prove line of sight, not to transfer
information.

> **The app carries all data between the box and other services.**

Setup is one Bluetooth connection: the app sends the Wi-Fi credentials and the
account grant to the box, and receives the pairing back. Nothing else is ever read
off the panel and retyped.

**The one browser step is payment, and it is intentional.** Sign-in happens inside
the airlock — email plus a code, `atlasPost('/account/login')` — but checkout
opens the real browser (`openUrl` on the atlas checkout URL), because cards and
Apple Pay live there and an embedded webview asking for a card looks exactly
like a phishing screen. The airlock explains this and polls until the payment
completes. This exception is consistent with the rule above: the app never opens
a browser to move *information it could have carried itself*; it opens one only
for a payment method it must not handle.

A consequence that settles a recurring argument: **the app must never tell
someone to go read something it could have fetched itself.** If a screen says
"type the code from your box", ask whether the app could have obtained it — and
the answer is yes for everything except the phrase, whose entire purpose is that
the app *cannot*.

**The app is therefore required for setup**, which is an intentional trade-off:

- Android has no Improv client, so an Android-only household cannot set up an
  appliance until one exists.
- A machine with Bluetooth disabled by policy has no path either.
- DIY boxes are unaffected: they have a terminal, which is a better interface for
  this than any of the above.

The alternative was maintaining a parallel code path on the panel permanently: a
second flow to build, test and explain, used by almost nobody, and decaying
because nobody uses it. One working path is better than two where one is not
really used.

## 5. The setup session

The phrase authorizes the setup; the **session** performs it.

> **A setup session is claimed explicitly, with the phrase, and only one may be
> live.** Only that connection may link and pair.

- **A session is tied to a connection, not a device.** It does not mean
  "whoever knows the phrase, forever"; it means the connection currently
  configuring this box. Drop the link and it dies; the next
  attempt starts over.
- **Guess attempts are limited per BOX, not per caller.** A BLE central can
  change its address between attempts, so per-device throttling would be
  ineffective. Only
  one legitimate setup ever happens at a time, so a global budget costs a real
  owner nothing and stops a patient attacker in range.

  As built (`api::setup_phrase`): a flat box-wide sliding window — **10 attempts
  per 15 minutes**, stored in memory, cleared on success so a fumbled entry never
  counts against the next legitimate setup. A poisoned lock fails *open*,
  because locking the owner out is the worse failure. It is intentionally kept
  in memory: a restart clears it, and forcing a restart already requires the
  physical access this limit does not try to defend against.

  > **Corrected 2026-08-28.** This section described exponential backoff and "a
  > tighter per-connection cap on top". Neither exists — the budget is flat and
  > there is only one. Both would be reasonable to add; neither should be
  > described as present until it is. (`ble_provision` does rate-limit `0x83`
  > pair attempts separately, which is a different surface and a different
  > budget.)
- **It survives the network change.** Bluetooth is a separate radio from Wi-Fi,
  which is why provisioning uses it. The connection continues through the
  Wi-Fi switchover that used to end it.
- **The panel shows the session live** — "Setting up with Nick's Mac…" — so
  anything unexpected is visible while it happens, on the owner's own hardware.

## 6. The three relationships setup establishes

Onboarding establishes three facts that have nothing to do with each other:

| | Fact | Between |
|---|---|---|
| **Network** | the box can reach the internet | box ↔ network |
| **Account** | someone pays for what connects it | box ↔ atlas |
| **Devices** | these devices may read the owner's data | box ↔ device |

They remain three relationships. They are no longer three *screens*, because one
session establishes all three.

**The order is determined by dependencies.** The account step needs the
network; remote reach depends on the relay, and the relay depends on the
account. Pairing before linking produces a box that is paired and unreachable.
This was observed live at an office on 2026-08-11, which is how we found the
required order.

**Skipping the account link is supported.** The airlock offers
"Skip — run it without a subscription", and a skipped sign-in writes no grant and
goes straight to pairing: LAN-only, by the user's choice. The ordering above is
what makes skipping safe: if you link first, remote reach exists by the time
anything needs it; if you skip, you get a LAN-only box, which is what you asked
for.

## 7. Atlas can pass a grant to the box but can never authorize access

The account provides **billing and remote reach, never data custody**. That
sentence is a design rule, UI copy, and a security invariant.

The app holds an account session and asks atlas for a **grant**; it hands that to
the box over Bluetooth; **the box redeems it outbound** with its own credential.
Atlas never gains a path into a box, and the box's key belongs to the box.

What must never be built is atlas **authorizing** access to a box. If our cloud
can grant that, we effectively hold your keys: a breach, a subpoena, or one
rogue employee could expose someone's personal data, and the product's central
claim would no longer be true. A compromise of atlas must stay limited to
billing and reach. This is the most important rule to keep, because every
convenience argument pushes against it.

After pairing, the app rarely talks to atlas directly: for usage, wallet or billing it asks
the **box**, and the box asks atlas with its own key. A user session is not a
second way into the same data.

## 8. What each surface does

| Surface | Job | Never |
|---|---|---|
| **Box display** | one setup screen (app + phrase) while factory-new; narrate the live session; then ambient | show the phrase once claimed, or count steps |
| **App** | the wizard *and* the authenticator: holds the account session, carries grants to the box, offers copy / save-to-password-manager / print for the phrase | persist the phrase itself, or email it |
| **Browser** | deliver the app; account and card management — including the one checkout hop during setup (§4) | hold a session that could reach the owner's data, or be where the phrase is typed |

The box drives its own state and the app mirrors it — never the reverse.
`/api/box/identity` carries `linked`/`online` for exactly this, because the box
is the source of truth for its own state.

## 9. What this design excludes

Listed here so the next person does not have to work them out again:

- **No atlas-initiated anything.** No callback, no remote unlock, no remote wipe.
  The box speaks outbound only.
- **No escrowed phrase.** It would make the claim that only the owner controls the box false.
- **The app never persists the phrase**, and never emails it. Storing it would
  mean a stolen laptop holds not just a revocable pairing but the thing that
  survives every reset; emailing it would route the permanent key through the
  most-attacked channel a person owns. Copy, save to a password manager, print.
- **No second setup path.** There is one setup flow.
- **No phrase on the panel after claim**, and none on a sticker inside the case —
  the button is in there too, and a secret that anyone who can press the button
  can read is not a secret.
- **No erase on the button.** Clearing pairings is an inconvenience; erasing the
  owner's data cannot be undone.
- **No forced subscription on DIY.** An appliance is a guided product and may
  require the link; a DIY box is somebody's own server and must run with no
  account at all, forever. `box_status.rs` enforces the split via
  `is_appliance()`.
- **No confirmation or success screens.** Each one looks like progress but adds
  a step. The change on the box's own screen is the confirmation.

---

*Superseded by this document (2026-08-12): the three-proof model (trusted device
vouches / emailed code / separate recovery code), the power-cycle trigger, the
"presence triggers, email authorizes" ordering, and the panel's 1/2/3 step
sequence. Each was a reasonable answer to "how do we prove ownership without
cloud custody"; the phrase solves it with one mechanism instead of three.*
