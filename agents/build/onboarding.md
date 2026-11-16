# Onboarding and setup

> **Order of authority:**
>
> 1. [onboarding-diy-plan.md](../plan/onboarding-diy-plan.md): the current
>    design, for a person who installs the server on their own always-on
>    machine and then connects the app to it. That is the intent.
> 2. The code: `virtues-core/src/main.rs` (`print_link_output`),
>    `virtues-core/src/cli/link.rs`, `virtues-core/src/api/pair.rs`,
>    `virtues-core/src/maintenance/pair_rotator.rs`,
>    `virtues-core/src/api/box_status.rs`,
>    `virtues-core/src/api/getting_started.rs`,
>    `virtues-core/src/api/identity.rs`,
>    `apps/web/src/routes/(onboarding)/setup/`,
>    `apps/web/src/lib/components/setup/`,
>    `apps/web/src/lib/components/recovery/reconnect.svelte.ts`,
>    `apps/web/src-tauri/ui/connect.html`.
> 3. This file: what is built.

Virtues sells no hardware. Setup has one path: install the server from a
terminal, then pair the app with the code the install prints.

---

## Four rules the design follows

1. **How the user arrived determines the setup path; we never ask.** A user
   who ran `curl virtues.com/sh` is already in a shell, so the server's half
   of setup happens there and ends with a code to type into the app. There is
   no "which kind of setup?" prompt and no separate dev domain.

2. **The app handles onboarding and the CLI handles infrastructure.** One
   setup state lives on the server (`/api/setup/state`) and both the app and
   `virtues status` read it. The CLI never runs an account, billing or OAuth
   flow, because a terminal is a poor interface for those.

3. **The transport used for setup is separate from long-term reachability.**
   Pairing needs the device and the server to reach each other once, on the
   local network or through an SSH forward. Remote reachability (iroh over the
   Virtues relay) is reported after setup, on the network where the server
   lives. Overlays and VPNs are never mentioned during setup.

4. **Auto-enable nothing, auto-notice everything.** Virtues never installs,
   configures or recommends a transport, but it detects the ones that exist.
   The SSH session the install ran in becomes the `ssh -L` hint in the
   handoff; a user-run overlay (Tailscale, WireGuard) becomes "Available via
   your own network" in the network summary (`net_check.rs`, `detect_byo`).
   Network detail is shown in steps, never all at once: nothing during the
   installer's preflight, the SSH hint in the handoff, a hint after 90 seconds
   in which nobody paired, and full detail in `virtues doctor`. The
   **headline is a weather report**: it states a fact about the network and
   never instructs, blames or says "wait". Instructions live in the guidance,
   which only `virtues doctor` prints. Network state never blocks the user
   from moving forward.

---

## Installing the server

```
curl -sSL https://virtues.com/sh | sudo sh
  installer (tools/virtues-installer/src/flow.rs): preflight → inference mode
    → packages, security updates, mDNS name → user and database → release
    → install manifest and env file → bring-up → systemd unit → restart
  → execs `sudo -u virtues env VIRTUES_NO_BANNER=1 virtues init`
      (plumbing: config from the env file, migrations, then the handoff)
  → handoff (print_link_output in virtues-core/src/main.rs):
      "Your server is ready", "Get the app" with https://virtues.com/downloads,
        and the 6-digit code with how many minutes it lasts
      a terminal QR code of the line "http://<LAN IP>:8000   123 456"
      [over SSH] the `ssh -L` forward recipe (cli/link.rs, ssh_handoff_block)
      "Don't have the app yet?" with the downloads link, and the plain
        statement that a browser cannot pair
      "If the app can't find this server, paste one of these lines into it:"
        each address the server answers on, each followed by the code, plus
        the global IPv6 origin when there is one
```

Linux is the only server platform today. Servers on a Mac or Windows machine
are a separate plan (slice 6 in the DIY plan).

**The handoff never prints a pairing URL.** A browser cannot pair: the
credential is an allowlisted iroh key and a browser tab has none.
`POST /api/pair/consume` accepts only the app, sensor, CLI and agent kinds,
and the `/pair` page (`apps/web/src/routes/(auth)/pair/+page.svelte`) only
explains this. A pairing link would send a user without the app to a dead end.
The test `handoff_block_offers_no_browser_pair_link` in `cli/link.rs` enforces
it.

**LAN IP, not mDNS, inside the QR.** Phone cameras and Android in particular
resolve `.local` names unreliably, so the QR line uses the raw LAN address
(`cli/link.rs`, `forward_host`). The QR holds the same short-lived code as the
printed line and never a key.

