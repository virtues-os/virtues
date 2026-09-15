# The establishing screen — a specimen

A runnable draft of a beat the airlock does not have: **one line, one control,
and light**, before `connect.html` asks its first question. Today the first
thing a new owner sees is *"Where is your server in its life?"* — a question
asked cold, of someone who has just unboxed an object.

This is a specimen, not a patch. Nothing in `apps/web/src-tauri/ui/` was
touched. It exists to be argued with.

```sh
python3 -m http.server 8899          # from the REPO ROOT, not from here
open http://127.0.0.1:8899/agents/plan/specimens/establishing-screen/establishing.html
```

Serving from the repo root matters: the page loads JJannon from
`apps/web/static/fonts/`, so that the repo carries one copy of a licensed face
rather than two.

## The three beats

1. **The establishing screen** — the promise, the control, a dawn, and a gobo.
2. **Bluetooth, explained in our own words** before macOS asks in its. This
   screen does not exist today; the system sheet is the first mention of
   Bluetooth anywhere in setup.
3. **The entry fork exactly as it ships**, so the seam is visible and beat 1
   can be judged on whether it hands off cleanly.

## What is deliberate

**The button is load-bearing twice.** WebKit will not start audio without a
user gesture, so `Begin` is what lets the score start *with* the flow rather
than over whatever the owner was already listening to.

**The score sits at `0.20`** and rises over 2.2s. `volume` is linear amplitude
while hearing is roughly logarithmic, so that is about −4 dB under the first
setting: a step back into the room, not a different piece of music. Mute is in
the corner and is remembered.

**The wind is modelled, not eased.** An early version rested at almost nothing
and spiked to nine times that, so the page sat dead still and then lurched.
Real wind does the opposite: it always blows, and a gust is a *modest* multiple
of the mean — the gust factor (peak ÷ mean) sits near 1.3–1.7 outside severe
weather, gusts last under twenty seconds before a lull, and met agencies
standardised on a three-second averaging window, which is why the rise is ~2s
and the decay ~6s. Foliage is then driven as a hierarchy — trunk sway slowest
and widest, branches faster and narrower — because that is how foliage is
rigged for film and games. Leaf flutter is deliberately absent: applied to a
whole photograph it is not flutter, it is camera shake.

**One plate, one sun.** Two photographed shadows stacked always read as two
light sources — different angle, different penumbra, different wall — and no
opacity setting fixes a contradiction. The `?w=` layer (a static window, never
animated, because buildings do not sway) stays wired for the day a frame and
its foliage are cut out of the *same* exposure. Until then the default is a
single plate and everything in it may move.

**The screen must work with no plate at all.** `?g=none` is not a debug flag;
it is what a cold first launch looks like when the asset fails, and the airlock
is compiled into the binary precisely so it draws when nothing else does. The
plate loads *over* the type, never under it.

## Switches

| | |
|---|---|
| `?l=NN` | the moving plate (foliage) |
| `?w=NN` | the static plate (architecture); `none` by default |
| `?g=NN` | one plate carrying both, for comparison |
| `?g=none` | no plate — the state that has to hold |
| `P` | a drawn stand-in horizon, in and out |

## What is NOT in git

**`plates/` and `score.mp3` are gitignored, and that is a licensing decision,
not housekeeping.** Both are commercially licensed for use *in* a production;
this repo is public, and committing them would be redistribution. The plates
live in iCloud under `gobos/` (five packs, 64 files); `tools/` has nothing that
fetches them, on purpose. Before either ships inside the app, the license needs
reading for **software embedding and App Store distribution**, which standard
production licenses commonly carve out — the music certificate names a
"production" and its files, and says the purchase terms govern.

Clone this without them and you get beat 1 in type and light alone, which is
the fallback state anyway.

## Open

- **The line.** *"The most intimate record of your life, finally yours."* Rests
  on a claim that is true and unarguable: that record already exists, on other
  people's servers, and has never been yours. "Finally" is load-bearing — it
  only works for something that was withheld, which is why "technology" was
  tried and dropped.
- **The register.** Beat 1 is a room; beats 2–3 are paper. Deliberate, but it
  is a departure from the airlock's white-paper grammar and deserves a decision
  rather than a drift.
- **JJannon in the airlock.** `connect.html` sets `ui-serif` with a comment
  explaining that JJannon lives in the SPA bundle, which does not exist when
  the airlock draws. True, and the thing to fix: the woff2 belongs in the
  binary beside `connect.html`, served through the `virtues://` handler. Right
  now every screen in setup is in a different typeface from the app it opens
  into.
- **Cost.** The plate is animated every frame; unmeasured on the kiosk and on
  older Macs.
