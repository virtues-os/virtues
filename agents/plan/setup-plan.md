# Setup — one flow, from the box on the desk to the app

> Written 2026-09-24. Supersedes the *order and surfaces* in
> [onboarding-plan.md](onboarding-plan.md) and the Getting started room built
> 2026-09-23 (`apps/web/src/lib/components/start/`). The paradigm's reasons —
> Bluetooth first, four words as line of sight, pairing is LAN-only — stand;
> see [the paradigm](../record/onboarding-paradigm.md).
> Delete this file when slice 4 lands; what survives is a record and a manual page.

## The shape

The person already has a server running. They download the iPhone or Mac app,
and it opens into **Setup** — one word, used everywhere, for one linear process.
Not "onboarding", not "getting started", not "the airlock".

```
Welcome → Letter → Server → Wi-Fi → Subscription → Names → Connections → Timeline → Interview → ∴ → app
   1         2        3        4          5            6         7            8          9
```

Revised 2026-09-24: Welcome (Hello, with a light/dark choice under its arrow)
and the letter are dots in the stepper too — everything is. The table below
predates that and still numbers from Server.

| # | Step | Required | What it is |
|---|---|---|---|
| — | **Hello** | — | The cold open: music, the ∴ forming, the tagline. One press continues. |
| — | **Letter** | — | The founder's letter, as a preface: why this exists, from the person who made it. Ends on one button. |
| 1 | **Server** | yes | Find it (Bluetooth + the local network), type the four words on its screen, pair. |
| 2 | **Wi-Fi** | yes | Only shown when the server is not online. On ethernet or already joined, the dot fills itself. |
| 3 | **Subscription** | yes, **no skip** | Set up your subscription · I already have an account · Use my own AI. Any of the three settles it. Carries the one paragraph that used to be the letter's P.S.: the server keeps the record, the subscription pays for the intelligence, and the goal is for all of it to run at home. |
| 4 | **Names** | yes | One exchange: its name, then yours ("And what should Ari call you?"). Then home: the city chart, time zone prefilled. |
| 5 | **Connections** | skippable | This device's permissions, the other device, accounts (Google, bank). Cards light as data arrives. |
| 6 | **Timeline** | skippable | The chapters of your life, drawn by hand; birth date is its left edge. |
| 7 | **Interview** | skippable | One question per screen, each question skippable. Says how long it takes before it starts. |
| — | **∴** | — | The seven dots draw together into the mark; the app opens beneath it. |

### Rules of the flow

- **Progress is visible.** Seven dots at the top with a label: where you are and
  what is next ("Wi-Fi · next, Subscription"). Hello and the letter have no dot.
  An adult deserves to know what is coming.
- **Forward is the only direction the flow pushes; back is always free.** Any
  filled dot can be tapped to revisit its step.
- **Steps 5–7 have two exits.** *Skip* marks the step skipped and moves to the
  next one. *Finish later* opens the app now. Skip must not open the app — one
  skipped step would throw away the two after it.
- **The app opens exactly once**, with the ∴ close and the app-opening
  transition (built 2026-09-23, `stores/appOpening.ts`), whether at the end of
  step 7 or on Finish later.
- **After the app opens**, any step not done (open *or* skipped) keeps a
  **Setup** tile on the rail above Home — "Setup 5/7" — whose panel lists the
  same seven steps with 1–4 already ticked. It is the same process continued,
  not a second one. The tile goes when all seven are done or the person hides it.
- **Resume is derived, never stored.** Every step's status is read off real
  rows. Reopening the app mid-setup lands on the first step not done; Hello and
  the letter replay only when nothing past them is done.
- **A second device** (or a server someone else already set up) takes the short
  path: Server → Connections → app. Subscription, Names and the rest belong to
  the server and are already settled.

## One name

The two-word codename ("Quaint Tern") goes. People are constantly confused by
it, and its only job — telling servers apart before anything is named — is
already done by the four words on the server's own screen, which also prove the
person is standing at their own.

- Before step 4 the server is "your server" in the app, and its screen shows
  only the four words.
- Step 4 names the assistant; that name becomes the server's name everywhere:
  its screen, its Bluetooth advertisement, Settings, the device list.
- **The name belongs to the assistant; the hardware takes the possessive.**
  "Ari's server is updating", never "Ari is updating". That keeps the copy
  rule that the box has no mind, with one name.

## What exists, what is new