---

## The pair code

`api/pair.rs` and `maintenance/pair_rotator.rs`. Six digits, shown `123 456`.
Digits because the main use is a person typing what they read.

- **While the server is unclaimed** there is one standing code. It is
  multi-use, and the rotator mints a new one every 15 minutes
  (`STANDING_ROTATE_INTERVAL_MIN`); the old one stays valid for 5 more
  (`STANDING_GRACE_MIN`), so a code read just before a rotation does not
  expire while the user types it.
- **At the first pairing** every standing code is expired
  (`expire_standing_codes`). A multi-use code that stays live on a claimed
  server is a permanent password anyone on the network can guess at. The
  rotator keeps running, so a server reset back to unclaimed gets a code
  again.
- **After that**, `virtues pair` and `virtues device add` mint a one-time
  code that lasts 30 minutes (`CLI_REDEEM_TTL_MIN`). `cli_pair_code` picks
  between the two. Codes minted from inside the app ("+ Add Device") last
  5 minutes.
- **Stored hashed.** Only `SHA-256(code)` is matched. The standing code's raw
  value is also stored encrypted so that `virtues pair` on the server can
  print it. It is never served over the network.
- **Guessing is limited per source IP**: 10 consume attempts per 30 minutes
  (`middleware/rate_limit.rs`). There is no server-wide lockout, because that
  would let anyone on the network lock the owner out.

Claimed means "a human brought a device": `pair::paired_device_count` and
`pair::is_unclaimed` exclude the loopback console row and agent devices.
`is_unclaimed` returns false on a database error, because a claimed server
wrongly treated as unclaimed would hand out a standing code.

Before an owner exists the server calls itself "Virtues 4812", a number from
its machine-id (`codename::box_number`), in the app's server list and in
`/api/identity`. After pairing it goes by the owner's name ("Nick's server",
`identity::box_label`). There is no box-naming step.

---

## The connect page

`apps/web/src-tauri/ui/connect.html` is compiled into the app binary and
served from it (`src-tauri/src/lib.rs`), ahead of any OTA overlay or baked web
assets, so a stale web bundle can never replace it. It is vanilla HTML with no
build step, because it draws before the SPA exists.

Who opens it:

| Device | Unpaired | Paired |
|---|---|---|
| Mac | `/setup` from its own baked copy (`src-tauri/src/main.rs`) | its own copy; `/reconnect` when the server refuses or does not answer |
| iPhone | `/setup` from its own baked copy; `connect.html#existing` only for the QR scanner | `connect.html`, which sends it on to its server |
| Windows, Linux | `connect.html`: network search, address and code, pairing | the server's copy of the app through the loopback, or `connect.html#reset` / `#unreachable` |
| Android | `connect.html` | `connect.html` |

The scanner (`#existing`, `acceptScanned`) accepts two things: the server's
QR line, which it splits with `Router.splitPasted` and pairs by address, and
a paired computer's handoff QR (`plugin:reach|pair_handoff_accept`). After a
phone pairs it opens `/setup/connections`.

`connect.html` keeps its own copy of the pasted-line split and the error
wording in `setup/pairing.ts`, because it loads nothing. Both go when Windows
and Linux carry their own copy of the app (dated TODO in `pairing.ts`).

Its look is copied by hand from `themes.css` `:root` for the same reason.
Every user-facing string says "server"; code identifiers and comments still
say box.

The reconnect screen (`apps/web/src/lib/components/recovery/reconnect.svelte.ts`,
route `/reconnect`) runs from the device's own copy and tells apart four cases:
the server is back, the server refuses this device (pair again), this device
is offline, and no answer. It checks the device's own pairing first, since a
missing or unreadable key is fixed only on the device.

---

## Setup

**Setup is one linear flow** at `/setup/[[step]]`. The order is `ORDER` in
`apps/web/src/lib/components/setup/setup.svelte.ts`:

```
Welcome → Server → Names → Connections → Subscription → Arrival → app
```

The founder's letter is not a step: the pair code lasts about 15 minutes,
so nothing stands between the person and it. Arrival offers it as "A note
from us"; Setup in the app keeps it to reread (`IN_APP`), and
`/founders-letter` shows it on its own.

Every step after Welcome is a sheet on the desk, with the Tuscany print
faded along the bottom in the theme's ink. The ∴ at the top is the
progress. Arrival is the close: the ∴ comes down, the print gives way to
the painting in color (`/scenes/lake.jpg`, the home painting), and the
person opens the app or the letter themselves.

