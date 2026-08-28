# Settings › Display — the box's face

> A new settings section, under Devices, for the screen on the box: whether it
> is awake, what we know about it, and — the real feature — **what it shows**.
> The thesis of this doc: the panel is not a peripheral to configure, it is
> the box's face, and choosing what it shows is choosing which face the box
> wears. The word "face" already exists in this codebase for exactly the right
> thing.

## Where things stand today (surveyed 2026-08-26)

The full stack, end to end:

- **The kiosk** is `virtues-display.service` (written by the installer,
  `tools/virtues-installer/src/install.rs:1786-1826`, appliance profile only):
  cage + a Python/GTK WebKit shim (`display.py`, `install.rs:1853-2265`)
  pointed at `VIRTUES_DISPLAY_URL`, default `http://localhost:8000/display`.
  Zoom is **derived** (DRM mode width / 585, `install.rs:1964`), overridable
  via `VIRTUES_DISPLAY_ZOOM` in `virtues.env` — the one existing user-tunable
  display knob, and nothing writes it.
- **What it renders** is `apps/web/src/routes/(public)/display/+page.svelte` —
  a 585×329 CSS px, non-interactive, 24/7-dark state machine: button-held →
  updating → no-disk → bootmark → unclaimed → **claimed ambient** (the record
  census ticking up). The header comment of that file is the panel's design
  law: one idea per screen, no spinners, no animation loops.
- **The server cannot see the panel.** Every hardware fact (connector, mode,
  whether cage runs) lives in the Python shim. No EDID parsing, no
  brightness/DPMS/backlight anywhere in the repo. `agents/build/onboarding.md:388`
  already lists "dim/sleep schedule" as intended kiosk hygiene; nothing
  implements it. This entire half is greenfield.
- **`/api/display/*` is loopback-only by explicit doctrine**
  (`virtues-core/src/api/display.rs:1-22`) — it carries the setup phrase;
  proximity is the authority. A Settings page arrives over LAN/relay, so
  **new control endpoints go on the authenticated surface, not in that
  module** (its own doc comment argues a single exception is how the next one
  gets argued for).
- **Applet faces already close the loop.** A face is `face/index.html` in an
  applet dir — sandboxed iframe, injected `virtues.css`/`virtues.js`,
  `await virtues.query(sql)` as read-only `virtues_face_reader`, served at
  `/face/<applet_id>/` (`virtues-core/src/server/faces.rs`). A face-only
  applet is a complete applet. Chat authoring writes faces today
  (`tools/applet_setup.rs`, `face_html` arg, 48 KB cap). And critically:
  **the kiosk is loopback, so it authenticates as `local-console` and can
  mint face tokens itself** (`middleware/auth.rs:112-129`) — the panel can
  render any applet face at full trust with no pairing ceremony.
- Only 2 of 26 shipped applets have faces (`hello_world`/Biscuit,
  `calorie_tracker`), plus the checked-in exemplar
  `applets/user/heart_rate_explorer` — "a display-only dashboard of my
  entire heart rate history," which is this feature in embryo.

## The narrative frame

