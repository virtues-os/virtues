# Offline maps: Protomaps on the box

**Status: planned, spike green (2026-09-24).** Replaces the OpenFreeMap proxy
(`virtues-core/src/server/atlas.rs`, [record](../record/map-atlas-plan.md))
with map data the box holds itself. When this ships, delete this plan and
rewrite the record.

## The goal

The map needs no key, no account and no API, costs nothing to run, and works
with the network unplugged for everywhere the person actually lives their life.
Nothing about which streets they look at leaves the box.

The OpenFreeMap proxy meets the first two and fails the rest: every area not
yet viewed needs the network, and the service "may discontinue at any time".

## The shape

```
                        once, then every ~90 days, and when you travel
Protomaps planet build ─────────── range reads ───────────▶ box: maps/
(one static .pmtiles file)                                   world.pmtiles      z0–6, everywhere
                                                             cells/10-x-y.pmtiles  z0–14, one per z10 cell

browser ── /api/map/vt/{world|local}/z/x/y ──▶ box ── mmap read ──▶ tile
        ── /api/map/fonts, /api/map/sprite ──▶ box ── shipped assets
```

- **World overview** (`world.pmtiles`): the whole planet at z0–6, 45 MB.
  MapLibre scales it up past z6, so the map always shows coastlines,
  countries, highways and city names, anywhere, offline.
- **Your places** (`cells/`): full street detail (z14) for every z10 cell the
  person's location history touches, plus the ring of cells around each. A z10
  cell is about 35 km across at mid-latitudes. One small archive per cell, so
  growth is additive and eviction is a file delete.
- **The style** is generated in the SPA from `@protomaps/basemaps` (BSD-3):
  a `world` source drawn underneath, then a `local` source on top with its
  background layer removed, so the world shows through wherever no cell exists.
  The box serves no style.
- **Fonts and icons** ship with the box as package data: 14 MB of Noto (OFL)
  and 180 KB of sprites in five themes, from `protomaps/basemaps-assets`.
- **No table, no migration.** A `maps/manifest.json` records the build each
  file came from. The data lives outside the lake (e.g.
  `/var/lib/virtues/maps`) because it is a regenerable cache and `virtues
  backup` archives the whole lake.

## What the spike proved

All in a scratch crate, nothing on `wave`.

