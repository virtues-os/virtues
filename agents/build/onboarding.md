# Onboarding & Setup

> **Order of authority:**
>
> 1. [onboarding-paradigm.md](onboarding-paradigm.md) — the settled model
>    (three relationships, codes as the fallback for a missing channel, two
>    tiers of device trust, recovery as an ordinary join). That is the *intent*.
> 2. The code: `crates/virtues-improv/src/protocol.rs`,
>    `maintenance/ble_provision.rs`, `api/setup_phrase.rs`, `api/pair.rs`,
>    `api/box_status.rs`, `api/getting_started.rs`, `api/identity.rs`,
>    `apps/web/src/routes/(onboarding)/setup/`,
>    `apps/web/src/lib/components/setup/`,
>    `apps/web/src-tauri/ui/connect.html`,
>    `apps/web/src/routes/(public)/display`.
> 3. This file — what is *built*.

Imaging and manufacturing live in [appliance-image.md](appliance-image.md),
written against the boot chain as measured on hardware.

---

## Doctrine

Four rules everything else follows from.

1. **The channel picks the path — no one is ever asked.**
   Flashed Virtues hardware ⇒ kiosk enabled in the image. `curl
   virtues.com/sh` ⇒ CLI handoff (the user is, by definition, in a shell).
   There is no "monitor or CLI?" prompt and no separate dev domain.

2. **The app owns onboarding; the CLI owns infra; the screen mirrors state.**
   One setup/status state machine lives on the box and is rendered three ways:
   the 7" front panel, the app, and `virtues status`. The CLI never hosts an
   account/billing/OAuth conversation — a TTY is the worst possible medium for
   those.

3. **Setup transport ≠ long-term reachability.**
   Setup needs exactly two things: the setup device and the box sharing a local
   link, and the box having *outbound* internet. Remote reachability (iroh over
   the relay — [relay-control-plane.md](../archive/relay-control-plane.md)) is assessed
   *after* setup, on the network where the box actually lives. Overlays/VPNs are
   never mentioned during setup.

4. **Auto-enable nothing, auto-notice everything.**
   Virtues never installs, configures, or recommends a transport — but it
   notices the ones the user already has. The SSH session you are typing in
   becomes the handoff's local-forward hint; a user-run overlay becomes
   "Available via your own network" in the reachability verdict. Disclosure
   climbs a ladder, never jumps: silent (preflight) → one verdict line
   (handoff) → evidence-triggered hint (90s, nobody arrived) → intent-triggered
   options (`[Fix remote access]`) → forensics (`virtues doctor`). Weather
   reports, not errors; never say "wait"; never block forward motion on network
   class.

---

## The one wire

Everything the setup device says to an unclaimed box rides **one Bluetooth
conversation** (Improv, extended). This is the single most important thing to
get right.

`crates/virtues-improv/src/protocol.rs` holds the wire format once; the box
re-exports it and a round-trip test builds every command as a client and parses
it as the box. The BLE *client* stack (`btleplug`) is behind the `client`
feature — **the box must never carry it**.

| RPC | Command | What it does |
|---|---|---|
| `0x86` | `ClaimSetup { phrase, label }` | **The gate.** Four-word phrase + the setup device's own name. Opens a session bound to this connection; drop the link and the session dies. Nothing else is permitted until this succeeds. |
| `0x01` | `WifiSettings` | Join a network. Cleartext — the accepted setup-window risk. |
| `0x81` | `EnterpriseSettings` | 802.1X variant. |
| `0x04` | `ScanWifi` | The **box's** own scan, so the owner picks from what the box can hear, not what the phone can. |
| `0x02` / `0x03` | `Identify` / `DeviceInfo` | Stock Improv. |
| `0x82` | `ClaimGrant { grant }` | **Account link.** The app carries a short-lived single-use grant *to* the box; the box polls atlas with it. Outbound-only — atlas never gains a path in. |
| `0x83` | `PairConsume { kind, source, label, endpoint_id }` | **Pair — codeless.** Session-authorized: the box fetches its own standing code and redeems it against its own consume endpoint over loopback, streaming the response back. First device only; a successful pair claims the box and the reconciler stops the BLE service. |

State bytes: `0x01` AuthorizationRequired · `0x02` Authorized · `0x03`
Provisioning · `0x04` Provisioned. Errors: `0x01` InvalidPacket · `0x02`
UnknownCommand · `0x03` UnableToConnect · `0x04` NotAuthorized.

