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
    The in-app WebP files have the top faded to transparent.
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

### The home control

From the owner, 2026-11-20: in the bottom right corner of a new tab or new
chat, a small glass button with an icon that suggests appearance. Clicking it
opens a popover with:

- **Left and right arrows** that step to the previous and next picture.
- **Appearance:** Light, Dark, System, and Follow the day.
- **A small collection picker** that swipes sideways through the collections
  of the current style.

Settings › Appearance keeps the full picker (styles, collections, pictures,
Plain). The popover is the quick way in from where the picture is.

Built in slice 3 as proposed. The owner has not confirmed the first and
third points:

- Under a rotating setting, the arrows change the picture for the rest of
  today (`ui_preferences.background_today`), and rotation resumes the next
  day. Under one picture they change the setting to its neighbor in the
  collection. "Keep this picture" is in the same popover.
- Choosing a collection in the swiper sets the setting to that collection,
  rotating.
- **Follow the day** is a fourth theme choice, `day`, beside `oxford`,
  `asgard` and `system`. It is Light from sunrise to sunset and Dark
  otherwise. Sun times come from today's forecast on the server
  (`GET /api/weather/current`, made for its last known location), cached as
  `virtues-sun` so `app.html` paints the right theme on the first frame, and
  fall back to 07:00 and 19:00.
- The button is the same glass as the header's corner buttons (`.lq`), with
  the Myna image icon. On the phone it stands above the docked composer.

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

The paintings are about 400 MB as made and 200 MB as the app uses them, so
they stay out of git.

`tools/backgrounds/build.py` (built 2026-11-20) reads a source folder laid out
as today's sets are (`<set>/new-chat-<theme>/<collection>/<name>.<ext>`) and
the set's `prompts.txt`, and writes into an output folder:

- `manifest.json` for the style: each collection with its title from the
  prompts file, and each picture with an id (its light name) and, for each
  theme, the source name, title, file paths, average luminance, a focus box
  and which edge fades to transparent. The luminance lets glass and the
  header text adjust over a bright picture. The focus box is the middle of
  the picture's edge energy, a rough guide for cropping on a phone.
- AVIF and WebP at 1792 and 896 wide. A rerun only re-encodes a picture whose
  source is newer.
- `review.html`, each picture's light version over its dark one.

The pairs live in `tools/backgrounds/pairs/<style>.tsv`, tracked. The tool
proposes pairs for a collection that has none, from the words of each
picture's title, file name and prompt (TF-IDF and an optimal matching), and
writes them there. After that the file wins, and the tool refuses a file that
does not use every picture exactly once. The proposals are rough: pictures
marked "(earlier)" in the prompts file have no prompt, and words cannot see a
composition.

The two styles are built in `~/virtues-backgrounds/paintings-muted` and
`~/virtues-backgrounds/paintings-original`. The ids are working names until
the picker names are chosen; they become part of a stored setting, so they
should settle before slice 2 ships.

What the measurement showed:

- The muted set fades to transparent at the **top** in both themes, so it
  needs no extra mask under the header. The original set is painted edge to
  edge and still needs `--window-scene-mask`.
- Light pictures are far brighter than dark ones in both sets, as expected,
  and the original light set is brighter than the muted one, which matters
  for glass density over it.

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

1. **The asset tool and the pairs.** Built 2026-11-20 for both painting
   styles. The owner accepted the proposed pairs ("good enough").
2. **One component and the setting.** Built 2026-11-20:
   - The server lists installed styles at `GET /api/backgrounds` and serves
     their pictures at `GET /api/backgrounds/files/<style>/…`
     (`api/backgrounds.rs`, routes in `server/api/settings.rs`). Styles live
     in `/var/lib/virtues/backgrounds` on a server and `.background-state/`
     in a dev checkout, which can be a symlink to the build tool's output.
   - `lib/backgrounds/resolve.ts` turns the setting into today's picture;
     `lib/stores/background.svelte.ts` holds the setting
     (`ui_preferences.background`, cached in localStorage with the day's
     picture so the first frame does not flash the lake);
     `components/Background.svelte` draws it and replaces the four places
     that named the lake. Only the lake is dimmed in Dark now; a style's
     picture has its own dark version.
   - The default stays the lake until a picker exists to change it.
3. **The pickers.** Built 2026-11-20:
   - `components/backgrounds/BackgroundControl.svelte` on home: the arrows,
     the four theme choices and the collection swiper.
   - `components/backgrounds/BackgroundPicker.svelte` under Theme in
     Settings › Appearance: Plain, the lake and each style, then the
     collections of the chosen style, then its pictures, with "Changes
     daily" first in each row.
   - Follow the day in `lib/utils/theme.ts`, `app.html`, the theme cards and
     the settings agent's list of themes.
   - Each style's manifest carries its name in the pickers (`title`, from
     `build.py --title`). The names are working names: "Muted paintings"
     and "Paintings".
4. **Delivery.** The box downloads, installs and serves a style. Manual page
   and the privacy boundary entry.
5. **A second renderer.** One cheap moving style (dots or a single shader) to
   check the renderer split before more image styles are added.
6. **Night versions** of the paintings, replacing the curated pairs.

## Open decisions

1. The names of the two painting styles in the picker. Working names are
   "Muted paintings" and "Paintings", and the ids `paintings-muted` and
   `paintings-original` are what a saved setting stores.
2. Daily rotation, as proposed above, or a different change point.
3. Where style archives are hosted.
4. What the home control's arrows do under a rotating setting, and how
   Follow the day finds sunrise and sunset (both built as proposed above).

## When this plan is done

Delete it when slice 4 ships. What stays is a manual page on backgrounds, the
privacy boundary entry, and a paragraph under Materials in
[design.md](../build/design.md) replacing the one that names the lake.
