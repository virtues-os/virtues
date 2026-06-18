# Onboarding & Setup

> Canonical spec for how a human goes from "box in hand" to "Virtues running
> and earning its keep" — across both hardware tiers, with and without a
> screen. Decided 2026-06-12; supersedes the TTY-wizard model of `virtues init`.

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
   WireGuard, see [networking.md](networking.md)) is assessed *after* setup,
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

### Appliance (flashed Virtues hardware, 8" non-touch screen)

```
power on
  → splash (static image; covers Chromium warm-up)
  → panel: QR + "open adam-jace.local from any device on your network"
  → phone scans QR → lands in the setup wizard, already trusted
  → wizard (on the phone): account → subscribe → name the box → done
  → panel ticks each step live, then flips to the ambient dashboard
```

The CLI is never seen. The screen is never *required* — it displays the same
URL/QR any browser on the LAN can reach.

### DIY / headless (`curl virtues.com/sh | sudo sh`)

```
installer: deps → db → user → env → binary → systemd → health check
  → execs `sudo -u virtues virtues init` (plumbing: migrations + handoff)
  → ONE handoff block: mDNS URL · IP fallback · loopback · expiry · verdict line
  → user opens the URL from any browser on the LAN → same wizard
```

The SSH session the installer ran in is itself the fallback transport on
hostile networks — `ssh -L 8000:localhost:8000 user@box` from the laptop
puts the wizard at `http://localhost:8000`. (Power users: an existing
OpenSSH session can add the forward live — type `~C`, then
`-L 8000:localhost:8000`.)

Idiomatic headless-server convention (Proxmox/Home Assistant/Pi-hole): one
`curl`, one URL. A terminal ANSI QR next to the URL gives parity with the
appliance (P2).

## The QR / link carries a capability, not just an address

`http://<LAN-IP>:8000/setup#t=<one-time-token>`

- **LAN IP, not mDNS, inside the QR** — phones (especially Android) fumble
  `.local`. The *printed/displayed* name leads with mDNS for humans.
- **The token rides in the URL fragment** (never hits logs/referers).
  Scanning **is** pairing — the phone lands trusted.
- **Trust-on-first-boot:** the wizard is gated on the token, and the token
  only exists on the physical screen (appliance) or in the installer's
  terminal (headless). Proximity = authority — consistent with the
  `virtues sudo` physical-presence doctrine. A LAN stranger cannot claim an
  unclaimed box.

## Setup vs onboarding (they are different things)

- **Setup = the wizard. It ends early.** Required core only:
  **claimed** (token consumed) → **account + subscription** → **named**
  (`adam-jace.local`, sets the Avahi hostname) → **on your network ✓**.
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
- **P3 — kiosk:** `/panel` route; cage+Chromium unit; splash; DRM guard.
- **P4 — appliance image:** flash with kiosk enabled; AP-mode wifi
  onboarding; naming step end-to-end.
- **P5 — reachability surface:** `[Fix remote access]` tiered flow
  (IPv6-direct first-class → Advanced: BYO transport via
  [byo-networking.md](byo-networking.md)).

## Open questions (design-time; none block P1)

1. **Pre-auth exposure:** audit what `/setup` serves before the token is
   presented (box name? version? state?) — minimize to the claim screen.
2. **Resume token:** phone↔box session continuity across network handoffs
   (hotspot → home wifi; AP-mode switchover) — wizard must resume, not
   restart. Panel shows "Setup in progress on Adam's iPhone · [start over]".
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
