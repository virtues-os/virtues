# Onboarding & Setup

> Canonical spec for how a human goes from "box in hand" to "Virtues running
> and earning its keep" — across both hardware tiers, with and without a
> screen. Decided 2026-06-12; supersedes the TTY-wizard model of `virtues init`.

## As built — read this before the rest (2026-08-07)

Most of the doctrine below survived contact with hardware. Four specifics did
not, and the document still argues for them further down. Where they conflict,
this section is right.

**A browser cannot pair.** The iroh pivot landed after this spec was written and
made a held Ed25519 key the credential. A browser tab has none, so `/pair` is
purely informational and every "open the URL on your phone and pair" flow below
is counterfactual. The app pairs; the browser cannot. The one exception is a
browser running *on the box*, which authenticates as the loopback console.

**The panel is called the `display`**, it is **not touch**, and the canvas is
**585 × 329 CSS px**. The digitizer does not work through the cover glass, so
the display is output-only — the pair code is *shown* there and typed elsewhere.
The 7" panel's EDID claims 53 × 30 cm (~24"), so every DPI heuristic renders the
UI 3.28× too small; the kiosk pins `devicePixelRatio`. Never trust EDID here.

**The kiosk is `cage` + WebKit, not Chromium.** On Ubuntu 24.04 arm64
`chromium-browser` is a snap transition stub, which would put a second
self-updating release channel under ours; `cog`/WPE has no arm64 build.
`libwebkit2gtk-4.1` is a first-class deb and is already Tauri's Linux webview.

