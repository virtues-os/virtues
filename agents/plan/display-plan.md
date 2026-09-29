# Settings › Display — what is left

Settings › Display, the face precedence, the applet-face shelf, the mirror for
paired browsers, and Hours with its sleep engine are built; how and why is in
[display-faces.md](../record/display-faces.md), and the panel's measured
behaviour in [display-hardware.md](../record/display-hardware.md). Delete this
plan when the list below is empty.

## Open

- **More built-in faces.** `BUILTIN_FACES` (`virtues-core/src/api/system_display.rs`)
  is still `["record", "matte"]`. Each is an existing feed wearing clothes, and
  each wants its own design pass for a 585×329, non-interactive, always-dark
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
  writes faces (`tools/applet_setup.rs`, `face_html`); this is a prompt and a
  door. "Ask for a chart of your resting heart rate and hang it on the box" is
  the sentence that sells the feature.
- **Real glass, end to end.** Owed to the first box with a screen on a build
  that has all of this: the sleep engine's loop through a real night (the
  mechanism was verified move by move on the bench, the loop never ran there),
  matte and an applet face in the claimed-ambient slot, and a second paired
  browser wearing the mirror.

## Not doing

- Brightness, ever, on this panel (no backlight control; DDC must not be
  probed).
- A zoom override in Settings: `VIRTUES_DISPLAY_ZOOM` needs a privileged write
  and a restart, and the derived zoom is right. CLI territory.
- Orientation, multiple displays, a DIY any-screen installer, photo frames
  from Drive: later, maybe never.