| Step | Exists | Work |
|---|---|---|
| Hello | `onboarding/Hello.svelte` | Move to the front of the flow. Music (`static/onboarding/hello.mp3`) and an owned picture still missing. |
| Letter | `onboarding/document/FoundersLetter.svelte` | Its close loses the subscription; ends on one button. `/founders-letter?read` stays as the re-read. |
| Server | `src-tauri/ui/connect.html` (vanilla, compiled into the binary) | Slice 2: rebuild in Svelte over the same `plugin:reach|*` commands. |
| Wi-Fi | `connect.html` | Slice 2, same. |
| Subscription | Twice: `connect.html` inline sign-in (pre-pair, atlas direct) and `LetterSubscription.svelte` (post-pair, via the box) | Keep the post-pair one as the step; delete the airlock's in slice 4. |
| Names | `start/steps/StepName.svelte`, `StepLocation.svelte` | Merge into one step with three beats. |
| Connections | `start/steps/StepDevices.svelte` | Accounts inline; an OAuth or Plaid round trip must land back on this step. |
| Timeline | `start/steps/StepTimeline.svelte` | Restyle into the flow's register. |
| Interview | The getting-started chat room | New: one question per screen over the same interviewer. |
| ∴ close | `appOpening` + `(app)/+layout.svelte` opening | Dots → ∴ morph. |

### Server state

`GET /api/getting-started` derives four steps today: `connect_ai`,
`introductions`, `connect_world`, `interview` — and `interview` is done by a
document **or** by drawn chapters. The flow needs Timeline and Interview apart:

- add `timeline`, done when `wiki_chapters` has rows;
- `interview` goes back to done only when the document exists
  (`narrative_identity_ready`).

Additive: a step id appears, none disappears, so an older client ignores it.

The app gate (`routes/(app)/+layout.ts`) redirects to `/founders-letter` while
`onboarding_status` is not `active`. It redirects to `/setup` instead, and the
flow sets `active` when the app opens (end of step 7, or Finish later).

## Slices

Each ships on its own.

1. **The flow in the web app, from the letter to the opening.** Hello, Letter,
   Subscription, Names, Connections, Timeline, Interview, ∴, at `/setup`, full
   screen, with the dots. The rail tile becomes Setup and counts skipped steps as
   unfinished. Server + Wi-Fi show as already-filled dots (a paired device has
   done them). All web code plus the additive server change; ships with the
   server, testable on dev with no app build.
2. **Server and Wi-Fi in the same flow.** Rebuild the airlock's find / four words
   / pair / Wi-Fi screens in Svelte, running before any server session with the
   app's reach plugin as transport. iPhone first: it already bakes the SPA, and
   the same origin makes the flow continuous. Then the Mac, which must start
   baking the SPA (decision below). Needs app releases and real hardware,
   including the first confirmed iPhone Bluetooth pair.
3. **One name.** Replace the codename: server screen, Bluetooth name, mDNS,
   Settings, device list. Older apps expect a codename, so the server keeps
   answering the old field until they age out.
4. **Cleanup.** Retire `connect.html` down to a small compiled-in recovery page
   ("can't reach your server", "forget this server"), the airlock's sign-in,
   the chat onboarding on the server (narrate, seed, tools), and the Start room
   code this replaces.

## Slice 1 — where it stands (2026-09-24)