**Wifi comes from the phone over a setup AP, not from the panel.** The box hosts
`Virtues-XXXX` (WPA2 — the owner's home password crosses that link), the display
shows a join QR, and the phone collects the credentials. AP+STA concurrency does
**not** work on the Q6A despite what `iw list` advertises, so the switchover is
sequential.

**The AP is up while the box is unclaimed AND offline** (revised 2026-08-10).
The earlier rule — up until a device *pairs*, full stop — could not work on this
radio. Pairing happens after provisioning (every `/api/provision/*` route 404s
once a device pairs, so it cannot go the other way), which means the moment
after a successful join the box is online, unclaimed, and its AP is down. The
reconciler saw "unclaimed, no AP" and raised one onto the single radio holding
the association it had just formed, dropping the box off the owner's wifi ~20s
after joining it — before anyone could pair. The worry the old rule encoded
(tearing the AP down while the phone is still on it) is covered by
`PROVISIONING_LOCK`, and on hardware that cannot do AP+STA "box is online" and
"phone is on our AP" are mutually exclusive anyway. Bonus: an ethernet box no
longer raises a setup network it never needed.

**The app is the wizard, and there is no app-less onboarding — because there is
no app-less pairing** (decided 2026-08-10, after a day on hardware). Pairing
requires a held iroh key, so an app-less user who provisioned wifi through a
captive portal still could not finish; the portal served a user who cannot
exist. So the app drives everything: its connect screen offers *Set up a new
box*, joins `Virtues-XXXX` itself (`wifi_join` → `NEHotspotConfiguration`,
prefix-matched on `Virtues-` so the owner types only the passphrase off the
display), runs the wifi picker natively, waits out the switchover, re-finds the
box, and moves to the pair code — one continuous session.

**The captive sheet is suppressed, not exploited.** The box now answers every
OS connectivity probe with its vendor's exact success token, so the Captive
Network Assistant never opens. The reversal is earned: on hardware the CNA
rendered our SPA as a blank sheet, force-reopened it, refused to let the owner
leave, and cached a stale portal page per-SSID across a box upgrade. Every
failure was on an OS surface we cannot patch. `/portal` (plain server-rendered
HTML, no JS) survives as the unadvertised manual hatch — join the AP by hand,
open `10.42.0.1` — for Android (until `WifiNetworkSpecifier` lands) and
laptops. The old `/provision` URL 301s to it server-side, because phones cache
captive URLs per-SSID and only a redirect can un-teach them.

**The display shows one job at a time** (three states): offline + unclaimed →
*get the app* (app QR — shown while the phone still has internet, the only
moment it can be followed — plus the AP passphrase the app will ask for);
online + unclaimed → *claim me* (app QR + the pair code); claimed → ambient.
The wifi join-QR was removed outright: its camera-banner presentation failed
twice on hardware, and the app path never needs it. The two
setup screens were one screen until 2026-08-10, and it carried two different
secrets at once — the first person shown it read the pair code and typed it as
the wifi password. Labelling helped and did not fix it: the fault was presenting
a *sequence* as a *set*. It also offered `virtues.com/downloads` to a phone it
had just told to join a network with no uplink, which is the one moment that
link cannot be followed.

Built and running on hardware: `/display`, `/api/display/state` (loopback-only —
it carries the live pair code), `/provision`, `/api/provision/*` (AP-subnet +
unclaimed, re-checked per request), the setup-AP lifecycle, captive-portal
detection, `--appliance` install profile, `virtues deprovision`, and per-unit
first-boot minting. Built but **not yet exercised on hardware**: the app-side
wifi picker and the three-state display. Not yet built: the iOS
`NEHotspotConfiguration` flow — note it is *not* Personal Hotspot and *not* the
hard-to-get `NEHotspotHelper` entitlement, and that it prompts rather than
joining silently, so it buys continuity rather than fewer taps.

**Delivery is: flash a pre-baked image, then `virtues upgrade` on first
connect.** Provisioning happens on our bench, never the customer's — their box
has no network until onboarding finishes, and onboarding needs the software
already installed. `deprovision` strips per-unit identity before imaging,
because the iroh secret *is* the box's identity and clones of an
un-deprovisioned master are literally the same box.

## Doctrine

Four rules everything else follows from:

1. **The channel picks the path — no one is ever asked.**
   Flashed Virtues hardware ⇒ kiosk enabled in the image. `curl
   virtues.com/sh` ⇒ CLI handoff (the user is, by definition, in a shell).
   There is no "monitor or CLI?" prompt and no separate dev domain.

2. **The web owns onboarding; the CLI owns infra; the screen mirrors state.**
   One setup/status state machine lives on the box and is rendered three
   ways: the 8" front panel, the phone wizard, and `virtues status`. The CLI
   never hosts an account/billing/OAuth conversation — a TTY is the worst
   possible medium for those.

3. **Setup transport ≠ long-term reachability.**
   Setup needs exactly two things: the phone and box sharing a local link,
   and the box having *outbound* internet. Remote reachability (IPv6-direct +
   the blind relay, see [networking-relay-tee.md](networking-relay-tee.md)) is assessed *after* setup,
   on the network where the box actually lives, and reported by the honest
   `net_check` verdict. Overlays/VPNs are never mentioned during setup — BYO
   transport lives behind the post-setup `[Fix remote access]` → *Advanced*.

4. **Auto-enable nothing, auto-notice everything.**
   Virtues never installs, configures, or recommends a transport — but it
   always notices the ones the user already has and folds them into copy and
   verdicts: the SSH session you're typing in becomes the handoff's
   local-forward hint; a user-run overlay (`tailscale0`) becomes "Available
   via your own network" in the reachability verdict. Disclosure climbs a
   ladder, never jumps: silent (preflight) → one verdict line (handoff) →
   evidence-triggered hint (90s, nobody arrived) → intent-triggered options
   (the dashboard's `[Fix remote access]` — the *only* place BYO is
   mentioned) → forensics (`virtues doctor`). And the copy rules: weather
   reports, not errors; never say "wait"; never block forward motion on
   network class — "move the box later; it re-checks automatically."

## The two journeys

### Appliance (flashed Virtues hardware, 7" non-touch display)

As built. The flow below is what runs on hardware today; the version this
document originally described (open a URL on the LAN, pair in a browser, name
the box) assumed a browser could pair, which it cannot.

```
power on
  → boot ~10s (no desktop session, no wait-online)
  → display SCREEN 1 "Get the Virtues app": app QR + the AP passphrase
  → owner installs the app (phone still online), taps "Set up a new box",
    types the passphrase off the screen
  → the APP joins Virtues-XXXX itself (one "Wants to Join" dialog)
  → app shows the BOX's scan list (cached pre-AP), owner picks + types their
    home wifi password in a native field
  → sequential switchover: AP down, join, AP back up ONLY if it failed
  → phone glides back to home wifi; app re-finds the box on the LAN
  → display SCREEN 2 "In the Virtues app, enter …": the 6-digit code
  → owner types the code → paired
  → display flips to the ambient screen
  (manual hatch, unadvertised: join the AP from wifi settings, open 10.42.0.1
   → /portal, plain HTML — Android + laptops)
```

Ethernet skips straight to screen 2: the box is online from boot, so no setup
network is ever raised and the whole middle disappears.

The CLI is never seen. The **display is output-only** — the digitizer does not
work through the cover glass, so nothing is ever typed on it.

> **The pair code is typed, never scanned.** The primary client is the desktop
> app, which has no camera. QR is used for public payloads only — the app
> download link and the `WIFI:` join string — never for the code.

### DIY / headless (`curl virtues.com/sh | sudo sh`)

```
installer: deps → db → user → env → binary → systemd → health check
  → execs `sudo -u virtues virtues init` (plumbing: migrations + handoff)
  → ONE handoff block: mDNS URL · IP fallback · loopback · expiry · verdict line
  → user enters the code in the desktop/mobile app (NOT a browser — a
    browser holds no iroh key and cannot pair)
```

The SSH session the installer ran in is itself the fallback transport on
hostile networks — `ssh -L 8000:localhost:8000 user@box` from the laptop
puts the wizard at `http://localhost:8000`. (Power users: an existing
OpenSSH session can add the forward live — type `~C`, then
`-L 8000:localhost:8000`.)

Idiomatic headless-server convention (Proxmox/Home Assistant/Pi-hole): one
`curl`, one URL. A terminal ANSI QR next to the URL gives parity with the
appliance (P2).

## The universal rotating code

One short code pairs everything — the desktop app, the phone wizard, the CLI.
Digits only (6, shown `123 456`): the primary surface is a human reading a code
off a screen and typing it, so digits beat a letter alphabet on a numeric pad.

- **It rotates and is always live.** A box-side task (`maintenance::pair_rotator`)
  mints a fresh code every ~15 min with a ~5-min **overlap window**, so the
  panel/CLI always show a valid code and a code read mid-rotation never dies
  under the user. The code is **multi-use within its window** — it can pair
  several devices over its life (unlike the single-use "+ Add Device" token).
- **Stored encrypted, shown only on physical surfaces.** Only `SHA-256(code)` is
  used for matching; the raw code is kept encrypted (vault key) so box-local
  surfaces — the `/display` render and `virtues pair` in the box's terminal — can
  *display* it. It is **never served over the LAN**. Proximity = authority,
  consistent with the `virtues sudo` physical-presence doctrine: a LAN stranger
  who cannot see the screen cannot claim the box.
- **No QR *for the code*.** The desktop app has no camera, so the typed code is
  the one mechanism for pairing. QR is used elsewhere on the display and that is
  not a contradiction: a QR is fine for a **public** value (the app download
  link, the `WIFI:` join payload for the setup AP) and never for a secret. The
  distinction is what the payload is worth to a stranger, not the encoding.

## Setup vs onboarding (they are different things)

- **Setup = the wizard. It ends early.** Required core only:
  **claimed** (a device paired) → **account + subscription** → **named**
  (`my-box.local`, sets the Avahi hostname) → **on your network ✓**.
  Target: under 5 minutes of human time, minimal input, each step a visible
  win on the panel. The bge-m3 model pull (GBs) runs in the background and is
  shown as a step ("Downloading AI models — one-time"), never a silent hang.
- **Onboarding = the first week, owned by the dashboard.** A "next wins"
  checklist: connect first source → first sync lands → pair your phone →
  remote access ✓ → first chat. Progressive, abandonable, resumable. The
  wizard hands off to this instead of front-loading everything.
- **Entitlement is a pluggable step.** Today it's Stripe/$20-mo (the OAuth
  proxy and AI wallet need virtues-api). The future $0/BYO-key DIY branch is
  one new variant of that step in the state machine — designed-for now,
  built later.