- **Server** (`steps/StepServer.svelte`) searches the network
  (`plugin:reach|discover`, a subnet scan, so it works where mDNS does not),
  asks "Which server is yours?" when it finds more than one, and otherwise
  asks for the address and code. The code is six boxes in the terminal's
  monospaced face; one pasted line from `virtues pair` fills the address and
  all six, wherever it is pasted. Once paired, the server appears for a
  moment (its name, set large) before Setup moves on by itself. Pairing is `prePair.pairByAddress` (`prepair.svelte.ts`), which calls
  the reach plugin's `pair`; `pairing.ts` turns failures into sentences.
- **Server runs only on a device that has no server yet** and carries the
  app itself: the iPhone and the Mac. Anywhere else it is already done. In
  dev, `/setup?radio=fake` walks it in a browser against a scripted server
  (`&nearby=diy` puts one on the fake network, `&nearby=none` finds nothing).
- **Connections** is this device's permissions. A device that has just paired
  lands here even when the server is already set up (the
  `virtues-just-paired` key), so a second device stops after this step.
  Connections is the only optional step: it carries Skip and Finish later.
- **Subscription** (`StepSubscription.svelte`) has three exits: subscribe,
  sign in to an account that already pays, or use your own AI. Sign-in is part
  of this step. Saving your own AI key needs the app's owner proof
  (`api/owner_proof.rs`), which the app signs itself, so the owner never needs
  a command line for it. Pairing and collecting need no account.
- **Names** names the person and the assistant, straight after the server,
  so the assistant has a name before any step uses it.
- Chapters and the interview are not part of setup.

**Resume position is derived, never stored.** Each in-app step reads its
status from `GET /api/getting-started`, whose steps are `STEP_IDS` in
`virtues-core/src/api/getting_started.rs`. Setup reads `connect_ai`
(Subscription), `introductions` (Names) and `connect_world` (Connections).
`STEP_IDS` still lists `timeline` and `interview` so older apps find every
step they know. `/setup` alone lands on the first step still to do. After the
app opens, an unfinished step keeps a Setup tile on the rail.

The app gate (`apps/web/src/routes/(app)/+layout.ts`) redirects to `/setup` on
`onboarding_status` only: when it is `onboarding`, or when it is not `active`
and `onboarding_complete` is false. Setup's close sets `active`.

`/onboarding/[[view]]` is a 308 redirect to `/setup`. Old step URLs are in
browser histories, and SPA delivery is OTA, so a bundle baked before the
rename can meet a newer server. `/founders-letter` stays as a read-only copy
of the letter.

### `/api/setup/state` (`api/box_status.rs::compute_setup_state`, public)

```
setup:               claimed · account · network
setup_complete:      claimed
onboarding:          device_named · device_collecting · first_source ·
                     living_source · first_device · first_phone ·
                     chat_imported · remote_access · first_sync ·
                     narrative_identity_ready
onboarding_complete: first_source OR device_collecting
onboarding_status:   new | onboarding | active   (app_user_profile)
```

`setup_complete` uses a positive allow-list, currently `claimed` alone, so a
new step does not block the gate unless someone adds it. `account` is not
required, because the server is the owner's own. `network` is not required:
the user cannot act on it, and it goes false on any brief network drop.

Every signal is derived from existing rows, and there is no wizard-progress
table, so the state survives reinstalls, restores and changes made elsewhere:
claimed = unrevoked device rows · account = the subscription link in the vault
· network = a primary IP · first_source = an active non-device credential ·
remote_access = the iroh endpoint is homed on the relay (or reachable on the
LAN with the relay switched off) · first_sync = a successful applet run ·
narrative_identity_ready = the narrative-identity article exists in
`wiki_articles`.

`compute_setup_state` counts the loopback console device row as claimed, while
`pair::paired_device_count` excludes it. The questions differ: "is there any
session here" versus "did a human bring a device". Counting the console row
in `paired_device_count` would make a fresh server report itself claimed from
the moment it starts.

`make dev` sets `VIRTUES_DEV_SKIP_SETUP=1` to mark every setup step done.
Never set it in production.

### Four rules for the setup flow

These came from a page-by-page comparison with Arc's and Dia's onboarding:
each of our pages worked on its own, but together they looked like separate
web pages with a progress bar on top. A new step must follow all four before
it ships.

1. **One shared background.** The route paints the paper and grain background
   once (`.setup-stage`, `lib/components/setup/setup.css`); steps crossfade on
   top of it and never fade through a blank page.
