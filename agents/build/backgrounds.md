# Backgrounds

The picture behind home is a setting, and every kind of background (a
painting, dots, later a shader, a video loop or a photograph) goes through
the same path. Read this before you add a style, a renderer or a new way to
choose one. The manual's page is [docs/use/backgrounds.md](../../docs/use/backgrounds.md);
what leaves the server is in [privacy-boundary.md](privacy-boundary.md).

## The model

```
setting  = '' | plain | style | style/collection | style/collection/picture
file     = picture × appearance (Light or Dark)
```

- **Style**: one renderer and one art direction. Its manifest names the
  renderer (`image`, `dots`) and lists its collections.
- **Collection**: a curated set inside a style.
- **Picture**: one entry in a collection. For an image style it is a file
  set per appearance; for a drawn style it is parameters.
- **Appearance** is never part of the setting. Every picture has a Light and
  a Dark version, or its renderer derives one: an image pack with one
  version marks its dark version `dim`, and a drawn style draws in the
  theme's ink.

The setting lives in `ui_preferences.background`, so every device shows the
same picture. `''` shows the lake (`static/scenes/`), which is not a choice in
the pickers: it is what home shows until one is made, and the fallback when
a chosen style is missing. A setting that stops above one picture rotates
daily (`pickForDay` in `lib/backgrounds/resolve.ts`): it changes at local
midnight, walks a seeded shuffle per cycle, shows every picture once per
cycle, and never shows one two days running. The arrows choose another
picture for the rest of the day (`ui_preferences.background_today`).

Ids are `[a-z0-9-]{1,64}`, because they are part of a stored setting.
Renaming a style or a collection after release breaks every setting that
names it.

## Where a style comes from

| Kind | Where it lives | Example |
|---|---|---|
| Built in | `lib/backgrounds/builtin.ts`, shipped with the app | Dots |
| Downloaded | the server's `/var/lib/virtues/backgrounds/<style>/`, fetched once from the catalog when picked | Watercolors, Oil paintings, Photographs |

The store (`lib/stores/background.svelte.ts`) lists built-in styles first,
then the server's (`allStyles`). A built-in style resolves at once; a
downloaded one needs `GET /api/backgrounds`, and until it answers home shows
today's cached picture.

**A drawn style is built in. A style of files is downloaded.** Files never
ship in the app bundle beyond the lake: every image style would grow every
update for everyone.

## Renderers

A style's manifest names its `renderer`. `image` is drawn by
`components/Background.svelte` itself, because it is what most people see on
the first frame. Every other renderer draws a style the app ships
(`DrawnStyle`, whose pictures are `DrawnPicture` parameters) and is
registered in three places, kept apart so nothing extra loads at launch:

| Where | What |
|---|---|
| `lib/backgrounds/resolve.ts` | its id in `DrawnRenderer`, and a type for its parameters (`DotsParams`) |
| `lib/backgrounds/renderers.ts` | the loader for its component; Background.svelte loads it the first time a setting needs it |
| `lib/backgrounds/controls.ts` | its thumbnail in `drawnThumbnail`, which loads with the pickers |

A renderer must:

- **Draw both appearances.** Follow `appearance` from the store; a theme
  change must redraw at once.
- **Fill its parent.** The parent (`.window-scene` on a computer,
  `.home-scene` on the phone) owns the size, the top mask and the fade.
- **Cost little while it moves.** The window's glass blurs whatever is
  under it, and a frame that changes makes the blur redraw. Draw at most
  20 frames a second, nothing while `document.hidden`, and one still frame
  under `motion.reduced` (`stores/motion.svelte.ts`). `DotsField.svelte` is
  the reference.
- **Draw a thumbnail** for the pickers, in the current appearance. A drawn
  style draws a still of the same function (`dotsPreview`).

## Adding a style

### An image pack (paintings, photographs)

1. Lay the source out as `<source>/new-chat-light/<collection>/<name>.<jpg|png|webp>`,
   and `new-chat-dark/` beside it when each picture has a night version. With
   no dark folder, each picture's dark version is the light one dimmed.
2. Build it: `python3 tools/backgrounds/build.py --style <id> --title "<Name>" --source <dir> --out ~/virtues-backgrounds/<id>`,
   with `--prompts` when there is a prompts file for titles. With a dark
   folder whose pictures are different places, the tool proposes pairs in
   `tools/backgrounds/pairs/<id>.tsv`; check them in `review.html` and fix
   the file. Pictures are AVIF at 1792 and 896 wide.
3. Package every style: `python3 tools/backgrounds/package.py --styles <each style dir> --out ~/virtues-backgrounds-dist`.
4. Upload the new archives to the bucket under `backgrounds/`, then
   `catalog.json` last, with the maps writer key on the cloud server
   ([deployment.md](deployment.md)). A server with the style already
   updates to a rebuilt archive on its own.

### A drawn style (dots, a shader, ASCII)

1. Write the renderer: a component that takes `params` and `appearance`
   (`DrawnProps`), and a function that draws a still for a thumbnail.
2. Register it in the three places under Renderers.
3. Add its manifest to `lib/backgrounds/builtin.ts`: collections of
   pictures, each a set of parameters.
4. Check it under Reduce motion, in a hidden window, in both appearances,
   on the phone, and against the bundle budget ([performance.md](performance.md)).

### A video loop

Not built. A loop is files, so it is a downloaded style with its own
renderer. The server's file route (`GET /api/backgrounds/files/...`) reads a
whole file into memory and has no `Range`, which Safari needs for video, so
it needs streaming with ranges first, and a still frame under Reduce motion.

## Rules every style keeps

- **Nothing to unlock, count or collect.** No "new" marks, no count of
  pictures seen, no streaks; someone in recovery sees home every day
  ([personas.md](personas.md)).
- **No mark of where a style comes from in the pickers.** A card may say
  its size and its download progress; where it comes from is in the
  manual.
- **Plain stays first.** Someone who wants nothing behind a chart can have
  nothing.
- **The apps load pictures from the server only.** A catalog preview must be
  a `data:image/` URL; the server drops anything else.