## Network edge cases

| Situation | Behavior |
|---|---|
| **Client-isolated wifi** (offices, hotels — phone can't reach box's LAN IP) | Detect: outbound ✓ + wizard unvisited after N min (+ mDNS self-probe fails). **DIY/SSH:** the session the installer ran in is the transport — `ssh -L 8000:localhost:8000 user@box-ip` from the laptop, then open `http://localhost:8000/pair#t=…`. The box stays on its own uplink: the multi-GB model pull never rides a hotspot; only the pairing page rides the forward. **Phone/appliance:** *"This network blocks devices from talking to each other. Use your phone's hotspot, or a network you control. You can move the box afterward."* Setup-scoped copy only — no VPN/overlay talk at the moment of maximum fragility (an SSH forward is not an overlay). |
| **Wifi-only first boot** (appliance; no ethernet) | Pilot batch: "plug in ethernet for the 10 minutes of setup." v1.1: AP-mode — box boots its own AP, screen shows a `WIFI:` join-QR (iPhone camera joins natively), wizard collects home-wifi creds, box switches over. DIY users have networking by definition (they SSH'd in). |
| **Onboarding venue ≠ deployment venue** (set up at the office, lives at home) | Fine by design: reachability is re-assessed wherever the box is plugged in; the verdict updates. Setup never depends on inbound reachability. |
| **Two boxes on one LAN** | mDNS auto-suffixes; panel and CLI always print the box's *actual* name, never hardcoded copy. |
| **mDNS-hostile clients** | Every printed handoff includes the raw-IP fallback line. |

## The kiosk (appliance only)

- **Runtime:** `cage` (Wayland kiosk compositor) + Chromium pointed at the
  `/panel` route of the existing SvelteKit app. **One UI codebase** — the
  panel is a responsive route, not a second frontend. Slint (native
  Rust-to-DRM) is the fallback card if a screen ever ships on a GPU-less
  tier or boot-reliability demands it.
- **Detection as guard, not decision:** the appliance image always ships the
  kiosk unit; it starts only if a DRM connector reports `connected`. The same
  image works headless. Overrides: `virtues panel enable|disable` (DIY
  opt-in / appliance opt-out).
- **No touch (decided):** the panel is display-only; every input happens on
  the phone. (A physical confirm *button* is a future hardware option for
  sudo-approve.)
- **Three jobs:** first-boot setup (QR + live step mirror) · ambient status
  (reachability verdict, devices, syncs, storage) · failure honesty (the
  `net_check` verdict + one action — never a spinner).
- **Hygiene:** dark theme default, dim/sleep schedule, burn-in-safe layout —
  it runs 24/7.

## CLI surface (the collapse)

End state — three human verbs:

| Verb | Question it answers |
|---|---|
| `virtues login` | *get me into my box* — prints the pair/setup URL + QR (absorbs `link`; "login" matches the human's intent, "link" described our mechanism) |
| `virtues status` | *how is it* — the textual mirror of the panel (same state machine) |
| `virtues doctor` | *what's wrong* — hardware/model/network deep report |

Everything else is plumbing/admin: `init` (auto-run by the installer:
migrations + handoff print — its interactive account/subscribe middle moves
to the web wizard and dies in the TTY), `migrate`, `backup`/`restore`,
`upgrade`, `uninstall`, `sudo`, `server`. `subscribe` and atlas-`login`
fold into the wizard and survive only as hidden power-user commands.

`sudo virtues uninstall`: computed manifest (probed, never guessed),
typed-hostname confirmation, `--keep-data` dev tier, `--purge-models`,
`--force` for CI. Shared infra (Postgres server, Avahi) always stays; the
llama-server inference sidecars are ours and are removed.

Wrong-user self-correction: on a box install, DB-touching commands re-exec
as the `virtues` service user (Unix-socket peer auth maps OS user → Postgres
role); permanent Postgres auth errors fail fast with the `sudo -u virtues`
hint instead of a 30s fake timeout.

## The state machine (P1 spine)

One source of truth on the box; three renderers.

```
GET /api/setup/state
{
  "setup": [                       // the wizard — required core
    { "id": "claimed",  "title": "Box claimed",        "done": true },
    { "id": "account",  "title": "Virtues account",     "done": true },
    { "id": "named",    "title": "Box named",           "done": false },
    { "id": "network",  "title": "On your network",     "done": true }
  ],
  "setup_complete": false,
  "onboarding": [                  // the first week — dashboard "next wins"
    { "id": "first_source",  "title": "Connect a source",      "done": false },
    { "id": "first_device",  "title": "Pair a device",         "done": false },
    { "id": "remote_access", "title": "Reachable from anywhere","done": false },
    { "id": "first_sync",    "title": "First data synced",     "done": false }
  ]
}
```

Signals are derived, never stored as a separate "wizard progress" table:
claimed = paired devices exist · account = billing token in the box vault ·
named = hostname differs from the default · network = `net_check` ·
first_source = an active non-device credential · remote_access = global IPv6
verdict · first_sync = a successful action run. Derivation means the state
survives re-installs, restores, and out-of-band changes.

## Build phases

- **P0 — quick wins** ✅ (shipped): one-block handoff + verdict line;
  mDNS-first URLs; wrong-user fail-fast + re-exec; `uninstall`.
- **P1 — the spine:** `/api/setup/state` + state machine module; `virtues
  status` renders it; then the `/setup` web wizard (account, subscribe,
  naming move out of the TTY).
- **P2 — CLI parity:** `login` verb rename; terminal ANSI QR;
  client-isolation hint; installer handoff final polish.
- **P3 — kiosk:** ✅ shipped as the **display**, not `/panel`: `/display`
  route, `cage` + **WebKit** unit (not Chromium — see "As built"), DRM-connector
  guard so the same image boots headless. Splash still outstanding.
- **P4 — appliance image:** partially shipped. `--appliance` install profile,
  setup AP, `/provision`, captive-portal detection and `virtues deprovision`
  are built and running on hardware. The image itself (flash → deprovision →
  `dd` → clone) is specified but not yet cut. The naming step was cut entirely
  — reach is by EndpointId, so the box keeps its default name.
- **P5 — reachability surface:** `[Fix remote access]` tiered flow. Note the
  doctrine moved after this was written: reach is the blind relay, not
  IPv6-direct, and BYO transport is the power-user escape rather than a tier —
  see [networking-relay-tee.md](networking-relay-tee.md) §"LAN: no tunnel".

## Open questions (design-time; none block P1)

1. **Pre-auth exposure:** audit what `/setup` serves before the token is
   presented (box name? version? state?) — minimize to the claim screen.
2. **Resume token:** phone↔box session continuity across network handoffs
   (hotspot → home wifi; AP-mode switchover) — wizard must resume, not
   restart. Panel shows "Setup in progress on your iPhone · [start over]".
3. **Stripe on LAN:** the web wizard needs the checkout poll loop (today in
   the CLI's `handle_subscribe`); confirm success-URL→LAN-IP redirect or
   keep poll-only.
4. **Wizard/atlas drift:** the wizard ships in box firmware but talks to
   live atlas — define the compat posture (atlas min-version check →
   "update your box first").
5. **Drop-off visibility vs privacy:** no telemetry means we never learn
   where onboarding loses people. Decide deliberately: nothing, or an
   explicit opt-in "share anonymous setup diagnostics" at the wizard's end.
6. **Screen hardware:** resolution/connection (DSI/HDMI) bounds the `/panel`
   design and cage config — procurement-gated.