| Question | Answer |
|---|---|
| Can the box cut a region without go-pmtiles? | Yes. A 200-line Rust extractor (own v3 header and directory reader, `pmtiles` crate's writer) cut a metro at z14 in 28 range requests. All 1,714 tiles came out byte-identical to `go-pmtiles extract`. |
| Can the box serve it? | Yes. `pmtiles` `AsyncPmTilesReader<MmapBackend>` in axum, tiles passed through gzipped with `Content-Encoding: gzip`. |
| Does it render? | Yes. Protomaps light and dark in MapLibre, labels and icons from local files, zero third-party requests. |
| Does the edge of your places show? | At z8, barely: the world overview carries highways, lakes and towns across it. At street zoom outside your cells you see only highways and a city name. That is the cost, and what "Travel" below is for. |
| Attribution | `© OpenStreetMap` in MapLibre's compact control: a small pill with an ⓘ. ODbL requires the credit, and the OSMF guidelines accept it collapsed behind an ⓘ. |

The crate's `Header` and `DirEntry` fields are `pub(crate)`, which is why the
extractor reads the format itself. The spike version buffers everything in
memory and fetches serially, so it took 49s where go-pmtiles took 14s. The real
one streams spans to the writer and fetches four at a time.

## Sizes (build 2026-09-24, measured by dry-run)

| What | Size |
|---|---|
| World z0–6 / z0–7 / z0–8 | 45 MB / 188 MB / 555 MB |
| One z10 cell at z14: rural / suburban / Manhattan / London / Paris | 0.6 / 5 / 13 / 19 / 20 MB |
| A metro area (z14) | ~26–40 MB |
| A US state (z14, Texas) | 661 MB |
| Continental US (z13 / z15) | 4.2 GB / 19 GB |
| Planet (z15) | ~138 GB |

z14 is about 40% of z15 and looks the same, because MapLibre overzooms z14.
A home area of two cells plus its ring is 12–18 cells, so **tens of MB**. A
person with twenty cities in their history is roughly 1–3 GB at the worst.

On the dev copy of real data, two months of location history covers **2** z10
cells.

## Which cells

Nightly, on the box:

1. Take `data_location_point`, bucket it into z10 cells, and keep cells with
   at least N points (N≈20), so a flight or a GPS glitch does not download a
   county.
2. Add the one-cell ring around each.
3. Diff against `manifest.json`, and extract only the new cells.

People with no location data yet get the world overview and nothing else,
until their phone reports.

## Where the planet comes from

Protomaps publishes daily planet builds at `build.protomaps.com/YYYYMMDD.pmtiles`,
keeps one week of them, and says "hotlinking … discouraged; copy the tileset
to your own storage." Reading ranges to cut an extract is the use their docs
point to, but a fleet doing it against their bucket is not.

- **Default source: a Virtues mirror.** Copy one build a month to R2 at a
  stable URL (name to be chosen). R2 has no egress fees;
  storage for ~140 GB is a few dollars a month. It is keyless and needs no
  account; the box only does HTTP `Range` GETs.
- **Configurable** (`VIRTUES_MAPS_SOURCE`), so a DIY box can point at
  `build.protomaps.com` or its own copy.
- **The honest privacy line:** whoever serves the planet file sees which byte
  ranges a box reads, which reveals roughly which z10 cells it wants. This
  happens once per cell, not per view. The ring around each cell blurs it a
  little, and asking for coarser cells blurs it more at a cost in megabytes.

## Refresh

Every ~90 days, re-extract all cells and the world file from the current
build into `.tmp` files, then swap each one atomically. Readers hold an mmap,
so the swap goes through an `ArcSwap` of readers, never an in-place write. OSM
changes slowly, and a map three months old is fine.

## Travel

This is the one real product decision (below). While someone is somewhere
new, the nightly job cannot help until their phone's points reach the box.

- **Strict offline:** new cells arrive the night after the points do. Until
  then, the world overview shows.
- **Fill-in on view (optional):** when a tile is missing, the box reads that
  one tile from the planet mirror (same schema, a `get_tile` over
  `HttpBackend` with cached directories), serves it, and marks the cell for
  the nightly job. This works on a trip with network and is keyless, but it
  sends per-view reads to the mirror, so it is weaker privacy than strict mode.

## Build order

1. **Serve.** `maps/` reader registry (world + cells), `/api/map/vt/{world|local}/z/x/y`,
   fonts and sprites from package data, and the installer carrying the assets.
   The world file downloads on first boot.
2. **Extract.** The Rust extractor made production-grade (streaming, four
   fetches at a time, retries, and a check that the result holds every tile
   id it was asked for), plus the nightly cell job and `manifest.json`.
3. **Frontend.** `$lib/map/atlas.ts` builds the style from
   `@protomaps/basemaps`, and Leaflet's attribution becomes a compact ⓘ.
   `MovementMap` and `DayGround` are unchanged: they already take
   `atlasLayer(style)`.
4. **Retire OpenFreeMap.** Delete the `/api/map/style`, `vt`, `fonts` and
   `sprite` proxy handlers and the `map_atlas/` cache, the same way
   `map_tiles/` was retired.
5. **Refresh + travel.** The 90-day swap and, if chosen, fill-in on view.
6. **Mirror.** A monthly job (virtues-api cron or a GitHub Action) copying the
   latest build to R2.

The theme can follow the house palette: a Protomaps flavor is only a table of
colors, so paper and ink are a small change, not a fork.

## Decisions for Adam

1. **Travel:** strict offline, or fill-in on view (default off, or on)?
2. **Disk cap for your places:** propose 2 GB, evicting the cells with the
   fewest points first.
3. **World detail:** z6 (45 MB), or z7 (188 MB) for regional roads at the
   edges of your places?
4. **Mirror:** run the R2 mirror, or point boxes at `build.protomaps.com`
   and accept its week-long URLs?
