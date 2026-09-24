# Offline maps: Protomaps on the box

**Status: planned, spike green (2026-09-24).** Replaces the OpenFreeMap proxy
(`virtues-core/src/server/atlas.rs`, [record](../record/map-atlas-plan.md))
with map data the box holds itself. When this ships, delete this plan and
rewrite the record.

## The goal

The map needs no key, no account and no API, costs nothing to run, and works
with the network unplugged. **Nobody, Virtues included, can tell from a
download where the person lives or which places they look at.**

The OpenFreeMap proxy fails the last two: every unseen area needs the network,
and every tile request names a street.

## The one idea: everybody downloads the same bytes

A box never asks for *its* area. It downloads **fixed, pre-cut bundles**, and
every box in the same bundle downloads the identical file. The server learns
which bundles a box took and nothing finer.

A bundle is one z5 tile: about 1,000 km across at mid-latitudes, several US
states or a large European country. The request reveals which square the box
is in, which is coarser than what its IP address already tells any server it
connects to. The residual leak is travel: a box in one bundle taking another
says the owner has an interest there (see Travel).

**The whole-world option removes even that.** The planet at z12 is 18 GB,
the same file for every box on Earth, and it carries every street and street
name.

An earlier draft of this plan had the box cut its own extract by range-reading
the planet. That told the server roughly which ~35 km squares each box wanted.
It is superseded, and the box no longer needs an extractor at all.

## The shape

```
weekly, Virtues-run job
build.protomaps.com/YYYYMMDD.pmtiles ──extract──▶ R2: maps/<build>/world.pmtiles        z0–7, 188 MB
                                                      maps/<build>/b5-<x>-<y>.pmtiles   z0–14 per z5 tile
                                                      maps/<build>/planet-z12.pmtiles   18 GB (whole-world option)
                                                      maps/latest.json                  build + sha256 per file

box ── plain GET of whole files (resumable, checksummed) ──▶ /var/lib/virtues/maps/
browser ── /api/map/vt/{world|detail}/z/x/y ──▶ box ── mmap read ──▶ tile
        ── /api/map/fonts, /api/map/sprite ──▶ box ── shipped assets
```

- **World overview** (`world.pmtiles`, z0–7, 188 MB): every box, first boot.
  Coastlines, countries, highways, regional roads and city names, anywhere.
- **Detail bundles** (`b5-x-y.pmtiles`, z0–14): the box takes the bundle
  its location history sits in (below). 0.3–3 GB each.
- **Whole world** (`planet-z12.pmtiles`, 18 GB): opt-in instead of bundles.
  No leak at all, fully offline everywhere, with no shops or POIs and fewer
  alleys than z14.
- **The style** is generated in the SPA from `@protomaps/basemaps` (BSD-3):
  a `world` source underneath, then a `detail` source on top with its
  background layer removed so the world shows through outside a bundle. The
  box serves no style.
- **Fonts and icons** ship with the box as package data: 14 MB of Noto (OFL)
  and 180 KB of sprites in five themes, from `protomaps/basemaps-assets`.
- **No table, no migration.** `maps/manifest.json` records the build and
  bundles on disk. The data lives outside the lake because it is a
  regenerable cache and `virtues backup` archives the whole lake.

## What the spike proved

All in a scratch crate, nothing on `wave`.

| Question | Answer |
|---|---|
| Can the box serve `.pmtiles`? | Yes. `pmtiles` `AsyncPmTilesReader<MmapBackend>` in axum, tiles passed through gzipped with `Content-Encoding: gzip`. |
| Does it render? | Yes. Protomaps light and dark in MapLibre, labels and icons from local files, zero third-party requests. |
| Does a bundle's edge show? | At z8, barely: the world file carries highways, lakes and towns across it. At street zoom outside every bundle you see only highways and city names. |
| What does less detail cost? | At z15 downtown: z12 data keeps every street and its name, blocks, parks and water, with no POIs. z13 adds alleys and minor streets. z14 adds shops, museums and restaurants. |
| Attribution | `© OpenStreetMap` in MapLibre's compact control: a small pill with an ⓘ. ODbL requires the credit, and the OSMF guidelines accept it collapsed behind an ⓘ. |
| Style trap | The detail source's `background` layer must be dropped, or it paints over the world source everywhere outside the bundle. |

The spike also built a Rust extractor (own PMTiles v3 reader, the crate's
writer) that matched `go-pmtiles extract` byte for byte. Bundles make it
unnecessary on the box; the weekly job uses `go-pmtiles`.

## Sizes (build 2026-09-24, measured by dry-run)

| What | z12 | z13 | z14 |
|---|---|---|---|
| World overview | z0–7: 188 MB (z0–6: 45 MB) | | |
| Bundle: rural Montana | | 143 MB | 298 MB |
| Bundle: central Texas | | 160 MB | 350 MB |
| Bundle: New York | | 266 MB | 577 MB |
| Bundle: Tokyo | | 323 MB | 683 MB |
| Bundle: London | | 602 MB | 1.2 GB |
| Bundle: Paris (densest found) | | 1.5 GB | 2.9 GB |
| North America | 4.2 GB | 8.8 GB | |
| Europe | 6.6 GB | 13 GB | 24 GB |
| **Planet** | **18 GB** | 36 GB | ~138 GB at z15 |

