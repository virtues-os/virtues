# Onboarding for a server you install yourself

Status: slices 1 to 4 built on wave, 5 and 7 partly; nothing tested on a
real device yet. Decided 2026-11-15: Virtues sells no hardware, so
setup is designed for a person who installs the server on their own machine
that stays on, from its terminal, and then connects a Mac or iPhone app to it.
The desktop app never runs the server. This plan supersedes the hardware parts
of [setup-plan.md](../archive/setup-plan.md) and [build/onboarding.md](../build/onboarding.md).

## The flow

1. **Install, in the server's terminal.** `curl -sSL https://virtues.com/sh | sudo sh`.
   The end of the install prints the pairing code, how long it lasts, the
   server's addresses, and (from slice 4) a QR code for the iPhone.
2. **Connect to your server.** The app's first screen: address and code on a
   Mac (a pasted `virtues pair` line fills both), a QR scan on an iPhone.
3. **This device.** Mac: Full Disk Access with the drag tile, then
   Accessibility and call audio if wanted. iPhone: Location, Health,
   Calendars. Collecting starts here, before any account, which the server
   already allows: pairing and ingest have no billing check.
4. **Account and AI.** Email sign-in and the $20 subscription on one screen,
   Virtues AI or your own, with one line on what leaves the server.
5. **Names**, then the app opens.

Chapters and the interview are not part of setup (owner, 2026-11-15). A second
device stops after step 3. The sketch is the Claude Design canvas
"Onboarding, server first".

## Slices

### 1. Install output and pairing

Built on wave (2026-11-15).

Server and CLI only. Useful to Linux installs today.

- End of install (`print_link_output`, `virtues-core/src/main.rs`): the
  code's real lifetime instead of "valid while shown", and "Get the app"
  instead of "Desktop app". The QR code moved to slice 4: printed before the
  iPhone app can read it, every scan would fail.
- One code lifetime, derived from `STANDING_ROTATE_INTERVAL_MIN`,
  `STANDING_GRACE_MIN` and the one-time TTL, used by the CLI and by the error
  copy in `connect.html`.
- On a server with no setup phrase, the first pairing no longer logs an error
  about the panel (`api/pair.rs`, `freeze_current`).
- The "running at" log matches `[::]` (`server/mod.rs`).
- No server-wide limit on failed pair attempts: `random_pair_code` in
  `api/pair.rs` records why it was rejected (it lets an attacker lock the
  owner out).
- False comments and help text: `api/pair.rs` header, `middleware/rate_limit.rs`,
  `cli/types.rs` (`Pair`), `tools/virtues-installer/src/install.rs` (https),
  `cli/link.rs` header.
- Dead code: `wait_for_pair`, the always-zero credential count from
  `revoke_all_devices`.

### 2. Setup order and steps

Partly built (wave, 2026-11-15): the new order, chapters and the interview out
of setup, and the device step's counts removed. The sign-in is already on the
subscription step. A phone paired to a set-up server now opens its device
step (`connect.html` hands off to `/setup/connections`); a computer gets the
same with slice 3. Left: the hardware-only parts (decision 1). The server's `STEP_IDS` stay as they are
so older apps still read every step they know.

`apps/web/src/lib/components/setup/`, `virtues-core/src/api/getting_started.rs`.

- New `ORDER` on a self-installed server: `server`, `connections` (the device
  step), `subscription` with the email sign-in folded in, `names`. The server's
  `STEP_IDS` follows: `connect_world`, `connect_ai`, `introductions`.
- `timeline` and `interview` leave setup, client and server, including the
  interview line in the getting-started prompt block. The two step components
  stay behind a dated TODO until chapters have a home.
- Hardware-only parts (Bluetooth prompt, "Setting up Virtues hardware?", the
  Wi-Fi and Account dots) show only on hardware, or go, per decision 1.
- A device that has just paired gets its device step even when the server is
  already active, using the `virtues-just-paired` key.
- Fix the header comments in `setup.svelte.ts` and `+page.svelte`.

### 3. The Mac stops using `connect.html`

Built on wave (2026-11-15), checked in the dev fake, not yet paired on a real
Mac. `StepServer` pairs by address and code (`prePair.pairByAddress`,
`pairing.ts`), then Setup opens this device's step; `handOff` and the
connect-page hand-offs are deleted. Left: `takeCarried` and the `?intro` rule
in `inApp.ts` (dated TODO), and `connect.html` for Windows and Linux.

- The manual address and code form moves into `StepServer.svelte`, with the
  pasted-line split and the error mapping from `connect.html`.
- After pairing the Mac stays in its own copy of the app (drop the
  `localhost:7117` fallback in `connect.html`) and opens `/setup/connections`,
  as the phone does, so a computer added to a set-up server gets its device
  step.
- `connect.html` stays for Windows and Linux with a dated TODO.
- Delete `prePair.handOff` and `bakedDesktop` once nothing uses them.