> **`0x84` and `0x85` are retired and never reused** — a stale client sending
> one gets `UnknownCommand`, which is the honest answer, and a test asserts it.
> Both read a code *off* the box for the app to carry back: the grant (`0x82`)
> inverts the account-link round-trip, and the codeless `0x83` does the pair
> hand-off box-internally, so **nothing reads a code off the box**. `0x83`
> needs no code field either: it sits behind `needs_session`, and a session is
> only opened by the phrase printed on the box's own screen — the proof of
> line of sight is already made.

`0x83` exists at all because pairing's LAN leg dies on hostile networks: client
isolation at an office blocked `POST /api/pair/consume` between phone and box on
the same wifi (live, 2026-08-11) while BLE sat there working.

---

## The two journeys

### Appliance (flashed hardware, 7" non-touch display)

```
power on
  → boot ~10s (no desktop session, no wait-online)
  → display: the box's number ("Virtues 4812") · "Get Virtues for your
    computer" · the FOUR-WORD PHRASE
    (rotates 15 min + 5 min grace while unclaimed; freezes forever at first claim)
  → owner opens the app → Setup: Welcome · Letter · Account (sign in first —
    the grant must cross BLE before pairing)
  → Server: nearby boxes listed by that number; types the phrase (0x86 — the
    session gate; line of sight = authority)
  → KEEP: the phrase is shown back and must be copied or written down — the
    only way back in if every paired device is lost
  → Wi-Fi: the BOX's own scan (0x04, 802.1X via 0x81); owner picks and types
    the password; the join is WATCHED over BLE (0x01)
  → account grant (0x82); the box polls atlas. A failed grant does not block:
    the Subscription step links through the box afterwards.
  → pair: codeless 0x83. Nothing is typed and nothing is read off the glass.
  → display flips to the ambient screen; Setup continues in the app
```

Ethernet removes the wifi step — the box is online from boot; phrase, account
link, and pair still ride BLE.

The CLI is never seen. The **display is output-only** — the digitizer does not
work through the cover glass, so nothing is ever typed on it.

