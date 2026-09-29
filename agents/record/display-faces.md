# Settings › Display — the box's face

Written 2026-09-29 from `agents/plan/display-plan.md`, whose built half this
replaces. Built 2026-08-26 to 08-27 (`ab34e908`, audit `39d10c98`, gaps
`675dca73`). The hardware behaviour it rests on (true off by forcing the
connector, no brightness, never probe DDC, the pinned EDID, the Q6A
bootloader) is in [display-hardware.md](display-hardware.md). Open work:
[display-plan.md](../plan/display-plan.md).

## What exists

- **Settings › Display** (`tabs/views/DisplayView.svelte`, sidebar id
  `display`): a live miniature of the glass, the panel's facts (connector,
  mode, service state, zoom; never the inches the panel claims), **Restart
  the screen**, the face picker, Hours, and the duty list. A box with no
  screen shows "No screen is attached to this server" rather than hiding the
  section: a hidden section reads as broken navigation.
- **Endpoints** (`virtues-core/src/api/system_display.rs`, authenticated, none
  in the loopback `api/display.rs`): `GET /api/system/display`,
  `PUT /api/system/display/face`, `PUT /api/system/display/hours`,
  `POST /api/system/display/restart`.
- **Storage:** the `app_display` singleton (face kind, built-in, applet id,
  sleep start/end). `ui_preferences` was rejected: it belongs to a device's
  session, and the kiosk has none; the face is a fact about the box.

## The face hangs inside `/display`, not instead of it

`VIRTUES_DISPLAY_URL` is never pointed at a face. `/display` owns the state
machine (button held, updating, storage fault, setup), and that precedence
survives any choice the owner makes. A chosen face is a tenant of the
claimed-ambient slot: built-ins are components inside `/display`; an applet
face is an iframe, and the kiosk, being loopback and so `local-console`, mints
its own face token, re-minting at 45 minutes inside the one-hour TTL and again
when the server comes back after being unreachable. A failed config read
renders The Record: the glass always renders.

This is also the security answer to "no caller-supplied content on the glass":
the authority is the owner's authenticated session choosing the face, and the
face itself stays in its jail (opaque origin, CSP, the read-only
`virtues_face_reader` role, row and time caps).

**The duty list** on the Settings page is that precedence, disclosed and not
configurable: updating, storage fault, button held, setup always interrupt
whatever the face is. It also answers "why did my face disappear during the
upgrade".

## The face is a URL

The appliance kiosk is one consumer of `/display`, not its owner. The page
tries two data doors in order: the loopback state feed (the box itself, which
carries the setup phrase), then the authenticated redacted mirror
(`GET /api/system/display`) for any paired browser (a tablet on a stand, a
spare monitor). A browser that is neither gets "This screen isn't paired". The
mirror never renders the phrase. This is the DIY display story; the
cage/`display.py` stack stays appliance-only, and a `virtues display install`
for DIY boxes is deliberately unbuilt.

## Fit

Faces are written for a ~420px pane; the panel is 585×329, non-interactive and
always on. The answer is to show, not declare: the picker previews each face at
true size, and `virtues.js` exposes `?surface=panel` as `data-surface` so a
face *may* adapt. A manifest fit key was rejected: the manifest silently
discards documented keys with no loader field, and a declared fit is a claim
the preview already tests. The glass passes `?theme=dark` always.

## Hours

Two times, box-local, or "Never sleeps"; the miniature shows "Asleep —
backlight off until HH:MM" rather than mirroring black. Off and on is the
whole vocabulary: this panel has no backlight control and must never be
probed over DDC, so there is no brightness slider, and a software-dimming
slider over a burning backlight would be a control that lies.

The sleep engine (`system_display::sleep_engine`, appliance only) is a
precedence state, not a cron toggle: it never sleeps an unclaimed box and
wakes for a button, an upgrade or a storage fault, then sleeps again when the
interruption clears. The kiosk unit keeps running; only the connector
toggles, and the `/run/virtues-display-asleep` marker lets a mid-sleep
`restart_display()` start the unit without a connected connector instead of
parking it in `--failed`.

The 2026-08-26 audit's six fixes, as failure classes: the appliance runs UTC
and Hours was evaluated in process-local time; a server restart mid-sleep
orphaned a dark connector (the marker now carries the connector name and is
adopted at start); wake was not retryable; a kiosk updating-latch race; a
per-poll GPU texture leak in `dot_cloud`'s face; a miniature token outliving
its TTL. The kiosk polls `GET /api/display/button` every second so a
three-second hold always draws its countdown. The Settings miniature shows
Updating during the download phase before the glass does, kept on purpose:
Settings is where updates start.