### 4. The iPhone scans the server's QR

Built on wave, not yet scanned on a phone. `virtues pair` prints a terminal QR
code (`print_qr_block` in `cli/commands/deploy.rs`, now black on white on a
terminal) of the same "address   code" line it prints for pasting, never a
key. The phone's scanner (`connect.html`, `acceptScanned`) reads that line
with `Router.splitPasted` and pairs by address; anything else goes to the
existing computer handoff. Release order: the app with the scanner change
must reach phones before a server release prints the QR, or every scan fails. No `virtues://pair`
link: the Mac already takes a pasted line, and `virtues://` is the scheme the
app serves its own UI on.

### 5. Manual and agent docs

Partly done (wave, 2026-11-15): `devices/mac.md`, `devices/iphone.md`,
`devices/windows.md`, the end of `install.md` and `index.md`. The remaining
hardware lines wait on decision 1, since owners of existing hardware still
read them; the agent-doc moves wait on `agents/plan/README.md`, which another
change has open.

After the code ships, since the manual claims only what ships. Rewrite
`docs/setup/devices/*.md` and the end of `docs/setup/install.md`; remove the
hardware lines in `docs/index.md`, `install.md`, `inference.md`,
`requirements.md`, `accelerators.md`, `operate/recovery.md` and
`operate/reach.md`. Done 2026-11-16: `onboarding-paradigm.md`,
`appliance-image.md`, `setup-plan.md`, `display-plan.md` and the display
records are in `agents/archive/`, and `build/onboarding.md` describes this
path.

### 7. Delete the hardware setup path

Decided 2026-11-15 (owner: "Delete hardware path"): no hiding behind
appliance mode. Done on wave: the app's Setup and reconnect screen
(`dca81a02c`), the manual (`316f67dc5`), and items 1 and 2 below
(`6fbd07d1e`, not yet built for iOS). Left, in this order, each its own
commit:

1. `connect.html`: the Bluetooth find, four words, Wi-Fi and owner screens
   (about 220 mentions), keeping address-and-code pairing and the scanner.
2. The reach plugin and its clients: the `improv_*` and `reach_rehome`
   commands (`plugins/reach/src/commands.rs`, `lib.rs`), `ReachPlugin.swift`
   and `ImprovClient.swift`, the Bluetooth usage string in
   `src-tauri/Info.plist`, `crates/virtues-improv`, and the owner and
   provisioning parts of `crates/virtues-reach-client`. Needs an iOS build.
3. The server: `maintenance/ble_provision.rs`, `api/setup_phrase.rs` (and a
   migration dropping `app_setup_phrase`), `maintenance/setup_ap.rs`,
   `maintenance/reset_button.rs`, `api/provision.rs`, the panel's phrase
   minting in `api/display.rs`, and the appliance flags they read in
   `install_manifest.rs`.
4. The installer's appliance profile (`tools/virtues-installer`, about 70
   references).

Item 3 is done (`4481b9939`), widened by the owner on 2026-11-15 to the
display panel and Settings › Network's Wi-Fi: the HTTP owner proof moved to
`virtues_iroh::owner_proof`, migration 0076 drops `app_setup_phrase` and
`app_display`, and CI's Improv wire check went (`7e26efd08`). The installer's
appliance mode and appliance detection went in `a0142c761`, and
`crates/virtues-improv` and `bluer` in `4e032edd8`. Still left: the appliance
image tooling (`cli/deprovision.rs`, `cli/image_check.rs`,
`tools/build-dragon.sh` and the imaging scripts). Every apt install turns on
security-only automatic updates (`c8f074c4e`, owner's call 2026-11-15). Existing Dragons have Postgres relocated to a
data disk, so the code that reads a relocated cluster on upgrade stays.

Not covered by the decision, so ask first: runtime hardware support (the
NPU engine `virtues-qnnd`, display faces and the kiosk). Item 3 overlaps it:
Settings → Network joins Wi-Fi through `api/provision.rs`
(`perform_join_full`), the panel reads the setup phrase and the reset button
(`api/display.rs`), and the server's HTTP owner proof lives in
`virtues-improv::protocol`, which has to move to another crate before the
Bluetooth half of that crate can go.

### 6. Servers on a Mac or Windows machine

A separate plan. Needs a launchd and a Windows-service lifecycle, per-OS state
and config roots, Postgres and search per OS, and checks that the machine
stays on. The server and installer have a few hundred Linux and systemd
assumptions.

## Open decisions

1. Existing hardware owners: keep the Bluetooth, four-word and Wi-Fi path
   behind appliance mode, or delete it.
2. Whether the installer stops renaming a self-installed machine to `virtues`
   (`install.rs`, `configure_mdns`). `virtues.local` depends on the rename.
3. Whether Welcome and the founder's letter stay in setup or the letter moves
   to the install page.
4. Where chapters live now that the interview is out of setup.