## Which bundles

- **Home:** nightly, bucket `data_location_point` into z5 tiles, keep tiles
  with at least N points (N≈20, so a flight's GPS trace downloads nothing),
  and fetch any bundle not on disk.
- **Near an edge:** if a cluster of points sits within ~50 km of a bundle's
  edge, also take the neighbor. It is still a fixed file everyone near that
  edge takes, so the privacy class is the same.
- **No location data yet:** the world overview only, until the phone reports.
- **Disk cap: 4 GB of bundles.** Over the cap, evict the bundle with the
  fewest points, never the one holding the most. The world file does not
  count. The whole-world option replaces bundles and ignores the cap.

## Travel

A bundle changes only when you cross ~1,000 km, so most trips never trigger a
download. The question is what a box does when one does:

| Option | For | Against |
|---|---|---|
| A. Take it the night after the points arrive | Simple; the map of the trip is complete by the time you look back at it | The server learns the owner went somewhere in that square, and roughly when |
| B. Take it as soon as ~20 points arrive | Ready during the trip | Same leak as A, and sooner |
| C. Never take travel bundles | Zero travel leak | Trips outside home show only the world overview |
| D. Whole world | Zero leak, travel solved everywhere | 18 GB, and z12 detail (no POIs) |

Timing is the part a bundle cannot hide. A download of bundle X on the day a
trip starts says when, even though the bytes say nothing. A random delay of
up to a few days (A with jitter) blurs it at the cost of freshness.

**Recommendation:** A with jitter by default, and D offered in Settings as
"Download the whole world (18 GB)" for anyone who wants the leak gone.

## Where the bundles come from

**Decided: Virtues hosts them, and boxes pull from us.**

- **Weekly**, a job reads the newest `build.protomaps.com/YYYYMMDD.pmtiles`
  by range and cuts the world file, every land-touching z5 bundle and the
  z12 planet straight to R2. It is one client reading one build a week,
  which is the use Protomaps' docs point to. The fleet never touches
  Protomaps.
- **Layout:** `maps/<build>/…` plus `maps/latest.json` naming the build and a
  sha256 per file. Keep the previous build for a week so a download that
  started before the switch does not 404.
- **Cost:** bundles total about the planet at z14 (~55–60 GB, estimated from
  z14 ≈ 40% of z15), plus 18 GB for z12 and 188 MB for the world file, so
  ~80 GB per build and ~160 GB with two builds live. At R2's
  $0.015/GB-month that is **about $2.50 a month**. R2 charges nothing for
  egress.
- **Box refresh:** every ~90 days, take the files named by `latest.json`
  into `.tmp`, verify the sha256, and swap atomically. Readers hold an mmap,
  so the swap goes through an `ArcSwap`, never an in-place write.
- **Configurable** (`VIRTUES_MAPS_SOURCE`), so a DIY box can point at its own
  copy of the same layout.
- **Privacy line for the manual:** "Your server downloads maps in regions
  about 1,000 km across, the same files every server in that region takes.
  Nothing it downloads says where in the region you live or which places you
  look at. A download outside your home region shows you were interested in
  that region. Download the whole world instead to avoid even that."

## Build order

1. **Serve.** `maps/` reader registry (world + detail),
   `/api/map/vt/{world|detail}/z/x/y`, fonts and sprites from package data,
   the installer carrying the assets.
2. **Mirror job.** Weekly cut to R2 with `go-pmtiles` (an Action or the cloud
   EC2), `latest.json`, sha256 per file.
3. **Download.** Resumable, checksummed whole-file GETs; the nightly bundle
   chooser; the 4 GB cap; the whole-world setting.
4. **Frontend.** `$lib/map/atlas.ts` builds the style from
   `@protomaps/basemaps`, and Leaflet's attribution becomes a compact ⓘ.
   `MovementMap` and `DayGround` are unchanged: they already take
   `atlasLayer(style)`.
5. **Retire OpenFreeMap.** Delete the proxy handlers and the `map_atlas/`
   cache, the same way `map_tiles/` was retired.
6. **Refresh + travel**, per the decision below.

The theme can follow the house palette: a Protomaps flavor is only a table of
colors.

## Decisions

1. **Privacy model (proposed):** fixed bundles, the same bytes for everyone
   in them. Nothing a box downloads names a place finer than ~1,000 km.
2. **Disk cap:** 4 GB of bundles.
3. **World detail:** z0–7, 188 MB.
4. **Source:** Virtues cuts bundles weekly to R2; boxes pull from us.
5. **Travel:** open; recommendation above (A with jitter, whole world as a
   setting).
6. **Bundle detail:** z14 (0.3–3 GB) or z13 (half the size, no POIs)?
   Recommend z14.
