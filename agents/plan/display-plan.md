# Settings › Display — what is left

Settings › Display, the face precedence, the applet-face shelf, the mirror for
paired browsers, and Hours with its sleep engine are built; how and why is in
[display-faces.md](../record/display-faces.md), and the panel's measured
behaviour in [display-hardware.md](../record/display-hardware.md). Delete this
plan when the list below is empty.

## Open

- **More built-in faces.** `BUILTIN_FACES` (`virtues-core/src/api/system_display.rs`)
  is still `["record", "matte"]`. Each is a new view of an existing feed, and
  each needs its own design pass for a 585×329, non-interactive, always-dark
  panel (one idea per screen, no spinners, no animation loops):

  | Face | Feed |
  |---|---|
  | The Day | the day summary: yesterday's opening line, each morning |
  | On This Day | `/api/wiki/on-this-day` |
  | The Clock | `/api/wiki/lifeline/clock` |
  | Weather | `/api/weather/current` |

  Add each to `BUILTIN_FACES`, the `face_builtin` values the PUT accepts, the
  `/display` ambient slot, and the picker.
- **"Ask for a new one."** The last item in the face picker opens a chat
  seeded with the panel contract (585×329, non-interactive, dark, no animation
  loops, `virtues.query` for data, `?surface=panel`). Chat authoring already
  writes faces (`tools/applet_setup.rs`, `face_html`); this item only needs a
  prompt and an entry point in the picker. The request it is meant for looks
  like "Ask for a chart of your resting heart rate and hang it on the box".
- **Test on real hardware, end to end.** This waits for the first box with a
  screen on a build that has all of this: the sleep engine's loop through a real night (the
  mechanism was verified move by move on the bench, the loop never ran there),
  matte and an applet face in the claimed-ambient slot, and a second paired
  browser showing the mirror.

## Not doing

- Brightness control on this panel, at any point (it has no backlight
  control, and DDC must not be probed).
- A zoom override in Settings: `VIRTUES_DISPLAY_ZOOM` needs a privileged write
  and a restart, and the derived zoom is right. It stays a CLI setting.
- Orientation, multiple displays, a DIY any-screen installer, photo frames
  from Drive: later, maybe never.
