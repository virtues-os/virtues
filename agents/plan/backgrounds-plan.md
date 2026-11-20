# Backgrounds plan

The picture behind home becomes a setting. You pick a style, a collection in
it, or one picture, and the app shows it in Light and Dark. Today there is one
picture, the lake, and the code names it in four places.

This plan replaces decision 8 of
[visual-direction-plan.md](visual-direction-plan.md) ("a bundled set of about
eight"). The ink print in page headers stays in that plan.

## What exists today

Checked against the code on 2026-11-20.

- **One picture.** `static/scenes/lake.{avif,webp,jpg}`. On a computer it is
  the window's picture (`routes/(app)/+layout.svelte`, `.window-scene`), and
  home draws no paper over it. On a phone, home draws it itself
  (`ChatView.svelte`, `.home-scene`). Onboarding and the Materials specimen
  each name it again.
- **Dark is the same picture dimmed** through `--window-scene-filter` in
  `themes.css`. `--window-scene-mask` fades its top edge.
- **A setting stored on the server already has a home.** `ui_preferences` on
  the assistant profile, written through `updateUiPreferences()` in
  `lib/api/client.ts`. Glass is stored per browser (`stores/glass.svelte.ts`),
  which is the wrong place for this, because the phone and the computer
  should show the same picture.
- **The paintings exist outside the repo.** Two sets, both 1792×768, made in
  Figma in November 2026, each with a `prompts.txt`:
  - The muted set: 8 collections (beach, countryside, desert, forest,
    gardens, lakes and rivers, mountains, US national parks) × 12 pictures.
    The in-app WebP files have the bottom faded to transparent.
  - The original set: 13 collections (the 8 above plus arctic, skies, space,
    travel, tropics) × about 12 pictures, as JPEG.
  - Each set has four folders: `new-chat-light`, `new-chat-dark`,
    `day-page-light`, `day-page-dark`. **The light and dark folders hold
    different places**, so no picture has a dark version yet.
- **The day page has its own picture**, unrelated to this plan: "Paint this
  day" makes a painting of a place the page names (`api/day_picture.rs`,
  `DayFigure.svelte`).

## The model

A picture has an address with three parts, and the app picks the file from
two more:

```
picture = style › collection › name      what you pick
file    = picture × theme                what the app draws
```

- **Style** is a renderer and an art direction: image, video loop, shader, or
  drawn on a canvas (dots, ASCII). Every collection in a style uses its
  renderer.
- **Collection** is a curated set inside a style: Forest, Space, and later
  Deep-sea animals or Slow noise.
- **Picture** is one entry in the style's manifest. For an image style it is
  a pair of files. For a shader it is a set of parameters.
- **Theme** (Light or Dark) is never something you pick in the background
  picker. It follows the app's appearance. A picture must have both, or the
  renderer must derive one, the way the lake is dimmed today.

The setting is one path, and it can stop at any depth:

| Setting | What you see |
|---|---|
| `plain` | No picture. The desk only. |
| `paintings` | A picture from any collection in the style, changing daily |
| `paintings/forest` | A picture from Forest, changing daily |
| `paintings/forest/birch-grove` | That picture, always |

When the setting points at something that is not installed or no longer
exists, the app falls back to the lake.

**Daily rotation.** A rotating setting changes picture at local midnight and
keeps it for the day. The order is shuffled once per pool and does not repeat
until every picture has shown. A picture that changes on every new chat would
feel like a slot machine; one that belongs to the day matches how the rest of
the app thinks about time. (Proposed, not confirmed.)

**Keep this one.** On home, an action turns the day's rotating picture into a
fixed setting.

### Decisions made 2026-11-20

- **Both painting sets ship, as two styles.** The muted set and the original
  set. Their names in the picker are open.
- **Light and dark are paired now, night versions later.** Each collection's
  12 light pictures are matched with its 12 dark ones by curation. Later, each
  light painting gets a night version of the same place.
- **Only home, for now.** The owner: "rm picture from day page for now. lets
  focus on the new chat / homepage bgs". The `day-page-*` folders are not
  used. The `new-chat-*` folders are the home pictures, which on a computer is
  also the window's picture.
- **The levels are called Style › Collection.** "Theme" keeps meaning Light
  and Dark.

## Assets

The paintings are about 400 MB as made and 108 MB as the app uses them, so
they stay out of git.

A tool in `tools/` reads a source folder laid out as today's sets are
(`<set>/new-chat-<theme>/<collection>/<name>.<ext>`) and writes:

- `manifest.json` for the style: for each picture its id, title, collection,
  and for each theme the file, its average luminance, and a focal box. The
  luminance lets glass and the header text adjust over a bright picture. The
  focal box keeps the subject visible where the window crops differently from
  the phone.
- The pairs. The tool proposes a light and dark pair inside each collection
  from color and the prompt's words, writes them to a pairs file, and the
  owner corrects it. The pairs file is the source the manifest reads.
- AVIF and WebP files at two widths, with the JPEG or PNG only as source.

## Delivery

The lake stays bundled in `static/scenes/` as the default and the fallback.
Every other style is a versioned archive that the box downloads once when you
turn the style on, installs into its state root (`/var/lib/virtues/`, beside
the applet state), and serves to its own apps. The apps never fetch a picture
from anywhere but the box.

The download carries no personal data, but it is a request that leaves the
server, so it gets a line in the manual and in
[privacy-boundary.md](../build/privacy-boundary.md) in the slice that adds it.

Where the archives are hosted is open.

## Moving styles

Shaders, loops and drawn styles have three costs an image does not:

- Glass blurs whatever is under it. A `backdrop-filter` over a picture that
  changes every frame redraws the blur every frame, and WKWebView pays the
  most.
- Under `prefers-reduced-motion`, a moving style shows one still frame.
- A moving style pauses when the window is hidden and drops its frame rate
  when nothing on screen is changing.

## What each persona gets

- **Narrative seeker:** a painting that belongs to the day, behind the place
  where you ask about your life.
- **Quantified self:** Plain at the top of the picker, so nothing sits behind
  a chart if you don't want it to.
- **Tech prosumer:** styles to choose from, moving ones later, and night
  versions of the same place when you switch to Dark.
- **Recovery:** nothing to unlock, earn or collect. No "new" marks on styles,
  no count of pictures seen.
- **Data sovereign:** styles install on the box and the apps fetch only from
  it. The manual says what the box downloads and from where.

## Order of work

Each slice replaces the old way in the same change.

1. **The asset tool and the pairs.** Manifest, pairs file, encoded files, for
   both painting styles. The owner reviews the pairs.
2. **One component and the setting.** A `Background` component and a
   resolver (setting → picture → file for the theme, with the daily rotation)
   replace the four places that name the lake. The setting is stored in
   `ui_preferences`. Until slice 4, styles are read from a local folder in a
   dev checkout.
3. **The picker.** Settings › Appearance: styles, then collections, then
   pictures, with Plain first. Keep this one on home.
4. **Delivery.** The box downloads, installs and serves a style. Manual page
   and the privacy boundary entry.
5. **A second renderer.** One cheap moving style (dots or a single shader) to
   check the renderer split before more image styles are added.
6. **Night versions** of the paintings, replacing the curated pairs.

## Open decisions

1. The names of the two painting styles in the picker.
2. Daily rotation, as proposed above, or a different change point.
3. Where style archives are hosted.

## When this plan is done

Delete it when slice 4 ships. What stays is a manual page on backgrounds, the
privacy boundary entry, and a paragraph under Materials in
[design.md](../build/design.md) replacing the one that names the lake.
