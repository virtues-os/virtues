# Establishing screen — design specimen

A runnable draft of a screen the airlock does not have yet: **one line of text,
one button, and animated light**, shown before `connect.html` asks its first
question. Today the first
thing a new owner sees is *"Where is your server in its life?"* — asked with
no introduction, right after the owner has unboxed the device.

This is a design specimen for discussion; it does not change the app. Nothing
in `apps/web/src-tauri/ui/` was touched.

```sh
python3 -m http.server 8899          # from the REPO ROOT, not from here
open http://127.0.0.1:8899/agents/plan/specimens/establishing-screen/establishing.html
```

Serving from the repo root matters: the page loads EB Garamond from
`apps/web/static/fonts/`, so that the repo carries one copy of a licensed face
rather than two.

## The three screens

1. **The establishing screen** — the opening line, the `Begin` button, a dawn
   light, and a gobo (a projected shadow pattern).
2. **Bluetooth, explained in our own words** before macOS shows its own
   permission prompt. This
   screen does not exist today; the system sheet is the first mention of
   Bluetooth anywhere in setup.
3. **The entry fork exactly as it ships**, so the transition is visible and
   you can judge whether screen 1 leads into it cleanly.

## Design decisions

**The button also starts the audio.** WebKit will not start audio without a
user gesture, so `Begin` is what lets the score start *with* the flow rather
than over whatever the owner was already listening to.

**The score sits at `0.20`** and rises over 2.2s. `volume` is linear amplitude
while hearing is roughly logarithmic, so that is about −4 dB under the first
setting: quieter, but recognisably the same music. Mute is in
the corner and is remembered.

**The wind follows a physical model instead of an easing curve.** An early version rested at almost nothing
and spiked to nine times that, so the page sat dead still and then lurched.
Real wind behaves differently: it always blows, and a gust is a *modest* multiple
of the mean — the gust factor (peak ÷ mean) sits near 1.3–1.7 outside severe
weather, gusts last under twenty seconds before a lull, and met agencies
standardised on a three-second averaging window, which is why the rise is ~2s
and the decay ~6s. Foliage is then driven as a hierarchy — trunk sway slowest
and widest, branches faster and narrower — because that is how foliage is
rigged for film and games. Leaf flutter is left out: applied to a whole
photograph it looks like camera shake.

**One plate, so one light source.** Two photographed shadows stacked always
look like two light sources — different angle, different penumbra, different
wall — and no opacity setting hides that. The `?w=` layer (a static window, never
animated, because buildings do not sway) stays wired for when a frame and
its foliage are cut out of the *same* exposure. Until then the default is a
single plate and everything in it may move.

**The screen must work with no plate at all.** `?g=none` is more than a debug
flag: it shows what a cold first launch looks like when the asset fails to
load. The airlock is compiled into the binary so that it draws when nothing
else does. The
plate loads *over* the type, never under it.

## Switches

| | |
|---|---|
| `?l=NN` | the moving plate (foliage) |
| `?w=NN` | the static plate (architecture); `none` by default |
| `?g=NN` | one plate carrying both, for comparison |
| `?g=none` | no plate — the state that must still work |
| `P` | a drawn stand-in horizon, in and out |

## What is NOT in git

**`plates/` and `score.mp3` are gitignored for licensing reasons.** Both are commercially licensed for use *in* a production;
this repo is public, and committing them would be redistribution. The plates
live in iCloud under `gobos/` (five packs, 64 files); nothing in `tools/`
fetches them, by design. Before either ships inside the app, the license needs
reading for **software embedding and App Store distribution**, which standard
production licenses commonly carve out — the music certificate names a
"production" and its files, and says the purchase terms govern.

A clone without them shows screen 1 with text and lighting only, which is the
fallback state anyway.

## Open questions

- **The opening line.** *"The most intimate record of your life, finally yours."*
  It rests on a true claim: that data already exists, on other people's
  servers, and has never belonged to the owner. The word "finally" only works
  for something that was withheld, which is why "technology" was tried and
  dropped.
- **The visual style.** Screen 1 is a lit room; screens 2–3 use plain paper.
  The difference is intentional, but it departs from the airlock's white-paper
  style, so it needs an explicit decision.
- **EB Garamond in the airlock.** `connect.html` sets `ui-serif` with a comment
  explaining that EB Garamond lives in the SPA bundle, which does not exist when
  the airlock draws. That is accurate, and it is what to fix: the woff2 belongs in the
  binary beside `connect.html`, served through the `virtues://` handler. Right
  now every screen in setup is in a different typeface from the app it opens
  into.
- **Performance cost.** The plate is animated every frame; unmeasured on the kiosk and on
  older Macs.