Built, uncommitted on `wave`: `/setup/[[step]]` (the flow), `lib/components/setup/`
(store, dots with the ∴ close, rail panel, steps), the Subscription step (the
letter's close moved out, with its P.S.), Names (its name → "And what should
Ari call you?" → home), the one-question interview over the existing
interviewer (drawn chapters are sent as the first answer), the rail tile as
**Setup N/7** (skipped keeps it), `/founders-letter` as a read-only re-read,
the app gate and `/onboarding` pointing at `/setup`, and the server's
`timeline` step. The in-app Start pane and its tab type are gone.

Not yet exercised: the interview end to end (needs the new server binary and
a box that can take writes), Hello → letter on a fresh box, the mobile shell
(no rail tile there yet), and accounts inline in Connections.

## The four laws (2026-09-25)

Written after a page-by-page review against Arc's and Dia's onboarding. Each
page was well made alone; together they read as four web pages with a
progress bar over them. These hold every step, and a new step obeys them
before it ships.

1. **One stage.** Paper and grain are painted once by the route
   (`.setup-stage`, `lib/components/setup/setup.css`); steps crossfade on it
   and never fade through a blank page.
2. **One mark.** The ∴ is the progress (`SetupMark`): three dots for the three
   thirds of Setup, filling as each third is done, the current one breathing.
   Welcome's big ∴ flies up to become it; the close brings it down to the
   middle, whole, and the app opens beneath it. No dots row, no "next" label.
3. **One motion.** Three durations, one ease, one spring for presses, one text
   entrance, one exit (`setup.css` tokens, `motion.ts` for Svelte). Every
   button gives under the finger.
4. **One voice and one alignment.** Titles, sentences and the way forward on
   the center line (`StepFrame` centers by default); copy through
   `agents/build/voice.md`.

### Built 2026-09-25 (uncommitted work lands with this plan)

- The stage, the mark, the motion tokens, centered steps.
- Welcome as one composition (lockup, door and drawing in one group); the
  drawing draws itself in with the signature's pen and leans from the
  cursor; the mark breathes once settled; no blank first frame.
- The letter as a sheet of paper on the stage, set aside when left, with the
  way on pinned at the foot until the letter's own button is in view.
- Subscription as one card: what it costs and what it buys (Billing's own
  claim: hosted AI, web search, place search, bank connections).
- The assistant's presence on Names: the ∴ breathes and ripples as you type.
- Connections: live counts under each device ("Arrived this week: …").
- The close opens on "You" if the interview was told, else the chapters.
- Settings → Devices opens the same add-a-device sheet as Setup (its own
  `/pair#t=` QR could not be scanned by the phone).
- Mac: Setup switches the title bar to an overlay (frameless) and back.
  Needs an app build to verify; capability added.

### Not built, and why

- **Setup before pairing (slice 2)** and **phone and server in lockstep**:
  app builds and real hardware; the lockstep also needs the Bluetooth session
  to report typed words, a wire change.
- **Answering the interview out loud**: needs a transcription route for the
  web app. Until then, touch screens point at the keyboard's own dictation.
- **Your city as the drawing**: needs an image route and storage, and a
  generated drawing that gets the city wrong costs more than it delights.
  Decide before building.
- **Paying on iPhone**: since May 2025 US storefront apps may link out to web
  checkout (what the flow does). Elsewhere it needs in-app purchase or a
  regional arrangement. Decide before the iOS app is sold outside the US.
- **The music** stays out of git: its license certificate grants nothing
  about publishing the master, and the repo is public. Ship it from a
  private path.

## One flow on every device (2026-09-25)

Setup runs **before** a server session exists, so it cannot be the server's
SPA. It is the app's own copy of `apps/web`, baked into the binary, on the
iPhone (which already bakes and updates over the air) **and the Mac, which
starts doing the same**. Carrying only Welcome to Wi-Fi and then jumping to
the server's copy was rejected: a hard reload between Wi-Fi and Subscription
breaks the dots, the music and every transition.

The steps are written once, in Svelte, and never ask which device they are
on. What differs is a small set of adapters behind one interface
(`lib/components/setup/device.ts`, not yet written):

| Capability | iPhone | Mac / PC | Browser (SSH console) |
|---|---|---|---|
| find servers | Bluetooth (Swift) + local network | Bluetooth + local network | none, already in |
| talk before pairing | `plugin:reach` over Bluetooth | same | n/a |
| pair, hold the key | `plugin:reach` | same | n/a |
| join an existing server | scan a QR | enter a code | n/a |
| show a code for another device | yes (new) | yes | no |
| this device's data | HealthKit, location, audio | the Mac collector | none |

A step that needs a capability the device lacks drops the beat, never the
flow: the browser starts at Subscription with Server and Wi-Fi filled.

### The paths

- **New server, first device.** Welcome → Letter → **Server** (look over
  Bluetooth and the network; "Which server?" if several; type the four words;
  save them, since they are the recovery phrase; update the server if it is
  behind the app) → **Wi-Fi** (the server's scan, password, joining; pairing
  happens at its end over the same Bluetooth link; fills itself on ethernet)
  → Subscription → Names → Connections → Timeline → Interview → ∴.
- **Server installed from the command line.** Server asks for the 6-digit
  code from `virtues pair` instead of the four words; Wi-Fi fills itself.
- **Second device.** Welcome → Server (join) → This device → ∴. No letter:
  it, the subscription and the names belong to the server's owner.
  - A phone joins by scanning the computer's QR.
  - A computer joins by entering a code another device shows. **Gap:** the
    phone cannot show one today, so a phone-only owner can add a computer
    only through `virtues pair`.
  - **Bug:** the airlock sends people to Settings → Devices → Add device,
    whose QR is a `/pair#t=` link the phone's scanner cannot read; only
    `DevicePairModal` makes a handoff QR. One "Add a device" sheet everywhere.
- **Not setup.** "Can't reach your server" and "doesn't recognize this
  device" become small screens in the baked app; one compiled-in HTML page
  stays only for when the baked app itself fails to load.
- The Mac collector installs in Connections ("This Mac"), not at pairing, so
  a failed install shows as undone there rather than hiding a paired Mac.

## Open decisions

- **Hiding the Setup tile** with steps still skipped: where the "hide" lives,
  and whether it is per device or per server.
- **Tagline** for Hello. Leading candidate: "The most intimate record of your
  life, finally yours." (settled for the establishing screen 2026-09-15), with
  "Intelligence, made personal." kept small beneath the ∴.