Three words are in tension: the codebase says **face** (applet HTML surface),
**view** (the SPA's full-page face route), and **display** (the panel). The
resolution is not to pick a neutral word — it is to notice the pun is load-
bearing. Applets have faces; the box's screen is literally the box's face.
The settings section is named **Display** (a plain device-class noun, sitting
comfortably in the You/Billing/Devices register), but its central object is
*the face the screen wears*.

Voice doctrine alignment (`agents/build/voice.md`):

- The bank already assigns a line to this surface: *"∴ The record of a life
  belongs where the life is lived"* — likely surface: **packaging, panel
  ambient**. The panel is a canonical voice surface; this section is where a
  person tends it.
- Artifacts, not features: copy says "the screen," "what it shows" — never
  "kiosk," "webview," "renderer."
- Section description, in register: **"The screen on the box, and what it
  shows."**

Design doctrine alignment (`agents/build/design.md`, `display/+page.svelte` header):
the panel is a hearth, not a dashboard. Calm is the product. Every option in
this section should read as *tending* something, not administering it.

## What the section contains

Four blocks, top to bottom, in one page (`maxWidth="wide"`, hand-rolled
sections per convention — there is no shared settings-row kit):

### 1. The screen right now

A live, bezel-true miniature of what the panel is showing at this moment —
the settings page mirrors the glass. Mechanically cheap (render the same
`/display` state, or literally iframe `/display` scaled into a 585×329-ratio
frame) and it does two jobs: presence-at-a-distance ("the box is fine, I can
see its face from my desk"), and it makes every choice below concrete —
change the face, watch the miniature change.

Beneath it, a `<dl class="facts">` (the UpdateSection pattern) of specs:

- Panel: connector, native mode, physical size *as claimed* — with honesty
  about the lie ("the panel claims 24″; the pixel mode is real, the inches
  are not" is doctrine stated three times in the repo; surface the mode,
  suppress the inches).
- Service: `virtues-display` active/inactive/not-installed (doctor already
  computes this vocabulary).
- Zoom: derived value, and whether an override is set.

And one verb: **Restart the screen** — the canonical field remedy
(`systemctl restart virtues-display`, currently folklore + a CLI hook in
`upgrade.rs:1128`) becomes a button. This alone pays for phase 1.

### 2. The face

The heart. A shelf of faces the screen can wear in its **ambient** slot —
a picker, previewed at true 585×329 before committing.

**Built-in faces** (each is an existing feed wearing clothes; all endpoints
exist today):

| Face | Feed | What it is |
|---|---|---|
| **The Record** | `/api/display/state` census | today's ambient — the count of a life, ticking |
| **The Day** | day summary | yesterday's opening line, each morning — "Every day, a page will be waiting for you," made literal on the glass |
| **On This Day** | `/api/wiki/on-this-day` | the anniversary surface |
| **The Clock** | `/api/wiki/lifeline/clock` | a life-clock on the hearth |
| **Weather** | `/api/weather/current` | the oldest ambient genre |
| **Biscuit** | `hello_world` face | a small dog who lives on your box |
| **Matte** | — | show nothing; black glass, deliberately, as a dignified choice — not "off," but reserve |

**Applet faces**: every applet where `has_face` is true, listed by name and
description. Choosing one hangs it in the ambient slot.

**And the door at the end of the shelf: "Ask for a new one."** Chat
authoring of faces already works end to end — the last item in the picker
opens a chat pre-seeded with the panel contract (585×329, non-interactive,
dark, no animation loops, `virtues.query` for data). *"Ask for a chart of
your resting heart rate and hang it on the box"* is the sentence that sells
the whole feature, and it costs a prompt template.

### 3. Hours

The dim/sleep schedule `onboarding.md` already promised. Two times ("sleeps
at / wakes at"), box-local. During sleep the panel goes dark — matte at
minimum, true backlight-off if the hardware allows (open question below).
Interruptions still wake it (see The Duty List).

A brightness slider only if the hardware audit finds a real backlight;
a fake software-dimming slider (CSS filter over an LCD whose backlight
still burns) is exactly the kind of asserted-not-real control the voice
doctrine forbids.

### 4. The duty list

A short read-only statement of when the screen interrupts whatever face it
wears — the existing precedence chain, disclosed: *updating · storage fault ·
button held · setup*. Nothing configurable. This is trust through
disclosure: the owner learns the glass will always tell the truth about the
box's condition, no matter what's hanging on it. It also permanently answers
"why did my face disappear during the upgrade."

## Architecture

### The keystone: the face hangs *inside* `/display`, not instead of it

Do **not** point `VIRTUES_DISPLAY_URL` at a face. The `/display` route owns
the state machine — updating latch, disk fault, button hold, setup — and
that precedence must survive any choice the owner makes. Instead:

- `/display`'s **claimed-ambient state** gains a tenant: when a face is
  configured, it renders that face in an iframe (kiosk is loopback →
  `local-console` → mints its own face token via
  `/api/applets/:id/face-token`); when none is configured, it renders The
  Record as today. Built-in faces are components inside `/display` itself
  (no iframe needed).
- The env var survives as the break-glass override it already is.

This is also the security answer. The deleted-QR-endpoint doctrine
(`display.rs:360-377`) warns against caller-supplied text reaching the
glass. A hung face passes because the *authority is the owner's paired,
authenticated session choosing it* — nothing external can push content to
the panel; the face itself stays in its jail (opaque origin, CSP, read-only
role, 5 s/5000-row caps).

### New surface (all authenticated, none in `api/display.rs`)

- `GET /api/system/display` — specs + service state + current config.
  Rust finally learns to see the panel: a small `/sys/class/drm` reader
  (connector, status, modes — mirror the shim's `_mode_width()` logic),
  minimal EDID parse for vendor/model only (never DPI), `virtues-display`
  unit state, plus the config below. Include `has_display` / appliance
  detection (`install_manifest::appliance()` exists in Rust but is exposed
  to the web app nowhere — this endpoint is where it surfaces).
- `PUT /api/system/display/config` — the face choice + hours.
- `POST /api/system/display/restart` — the verb, via the existing
  sudoers/`systemd-run` privilege model (`api/updates.rs:335-370`).
- The kiosk reads its config through **`DisplayState`** — extend the
  loopback endpoint's response with the chosen face (that module stays
  read-only-to-the-panel; the settings surface never calls it).

### Storage

A new singleton table, not a JSONB blob — this schema is shown to an LLM at
runtime, and the naming doctrine exists precisely so the model can read it:

```
app_display (singleton)
  face_kind        text      -- 'builtin' | 'applet' | 'matte'
  face_builtin     text      -- 'record' | 'day' | 'clock' | ...
  face_applet_id   text      -- FK-ish to app_applets
  sleep_start      time      -- box-local
  sleep_end        time
  is_enabled       boolean
  updated_at       timestamptz
```

(`app_` prefix = product state; `is_` boolean; claim the migration number
with `make migration` before writing SQL.)

`ui_preferences` JSONB was considered and rejected: it belongs to the
assistant profile and is read by the *SPA session* — the kiosk has no
session, and the panel's face is a fact about the box, not about a device's
preferences.

### Settings UI touch points

The four known ones, verbatim from the survey:
`lib/sidebar/modes.ts:35-71` (row after `devices`, icon e.g.
`ri:tv-2-line`), `SettingsView.svelte` `SECTIONS` + dispatch chain, and —
easy to forget — the **duplicated mobile list** in
`MobileSettingsView.svelte:57-67`, which is already out of sync with
desktop and will silently diverge again.

Name collision noted: `DisplaySettingsPopover.svelte` /
`pageDisplay.svelte.ts` are the *Pages editor's* typography popover.
The new section's component should be `DisplayView.svelte` under
`tabs/views/` per sibling convention; do not touch the popover.

### The fit problem — the one honest manifest question

Faces today are authored against a ~420px browser pane. The panel is
585×329, non-interactive, always-on. Options, in order of preference:

1. **Don't declare — show.** The picker previews every candidate at true
   size; the owner sees the truth before hanging it. Zero schema change,
   and it matches the repo's "measure, don't believe" temperament.
2. Pass context to the face: the injected `virtues.css` already varies by
   query param (`?theme=`); add `?surface=panel` and a corresponding class,
   so a face *may* adapt. Cheap, optional, additive.
3. A manifest fit declaration (`face.fits_panel` or similar) — defer.
   The manifest has a documented failure mode ("documented key with no
   loader field is discarded silently"), and a declared fit is a claim the
   preview already tests empirically.

Recommend 1 + 2 now, 3 never unless a real need appears.

## Open questions / audits before building

1. **Backlight audit — DONE 2026-08-26** on the bench box (Rosy Swallow),
   over UART, with eyes on the glass. Findings, in verdict order:
   - **True off exists.** Forcing the connector down
     (`echo off > /sys/class/drm/card1-HDMI-A-1/status`) drops the HDMI
     signal and the panel goes **fully dark** — backlight off, no glow, no
     NO SIGNAL banner. Verified by eye.
   - **No brightness, ever.** `/sys/class/backlight/` is empty (HDMI
     panel; nothing on a kernel PWM), and the panel does not speak DDC/CI.
     Hours is off/on, never a slider.
   - **Never probe DDC on this panel.** The `ddcutil detect` scan threw
     GENI i2c DMA errors and can leave the DDC line wedged; recovery took
     a physical power cycle. A brightness feature must not ship a probe.
   - **cage lacks `wlr-output-power-management-v1`**, so `wlopm` cannot
     sleep the output under the running kiosk. Not needed given the
     connector force, but it closes the compositor path without a patch.
   - **THE TRAP: re-detect cannot re-read EDID.** After any forced
     off → `detect` cycle, `msm_dp_bridge_get_modes` fails (rc=0,
     reproducibly, clean cycles included) and the connector falls back to
     VESA 1024×768/800×600 — the scaler then draws stretched. Only a full
     power cycle of panel+box recovered the real EDID. And the EDID's
     *content* is itself inconsistent: 1920×1080-preferred after a cold
     boot, 1024×600-preferred after one hotplug. This board lies twice.
   - **Therefore Hours requires the EDID firmware override.** Capture the
     panel's EDID once and pin it (`drm.edid_firmware=HDMI-A-1:edid/…` on
     the kernel cmdline, or the per-connector debugfs override) so no
     probe ever asks the wire again — deterministic modes on every wake,
     and it structurally retires the lying-EDID/zoom problem too. The
     bench panel's EDID is captured: 128 bytes, archived at
     `/home/radxa/panel-edid.bin` on the box
     (md5 `c20eb215b495a300a9738d06d9285a45`), base64:
     `AP///////wBI9BFSAQQAAAUXAQSlNR54AoBCrFEwtCUQUFMAAAABAQEBAQEBAQEBAQEBAQEBKDaAoHA4H0AwIDUAB0QhAAAaIi2AoHA4H0AwIDUAB0QhAAAaAAAA/gAKICAgICAgICAgICAgAAAA/gAxOTIweDEwODAKICAgADY=`
   - Sleep/wake recipe once the override is in place: sleep =
     `systemctl stop virtues-display` + force connector off; wake = force
     `detect` + `systemctl start virtues-display`. Both are verbs the box
     can run on a timer with the existing privilege model.
   - Bench residue: `wlopm` and `ddcutil` are now apt-installed on Rosy
     Swallow (harmless; do not run ddcutil against the panel again).

   **Hours design consequences** (settled 2026-08-26, before any build):

   1. **Sleep is a precedence state, not a cron toggle.** A sleeping
      screen during a storage fault, an update, or a held case button
      violates the duty-list contract — the button case is the sharp one:
      a hold against dark glass gets no countdown, the exact failure
      `button_held_secs` exists to prevent. So the SERVER owns the
      schedule and overrides it while any interruption state is active:
      sleep slots into the existing chain *below* every interruption,
      exactly like a face. The glass wakes to say what must be said, and
      goes back to sleep when it has said it.
   2. **The upgrade interaction.** Every upgrade ends in
      `restart_display()` (cli/upgrade.rs). If that fires mid-sleep
      against a forced-off connector, `ExecStartPre`'s connected-connector
      guard fails and the unit parks in `systemctl --failed`
      (StartLimitBurst). Either the wake path must `reset-failed` + force
      `detect` before `start`, or — cleaner — implement sleep so the unit
      keeps running (matte page) and only the connector toggles, leaving
      nothing for a restart to trip over. Decide at build time; do not
      discover in the field.
   3. **Never probe the wire again.** The DDC wedge is a standing
      constraint, not an incident: any future "brightness" ambition for
      this panel is answered — no, permanently. Off/on is the entire
      vocabulary.
   4. **Forced-connector-off is off-label, on purpose.** The by-the-book
      path is DPMS via the compositor, which means building cage with
      wlroots' output-power-management enabled — converting an
      apt-installed dependency into one we compile and ship, forever, for
      the same dark glass. The force is a DRM debug facility doing a
      power-management job; we take it and say so here. Revisit only if a
      kernel update breaks connector forcing.
   5. **Pin per panel model, not per box.** This panel served two
      different EDIDs in one afternoon, so "capture at firstboot" could
      capture the wrong one. The golden image pins the known-good
      1920×1080 blob (the one the shipped zoom was verified against) into
      `/lib/firmware/edid/` + kernel cmdline at install time.

## Hours — BUILT + HARDWARE-VERIFIED 2026-08-26

Wave `ab34e908` (code) + bench work on Rosy Swallow. What shipped:

- Migration 0005: `sleep_start`/`sleep_end time` on `app_display`,
  both-or-neither CHECK. `PUT /api/system/display/hours` ("HH:MM"
  box-local; both null = never sleeps); GET carries `hours` + `asleep`;
  the mirror's miniature shows an "Asleep — backlight off until HH:MM"
  card instead of mirroring blackness. UI = two time fields + "Never
  sleeps", verified live (set → persists → clear).
- The **sleep engine** (`api::system_display::sleep_engine`, spawned at
  server start, appliance-gated): a 1s lane reads the clock and the
  button atomic; a 30s lane (nudged by the PUT) reads claimed/updating/
  disk-fault and the config. Sleep = write `/run/virtues-display-asleep`
  + force the connector off; wake = remove, `detect`, and self-heal a
  failed unit (`reset-failed` + `start`). Sleep never engages unclaimed
  or over any interruption; a wake mid-window re-sleeps when the
  interruption clears. `window_active` unit-tested incl. the overnight
  wrap and the degenerate equal-times row.
- **Hardware-verified on the bench**: connector force-off under a RUNNING
  cage → glass fully dark, unit active, cage + shim alive. Unit restart
  mid-sleep with the marker escape → starts clean; **cage both survives
  losing its outputs and starts with zero outputs**. Full off→detect
  cycle under the pinned EDID → modes hold 1920×1080 (before the pin,
  every cycle degraded to VESA fallback); glass returns at the right
  proportions.
- **The bootloader finding**: on the Q6A, `/boot/extlinux/extlinux.conf`
  is decorative — the Qualcomm boot chain bakes bootargs into the device
  tree, and the documented `/etc/kernel/cmdline` + `u-boot-update` flow
  regenerates an append line nothing reads (live cmdline lacks even the
  `quiet splash` extlinux has carried all along). So the EDID pin ships
  as the **runtime module param** — the display unit's first
  ExecStartPre writes `/sys/module/drm/parameters/edid_firmware` when
  the image ships a blob — which is always in time, since Hours'
  re-detects are post-boot by definition. Both cmdline (inert, may start
  working if Radxa fixes the chain) and the unit line are in place on
  the bench; the installer template carries the unit line.
- **Not yet exercised end-to-end**: the engine loop itself on real glass
  (the bench box runs v0.1.4 stable, which predates all of this) — owed
  to the next staging cut that reaches a box with a screen. The
  mechanism it drives is verified move by move above.

## Audit 2026-08-26 (`39d10c98`) — findings fixed + known gaps

Self-audit plus an adversarial review over the whole display slice. Six
findings fixed (timezone: the appliance runs UTC and the engine evaluated
Hours in process-local time; mid-sleep server-restart orphaned the dark
connector — marker now carries the connector name and is adopted at spawn;
wake made retryable; the kiosk updating-latch race; dot_cloud's per-poll
GPU texture leak; Settings miniature token TTL + blank-fields-clears-hours).

**Gap resolutions (`675dca73`, 2026-08-27)** — three closed, one kept:

1. **Button hold** — CLOSED. `GET /api/display/button` (loopback family,
   two atomic loads), polled by the kiosk at 1s; a 3s hold now always
   draws its countdown. Verified: endpoint + 403 gate + 1s cadence in the
   page's network log.
2. **Face tokens dying with the server** — CLOSED. The kiosk bumps a face
   epoch on the unreachable→reachable transition, re-minting the token and
   reloading the hung face the moment the server returns.
3. **Miniature vs glass "Updating" divergence** — KEPT, by judgment:
   Settings is the surface updates are started from, and showing Updating
   during the download phase there is earlier honesty, not a bug.
4. **Deleted-applet orphan rows** — CLOSED. Migration 0008 deletes the two
   shipped ids (only our templates could have minted them); run history
   survives via ON DELETE SET NULL, and the calorie schema's logged meals
   are deliberately kept — a cleanup migration does not delete a record.
2. **Sleep enforcement locus**: the shim (Python, has the Wayland session)
   vs. the server (has the schedule). Likely: server computes `is_asleep`
   into `DisplayState`; the `/display` page renders matte; the shim handles
   true power if the audit says it can.
3. **Headless/DIY presentation**: `has_display` is false on every DIY box.
   Show the section with an honest empty state ("No screen is attached to
   this box") rather than hiding it — hidden sections read as broken nav,
   and the empty state is a natural place to eventually say "attach any
   HDMI screen" if the kiosk stack ever ships outside the appliance
   profile. Defer that last part; it drags cage/seatd installation with it.
4. **Zoom override UI**: `VIRTUES_DISPLAY_ZOOM` needs a privileged write to
   `virtues.env` + restart. Real but niche — advanced/CLI territory for
   now; the derived value already does the right thing since the 800×480
   bench-panel fix.
5. **Theme on the glass**: the panel is deliberately 24/7 dark with literal
   tokens ("the kiosk has no user to change theme"). Hanging themed faces
   doesn't change that doctrine — pass `?theme=dark` always, revisit never.

## Phasing

- **Phase 1 — sight.** Section + specs endpoint + live miniature + Restart
  verb + duty list. Read-only except one button. Ships value immediately
  (the restart remedy alone) and forces the Rust-sees-panel plumbing.
  **BUILT 2026-08-26** on wave.
- **Phase 2 — the face.** `app_display` migration (0004), config endpoints,
  the shelf, built-ins, applet-face hanging, true-size preview,
  `?surface=panel`. **BUILT 2026-08-26** on wave — built-ins shipped are
  The Record and Matte; Weather joined the phase-3 list (its feed's shape
  wants its own design pass, same as The Day/Clock). Biscuit hangs via the
  applet shelf, as any applet face does.
- **Phase 3 — hours + authoring door.** Sleep schedule (pending audit),
  "Ask for a new one" chat seed, Weather / The Day / The Clock /
  On This Day built-ins.
- **Later, maybe never:** orientation/transform, multi-display, DIY
  any-screen, photo frames from Drive.

## As built (2026-08-26)

- `virtues-core/src/api/system_display.rs` — `GET /api/system/display`
  (panel facts + redacted mirror), `PUT /api/system/display/face`,
  `POST /api/system/display/restart`. Registered beside the update routes.
- `app_display` singleton (migration 0004): `face_kind` builtin|applet,
  `face_builtin` record|matte, `face_applet_id`.
- `DisplayState` (loopback) gained `face`; kiosk read degrades to the
  record screen on any config-read failure — the glass always renders.
- `/display` ambient slot: matte and applet-face tenants; the kiosk mints
  its own face tokens (loopback = local-console) and re-mints at 45 min
  inside the 1 h TTL. Interruption precedence untouched.
- `virtues.js` faces runtime exposes `surface` (`?surface=panel` →
  `data-surface` on the root); nothing requires a face to use it.
- `DisplayView.svelte` + sidebar/mobile/SettingsView wiring. Verified
  against the dev stack: route dispatch, sidebar placement, error state,
  and the kiosk page tolerating a pre-face server.

## The face is a URL (2026-08-26, second pass)

The appliance kiosk is one pre-wired consumer of `/display`, not its owner.
The page now has two data doors, tried in order: the loopback state feed
(the box itself; carries the phrase), then the authenticated **redacted
mirror** (`/api/system/display`) for any *paired* browser — an old tablet
on a stand, a spare monitor, a TV. A browser that is neither gets an honest
"This screen isn't paired" sentence instead of a forever-bootmark. The
mirror mode carries the updating latch inline, never renders the phrase
("the setup words show on the box's own screen"), and hangs applet faces
through the normal authenticated token route. This is the DIY display
story; the cage/`display.py` stack stays internal-appliance-only, and a
`virtues display install` for DIY Linux boxes stays deliberately unbuilt.

Verified live on the dev stack (fresh server, migration 0004 applied):
face PUT validation (bad kind / bad builtin / faceless applet all 400),
choose-in-UI → loopback feed carries the face, picker lists all three
faced applets, off-box 403 → mirror fallback path (403 confirmed via
forwarded-header curl). The face chooser is a SELECT, not a row list —
applet faces grow unbounded and a select holds fifty as calmly as five
(same argument as the update channel picker); an orphaned choice (applet
deleted after hanging) keeps a "(missing)" option so the select never
silently lies. Still owed to a hardware/second-device run:
claimed-ambient rendering of matte + applet iframe on real glass, and a
true remote browser wearing the mirror.