2. **One progress indicator.** The ∴ mark shows progress (`SetupMark.svelte`):
   its three dots are the three thirds of Setup's steps, each filling as its
   third is done, with the current one pulsing. Welcome's large ∴ animates up
   to become it; the close brings it back down whole and the app opens
   beneath it. There is no separate row of dots and no "next" label.
3. **One set of motion values.** Three durations, one easing curve, one spring
   for presses, one text entrance, one exit (`setup.css` tokens, `motion.ts`
   for Svelte). Every button visibly responds when pressed.
4. **One writing style and one alignment.** Titles, body text and the forward
   button sit on the center line (`StepFrame.svelte` centers by default).

---

## Network edge cases

| Situation | Behavior |
|---|---|
| **Client-isolated wifi** (offices, hotels) | The SSH session the install ran in is the transport. The handoff prints `ssh -L 8000:localhost:8000 user@server` and "give the app this address: http://localhost:8000", with port 18000 as the fallback when 8000 is busy. The app pairs through the forward. A browser opened at the forwarded address arrives over loopback TCP, which an installed server treats like any unpaired device; a browser uses `ssh -L 8000:/run/virtues/console.sock` instead, which is the owner for a login in `virtues-console` (`console_socket.rs`). |
| **Nobody pairs within 90 seconds** | `virtues pair` (`wait_for_new_device`) prints a hint that the network may block device-to-device traffic: the forward recipe over SSH, otherwise a hotspot or a network you control. |
| **The app's search finds nothing** | Server falls back to address and code. Every printed handoff lists the raw LAN IP beside the mDNS name. |
| **Server set up in one place and moved** | Reachability is assessed wherever the server runs. Setup never depends on inbound reachability; remote access is the server dialing out to the relay. |
| **Two servers on one LAN** | mDNS adds a suffix, and the CLI prints the server's actual name. Before an owner, each server's number ("Virtues 4812") appears in the app's list, so the four digits tell them apart. |

---

## CLI surface

Three human verbs:

| Verb | Question it answers |
|---|---|
| `virtues pair` (aliases `login`, `link`) | *Connect a device.* Prints the code, how long it lasts, the QR code and the addresses, then waits for a device (`--no-wait` to exit at once). |
| `virtues status` | *How is it.* The server's state; `--json` for pasting into a support thread. |
| `virtues doctor` | *What is wrong.* Inference, Reach, Data disk and Diagnostics sections, then one summary line. It never binds the iroh endpoint and treats the database as optional, so it still answers when other parts are broken. Every issue names a command that fixes it, and the exit code is 1 on an error. |

Everything else is plumbing or admin: `init` (hidden, run by the installer),
`device`, `migrate`, `backup`/`restore`, `volumes`, `upgrade`,
`prepare`/`activate`, `rollback`, `channel`, `reindex`, `configure-inference`,
`uninstall`. `subscribe` and `account-login` remain as hidden commands, and
`reset --keep-data` (hidden) revokes devices so setup opens again. See
[recovery.md](recovery.md) for the operator runbook.

**Wrong-user self-correction:** on an installed server, commands that touch
the database re-exec as the `virtues` service user, because Postgres uses
Unix-socket peer auth and the login user has no role. Permanent Postgres auth
errors fail fast with the `sudo -u virtues` hint.

---

## Paths ruled out

Do not rebuild one of these without first addressing why it failed.

- **Setup in a browser.** A browser can reach the server but cannot pair,
  because pairing stores an iroh key that a browser tab cannot hold. A browser
  flow leaves the user one step from the end.
- **A pairing link in the install output.** Same reason. The output points to
  the app download instead.
- **A codename for the server.** A two-word name ("Quaint Tern") meant nothing
  to testers. Before an owner the server goes by a number; after, by the
  owner's name.
- **Setup without a terminal** (Bluetooth provisioning, a setup hotspot, an
  on-server screen). Built for the hardware Virtues no longer sells and
  removed with it. The reasoning is in
  [onboarding-paradigm.md](../archive/onboarding-paradigm.md).

---

## Open questions

The DIY plan's open decisions are the current list: whether the installer
keeps renaming the machine to `virtues`, and where chapters live
([onboarding-diy-plan.md](../plan/onboarding-diy-plan.md#open-decisions)).
Also open:

1. **Windows and Linux on their own copy.** Until they carry the app
   themselves, they pair on `connect.html` and keep its duplicate pairing
   code.
2. **Drop-off visibility versus privacy.** With no telemetry we never learn
   where setup loses people. Decide: nothing, or an explicit opt-in at the end.