**Breakglass, unadvertised:** a box whose Bluetooth is dead in the field can
revive the setup AP by touching `/var/lib/virtues/enable-setup-ap`
(`maintenance::setup_ap::AP_BREAKGLASS`). Its client is `/api/provision/*` plus
the airlock's LAN path — **not** a browser page (see
[Paths ruled out](#paths-ruled-out)).

### DIY / headless (`curl virtues.com/sh | sudo sh`)

```
installer: deps → db → user → env → binary → systemd → health check
  → execs `sudo -u virtues virtues init` (plumbing: migrations + handoff)
  → handoff block:
      wordmark (skipped when the installer already printed it)
      the 6-digit pair code, "Rotates automatically · valid while shown"
      [if over SSH] the `ssh -L` local-forward recipe
      "Don't have the app yet?  https://virtues.com/downloads"
        — with the plain statement that a browser cannot pair
      "If the app can't find this box, give it an address:" + mDNS name,
        raw LAN IP, loopback, and the global IPv6 origin when there is one
  → user enters the code in the desktop or mobile app
```

**The handoff never prints a pairing URL.** A browser cannot pair — an
allowlisted iroh key is the credential and a tab holds none, so
`/api/pair/consume` rejects `kind: "browser"` and the `/pair` page exists only
to say so. A pairing link would send the one reader without the app to a dead
end, as the last thing the installer prints. A test
(`handoff_block_offers_no_browser_pair_link`) holds that shut.

What the addresses are *for* is the app's "enter its address" field, when mDNS
does not carry.

---

## The phrase and the pair code

Two secrets, deliberately never on the glass at the same time.

### The four-word phrase (`api/setup_phrase.rs`)

The Bluetooth setup key **and** the recovery key, because those were never two
things.

- **Unclaimed** → on the panel, rotating every **15 min** with a **5 min**
  grace. Rotation is what stops a box left unclaimed for a week from being a
  permanent key on display for every houseguest with a camera.
- **Claimed** → **freezes** and leaves the screen forever. It freezes rather
  than being replaced, so what the owner saved is exactly what they typed.

Four words from a ~400-word list is ~2^34.6; capping words at 7 characters (so
the phrase always fits one 585px line) costs 50 words and lands at ~2^33.8.
Verification is rate-limited **globally on the box** — 10 attempts per 15 min —
because a BLE central can change its address between attempts, so per-device
throttling is theatre.

That asymmetry is what makes the case button safe: anyone who can open the case
can reset a box (a nuisance — the data survives), but only someone with the
phrase can *claim* it.

### The rotating pair code (`api/pair.rs`, `maintenance::pair_rotator`)

Six digits, shown `123 456`. Digits beat a letter alphabet because the primary
surface is a human typing what they read.

- **Rotates every 15 min with a 5 min overlap**, so a code read mid-rotation
  never dies under the user. Multi-use within its window, unlike the single-use
  "+ Add Device" token.
- **It is alive only while the box is UNCLAIMED.** On claim it is retired
  (`expire_standing_codes`) — an always-live multi-use code on a claimed box is
  a permanent brute-forceable password. The rotator keeps looping so a reset
  back to unclaimed re-arms it.
- **Stored hashed, handed over proximate channels only.** Only `SHA-256(code)`
  matches; the raw value is kept encrypted so `virtues pair` in the box's own
  terminal can print it. **Never served over the LAN.**
- **The panel does not render it, and there is no state where it should.**
- **No QR for the code.** The desktop app has no camera. A QR is fine for a
  *public* value and never for a secret.

---

## The display

`/display`, one responsive route in the one UI codebase. The canvas is
**585 × 329 CSS px**, not 1920×1080.

**States, in the order they outrank each other:**

| Rank | State | Shows |
|---|---|---|
| 0 | button held | a countdown, because something is about to happen |
| 0 | updating | the server is going away on purpose |
| 0 | no data disk | the box booted so that it could say exactly this |
| 1 | unclaimed, factory | get the app, and the four words that let it in |
| 2 | unclaimed, frozen | reset box: the words are the ones you saved |
| 3 | session live | the words are spent; who is setting up, instead |
| 4 | claimed | ambient — devices, record lines, clock |

The three at rank 0 are **interruptions, not steps** — most sharply the last,
where an ambient "REACHABLE · 3 devices syncing" over a missing disk would spend
the entire reason the OS lives on the eMMC.

**There is no QR on this screen.** It pointed a phone at the download page, and
setup is a desktop job — scanning it handed the page to the wrong device.
Dropping it also let the phrase go to one line, which is what makes it readable
across a room while you type it on another machine.

`/api/display/state` is **loopback-only**, enforced in the handler rather than
by router placement, because it carries the live phrase. Proximity is the
authority: a stranger on the wifi who cannot see the screen must not be able to
claim the box.

The top line is the box's name from `api/identity.rs::face_label`: "Virtues
4812" (a number from its machine-id, `codename::box_number`) until it has an
owner, then the person and the assistant side by side, "Nick · Ari". The radio
name stays machine-shaped (`Virtues-4812`).

**Never trust this panel's EDID.** It claims 53 × 30 cm (~24"), so WebKit
computes ~92 DPI against a real 315 and renders the UI 3.28× too small — body
text at 1.4 mm. The kiosk therefore sets **zoom = `mode_width / 585`, derived
from the connected connector's pixel mode**. A *pinned* 3.28 was correct only
for a 1080p panel and is not what ships.

---

## The kiosk (appliance only)

- **Runtime:** `cage` (Wayland kiosk compositor) + **WebKit** on bare DRM — no
  X, no desktop session. On Ubuntu 24.04 arm64 `chromium-browser` is a snap
  transition stub, which would put a second self-updating release channel under
  ours; `cog`/WPE has no arm64 build; `libwebkit2gtk-4.1` is a first-class deb
  and already Tauri's Linux webview.
- **Unit:** `virtues-display.service`, written by the installer.
- **Detection as guard, not decision:** the image always ships the unit; an
  `ExecStartPre` guard (`grep -qx connected /sys/class/drm/*/status`) keeps a
  headless board from crash-looping. The same image works with and without a
  screen. There is no `virtues panel enable|disable` — use systemctl;
  `virtues doctor` reports the unit's state in its Appliance ledger.
- **No touch (decided).** Every input happens on the setup device. The physical
  control is the case button (`maintenance/reset_button.rs`).
- **Hygiene:** dark theme default, dim/sleep schedule, burn-in-safe layout — it
  runs 24/7.

**The kiosk caches the SPA.** After an upgrade the panel can draw the old
interface until `systemctl restart virtues-display`.

---

## The airlock

**`apps/web/src-tauri/ui/connect.html`**, compiled into the app binary. It is
the first-device path only where the app does not bake the SPA — Windows,
Linux, Android — and the recovery page everywhere it is still linked
(`connect.html#reset`, `#unreachable`). An unpaired **iPhone** opens `/setup`
from its own baked copy (`src-tauri/src/lib.rs`), and an unpaired **Mac** does
the same (`main.rs`, `tauri.macos.conf.json`), handing over to the server's
copy at the same step after pairing; a paired Mac that cannot reach its server
opens `/reconnect` from its baked copy.

**It is served from the BINARY**, before the OTA overlay and before the baked
assets. An airlock must not depend on packaging, and must not be *overridable*
by it either: `tauri ios dev` never runs `beforeBuildCommand`, so a
packaging-served connect screen went stale on every dev build.

**Desktop and Windows are first-class FIRST devices.** A computer does the
Bluetooth wifi step itself, which is the right default anyway: 802.1X
credentials and a checkout page both want a keyboard. `tauri.windows.conf.json`
overlays the macOS-shaped base config (RFC 7396 — arrays replace) with `nsis`
targets, an `.ico`, no collector sidecar, and `createUpdaterArtifacts: false`.
Windows has no collector and no tray — it is a viewer that can also be the
setup instrument.

**macOS will not do Bluetooth from `tauri dev`.** TCC attributes permission to
an app *bundle*, so a bare dev binary aborts with SIGABRT, no dialog, nothing on
stdout. Embedding an `Info.plist` in the executable does not satisfy it (tried).
BLE work needs `pnpm tauri build --debug` then `open` on the bundle — launching
the inner binary directly fails the same way, because LaunchServices is what
confers bundle identity. In a browser, `/setup?radio=fake` walks the first half
against a scripted server (`setup/prepair.svelte.ts`).

**Register.** Light, serif, ink — transcribed by hand from `themes.css`
`:root`, because the SPA's theme system does not exist when this page draws.
`∴ Virtues` top-left; `Server ID · <label>` top-right once a specific machine is
in play (`setServer`), where the label is what the box advertises ("Virtues
4812"), matching the panel for the two-servers-in-one-house case. **"Server",
not "box"**, in every user-facing string; code identifiers and comments still
say box.

`offerUpgrade` runs at the end: a box is flashed at manufacture and then sits in
a warehouse, so an owner's first minute is often spent on code older than
everything they just read about.

---

## Setup

**Setup is one linear flow**, at `/setup/[[step]]`, from the box on the desk to
the app. The order is `ORDER` in `apps/web/src/lib/components/setup/setup.svelte.ts`:

```
Welcome → Letter → Account → Server → Wi-Fi → Names → Subscription → Connections → Timeline → Interview → ∴ → app
```

- **Everything through Subscription is required**; Connections, Timeline and
  Interview each carry *Skip* (next step) and *Finish later* (open the app now).
  Subscription has no skip — a subscription, an existing account, or the
  person's own AI each settle it.
- **The first half** (Account, Server, Wi-Fi, pairing) runs only on a device
  that has no server yet and carries the SPA itself: the iPhone and the Mac
  (`setup/prepair.svelte.ts`). Anywhere else — a browser, a paired device —
  those steps are done by construction.
- **Resume is derived, never stored.** Each in-app step reads its status from
  `GET /api/getting-started`, whose steps are `STEP_IDS` in
  `virtues-core/src/api/getting_started.rs`: `connect_ai` (Subscription),
  `introductions` (Names), `connect_world` (Connections), `timeline`,
  `interview`. `/setup` alone lands on the first step still to do.
- **After the app opens**, any step not done keeps a **Setup** tile on the rail
  whose panel lists the same steps; it is the same process continued.
- **Names** names the assistant; after an owner exists the box goes by that
  household's names (`identity::box_label` in lists — "Nick's server";
  `face_label` on the glass). There is no codename step and no box-naming step.
- **Interview** is one question per screen over the same interviewer chat and
  close (`write_it_up`, which writes the "In your own words" article); drawn
  chapters from Timeline are sent as the first answer.

The app gate (`routes/(app)/+layout.ts`) redirects to `/setup` on
`onboarding_status` only — `onboarding`, or not `active` with
`onboarding_complete` false — never on `setup_complete`, which on an appliance
also requires the account and would loop an account-less box. Setup's close
sets `active`.

`/onboarding/[[view]]` is a **308 redirect to `/setup`**, kept rather than
deleted: old step URLs are in browser histories, and SPA delivery is OTA — a
bundle baked before the rename can meet a box after it. `/founders-letter`
stays as a read-only re-read of the letter.

### `/api/setup/state` (`api/box_status.rs::compute_setup_state`, public)

```
setup:      claimed · account · network
setup_complete:  appliance → claimed + account
                 DIY      → claimed only
onboarding: device_named · device_collecting · first_source · living_source ·
            first_device · first_phone · chat_imported · remote_access ·
            first_sync · narrative_identity_ready
onboarding_complete: first_source OR device_collecting
onboarding_status:   new | onboarding | active   (app_user_profile)
```

`setup_complete` uses a positive **allow-list**, so a newly-added step is
non-gating by default — you opt a step *into* blocking the gate, never
accidentally out of it. `network` is deliberately excluded: it is an
informational weather-report the user cannot "do", and it flips false on any
transient LAN blip, which used to bounce a fully-set-up user back into setup.

`account` gates the **appliance only** (`setup_ap::is_appliance()`). A DIY box
is somebody's own server, and forcing an account on it contradicts "prescribe,
never enforce" outright — which it did, with no exit, for exactly the users the
doctrine was written for.

Signals are **derived**, never stored as a wizard-progress table: claimed =
unrevoked device rows · account = API key in the box vault · network = a primary
IP · first_source = an active non-device credential · remote_access = **iroh
relay registered** · first_sync = a successful applet run ·
narrative_identity_ready = the narrative-identity ARTICLE exists
(`wiki_articles`, subject_type `narrative_identity`) — the document is the one
artifact. Derivation means the state survives re-installs, restores, and
out-of-band changes.

Note the deliberate split on `claimed`: `compute_setup_state` **counts** the
`local-console` device row, while `pair::paired_device_count` **excludes** it.
The questions differ — "is there any session here" versus "did a human bring a
device to this box". Counting it naively once made a fresh box report itself
claimed from the moment it powered on, silently disabling the whole appliance
onboarding path.

`make dev` sets `VIRTUES_DEV_SKIP_SETUP=1` to pre-satisfy the wizard. Never set
in prod.

**Entitlement is one step with three exits** (`StepSubscription.svelte`):
subscribe, sign in to an account that already pays, or bring your own AI. The
first BYO key saved during setup from the pairing device needs no command-line
approval (`settings_byo.rs`, `first_key_during_setup`) — an appliance owner has
no command line.

### The four laws

Written after a page-by-page review against Arc's and Dia's onboarding: each
page was well made alone, and together they read as four web pages with a
progress bar over them. A new step obeys them before it ships.

1. **One stage.** Paper and grain are painted once by the route
   (`.setup-stage`, `lib/components/setup/setup.css`); steps crossfade on it
   and never fade through a blank page.
2. **One mark.** The ∴ is the progress (`SetupMark`): three dots for the three
   thirds of Setup, filling as each third is done, the current one breathing.
   Welcome's big ∴ flies up to become it; the close brings it down whole and
   the app opens beneath it. No dots row, no "next" label.
3. **One motion.** Three durations, one ease, one spring for presses, one text
   entrance, one exit (`setup.css` tokens, `motion.ts` for Svelte). Every
   button gives under the finger.
4. **One voice and one alignment.** Titles, sentences and the way forward on
   the center line (`StepFrame` centers by default); copy through
   [voice.md](voice.md).

---

## Network edge cases

| Situation | Behavior |
|---|---|
| **Client-isolated wifi** (offices, hotels) | This is what BLE is for on the appliance path — `0x83` exists because client isolation blocked `/api/pair/consume` between a phone and a box on the same wifi while Bluetooth sat there working. **DIY/SSH:** the session the installer ran in is the transport. `ssh -L 8000:localhost:8000 user@box` and the forwarded browser authenticates as the **loopback console** (`middleware/auth.rs` — peer is loopback and not proxied), so it lands *in*; it does not "pair", and does not need to. The box stays on its own uplink: the multi-GB model pull never rides the forward. |
| **Wifi-only first boot** (appliance, no ethernet) | Shipped, over Bluetooth: the app reads the box's own scan (`0x04`), sends credentials (`0x01`), and watches the join. No AP, no join-QR, no captive page. |
| **Bluetooth dead in the field** | Breakglass only: touch `/var/lib/virtues/enable-setup-ap` to revive the setup AP. Client is `/api/provision/*` + the airlock's LAN path. Unadvertised by design. |
| **Onboarding venue ≠ deployment venue** | Fine by design: reachability is re-assessed wherever the box is plugged in. Setup never depends on inbound reachability. |
| **Two boxes on one LAN** | mDNS auto-suffixes; panel and CLI print the box's *actual* name, never hardcoded copy. Before an owner, each box shows its own number ("Virtues 4812") on its glass, in its radio name, and in the app's list, so matching four digits tells them apart. |
| **mDNS-hostile clients** | Every printed handoff includes the raw-IP fallback line. |

---

## CLI surface

Three human verbs:

| Verb | Question it answers |
|---|---|
| `virtues pair` (aliases `login`, `link`) | *get me into my box* — prints the standing 6-digit code and waits. No URL, no QR: a browser cannot pair. |
| `virtues status` | *how is it* — the textual mirror of the panel, same state machine. `--json` for pasting. |
| `virtues doctor` | *what's wrong* — three ledgers (Inference, Reach, Appliance) and one verdict. Never binds the iroh endpoint and treats the DB as optional, so it answers when other things are broken. Every issue names a runnable remedy; exit code is the diagnosis. |

Everything else is plumbing or admin: `init` (auto-run by the installer:
migrations + handoff), `migrate`, `backup`/`restore`, `volumes`, `upgrade`,
`prepare`/`activate`, `rollback`, `channel`, `reindex`, `configure-inference`,
`uninstall`, `sudo`, `server`. `subscribe` and `account-login` survive as hidden
power-user commands. See [recovery.md](recovery.md) for the operator runbook.

**Wrong-user self-correction:** on a box install, DB-touching commands re-exec
as the `virtues` service user (Unix-socket peer auth maps OS user → Postgres
role). Miss one and it runs as the login user, whose role does not exist in the
cluster — `virtues reindex` died with `role "root" does not exist` after an
hour-long restore. Permanent Postgres auth errors fail fast with the
`sudo -u virtues` hint instead of a 30-second fake timeout.

---

## Paths ruled out

Each of these was built, failed on hardware or with people, and is gone. Do not
rebuild one without answering the failure.

- **A captive portal / browser setup flow.** It could provision wifi and then
  strand the owner one step from the end, because pairing needs a held iroh key
  that a browser tab does not have — it served a user who cannot exist. Its
  failures (iOS rendering the SPA as a blank captive sheet, force-reopening it,
  caching a stale portal page per-SSID across upgrades) were all on OS surfaces
  we cannot patch.
- **The setup AP as the main path.** AP+STA concurrency does **not** work on
  the Q6A despite what `iw list` advertises, so the switchover is sequential;
  an AP raised while "unclaimed" lands on the one radio holding the owner's
  wifi and drops the box off it. The AP rises only while unclaimed **and**
  offline, which is breakglass-only in practice.
- **The app joining the setup AP itself** (`NEHotspotConfiguration`). The
  screens were unreachable in practice.
- **QRs on the panel.** A wifi join-QR's camera-banner presentation failed on
  hardware; an app QR hands the download page to a phone when setup is a
  desktop-or-app job. A QR never carries a secret.
- **Two secrets on screen at once.** The first person shown the pair code and
  the wifi password together typed the code as the password — a *sequence*
  presented as a *set*. The panel shows the phrase and never the pair code.
- **A download link on a box with no uplink.** Never offer a link at the one
  moment the device reading it cannot follow it.
- **A codename for the box.** A two-word name ("Quaint Tern") read as a mystery
  to testers; nobody knew it was the server. Before an owner the box goes by a
  number; after, by the household's names.

---

## Open questions

1. **Resume token.** Setup-device↔box session continuity across network
   handoffs — the flow must resume, not restart. Note the `0x86` session is
   bound to the BLE *connection*, so today a dropped link means retyping the
   phrase.
2. **Stripe on LAN.** The account gate's checkout poll loop — confirm the
   success-URL redirect story or keep poll-only.
3. **Airlock/atlas drift.** The airlock ships in the app bundle but talks to
   live atlas; define the compat posture (min-version check → "update your box
   first").
4. **Drop-off visibility vs privacy.** No telemetry means we never learn where
   onboarding loses people. Decide deliberately: nothing, or an explicit opt-in
   at the end.

Settled: screen hardware (the 7" panel on the Q6A —
[npu-hardware-findings.md](../record/npu-hardware-findings.md)) and the naming
step (the assistant's name, not the box's — reach is by EndpointId).
