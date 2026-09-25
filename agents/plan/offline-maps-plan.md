# Offline maps: Protomaps on the box

**Status: planned, spike green, decisions settled (2026-09-24).** The maps
have no basemap today ([record](../record/map-atlas-plan.md)): CARTO went
key-only, and the OpenFreeMap stopgap was removed because it leaked every
viewed street. This plan gives the box its own map files. When it ships,
delete this plan and rewrite the record.

## The goal

The map needs no third-party key or API, costs next to nothing to run, and
works with the network unplugged. **No download says where in a region the
person lives or which places they look at**, and nothing that serves the files
keeps a log.

## Everybody downloads the same bytes

A box never asks for *its* area. It downloads **fixed, pre-cut files**, and
every box in the same square downloads the identical file. A full log of every
download could still only say which squares a box took, and those squares are
coarser than what the box's IP address already reveals to any server it talks
to. The residual is travel (a box taking a distant square), which is covered
by keeping no logs.

## Three tiers, by importance

| Tier | For | Detail | Square (fixed files) | Size per file |
|---|---|---|---|---|
| **Home** | where you live, and where you have lived | z15: every café, bar and shop | z7, ~300 km | 187 MB (central Texas) to 614 MB (London) |
| **Visited** | anywhere you have spent real time | z13: every street and block | z5, ~1,000 km | 143 MB (rural) to 1.5 GB (Paris) |
| **World** | everywhere else | z0–7: coastlines to regional roads | one file | 188 MB |

Measured on the 2026-09-24 build, rendered side by side on Austin:

- **z15 vs z14:** downtown, z15 adds nearly every café, bar and shop. On a
  residential street they are near-identical. So z15 is the home tier, and z14
  has no tier.
- **z14 vs z13:** z13 keeps every block and main street name, but at z16 a
  residential street often loses its name, and POIs are gone. Right for places
  you passed through, wrong for where you live.
- **At city zoom (z13)** the three are indistinguishable.

The whole planet at z12 (18 GB) or z13 (36 GB) was considered as a zero-leak
option and dropped as too large to offer.

## Importance: days you spent real time there

The unit is **days present**, not visits or summed durations:

- A day counts for an area if the person spent at least an hour in it,
  measured from `data_location_point`, which every source produces.
- **Do not sum `data_location_visit.duration_minutes`.** On the dev copy of
  real data the visits overlap (12,397 overlapping pairs): their durations sum
  to 3,583 hours while their union covers 529. A separate task is fixing that.
- Visit counts would reward errands: ten coffee runs are not ten times one
  week.

**score(area) = Σ over days present of 0.5^(age ÷ 1 year)**

- **Recent bias:** a day last month counts about twice a day a year ago.
- **Former homes are kept:** an area with 90 or more lifetime days counts as
  "lived there" and qualifies for the home tier regardless of decay.
- **Home tier:** the top areas by score (z7 squares), up to ~2 GB, so a current
  and a former home both fit. A new city reaches it within a couple of weeks.
- **Visited tier:** any z5 square with 2 or more days, so a drive-through
  downloads nothing.
- **Cap: 4 GB** for home plus visited. Evict the lowest score first; the world
  file does not count.
- **New box:** the world file only, until the person has spent days somewhere.
  Getting started should say the map fills in over the first days.

## Serving and gating

```
monthly job on the file host
build.protomaps.com/YYYYMMDD.pmtiles ──go-pmtiles extract──▶ file host (OVH or Hetzner, nginx, no access log)
                                                               maps/<build>/world.pmtiles
                                                               maps/<build>/home-z7-<x>-<y>.pmtiles     z15
                                                               maps/<build>/visited-z5-<x>-<y>.pmtiles  z13
                                                               maps/<build>/index.json  (sha256 per file)

box ──Bearer key──▶ virtues-api /maps/sign ──▶ short-lived signed URL (≈1 hour)
box ──signed URL──▶ file host (nginx secure_link) ──▶ whole file, resumable ──▶ /var/lib/virtues/maps/
browser ──▶ box /api/map/vt/{world|home|visited}/z/x/y ──▶ mmap read
```

- **Everything is gated.** `/maps/sign` sits beside `routes/unsplash.rs` and
  uses the same `BearerAuth`. A lapsed subscription gets a 402, so no new files
  or refreshes, but what is on the box keeps working. A free self-hosted box
  points `VIRTUES_MAPS_SOURCE` at its own copy of the same layout.
- **No AWS in the data path.** virtues-api (EC2) only signs, a few hundred
  bytes per file. The gigabytes come from a box on OVH or Hetzner, where
  bandwidth is included or about a dollar a TB, against AWS's ~$90 a TB. Keep
  it separate from the relay, so map downloads never compete with relay
  traffic.
- **No logs, on both sides.** The file host runs nginx with `access_log off`
  and sees only a signature and an IP. virtues-api sees which account signed
  for which file, and its maps route records neither path nor account. Today
  `TraceLayer::new_for_http()` (`services/virtues-api/src/main.rs`) records
  request URIs at debug level, so the maps route has to be excluded from it.
- **Storage:** one build is ~175 GB (home z15 squares ≈ the planet at z15 over
  land, visited z13, world). Keep the old build a few days so in-flight
  downloads finish, then delete it.
- **Box refresh:** every ~90 days, download into `.tmp`, verify the sha256,
  swap atomically. Readers hold an mmap, so the swap goes through an
  `ArcSwap`, never an in-place write.
- **License:** the files are OpenStreetMap data under the ODbL. The file host
  carries a one-line license and attribution notice, and every map shows
  `© OpenStreetMap` in a compact ⓘ control, which the OSMF guidelines accept.

## On the box

- **Reader registry:** world, plus the home and visited files on disk, each an
  `AsyncPmTilesReader<MmapBackend>` (`pmtiles` crate, MIT/Apache). A tile
  request goes to the home file covering it, then visited, then world. Tiles
  pass through gzipped with `Content-Encoding: gzip`.
- **No table, no migration.** `maps/manifest.json` records build, tier and
  file. The data lives outside the lake because it is a regenerable cache and
  `virtues backup` archives the whole lake.
- **Legacy caches:** delete `map_tiles/` (CARTO, including its cached
  watermarks) and `map_atlas/` (OpenFreeMap, staging boxes only) from the lake
  on first start.
- **Fonts and icons** ship with the box as package data: 14 MB of Noto (OFL)
  and 180 KB of sprites, from `protomaps/basemaps-assets`.

## In the SPA

- `$lib/map/atlas.ts` builds the style from `@protomaps/basemaps` (BSD-3). It
  draws the world source underneath and the detail sources on top, with **their
  background layers removed**. Otherwise they paint over the world everywhere
  outside their square (found in the spike).
- MapLibre bound into Leaflet (`@maplibre/maplibre-gl-leaflet`), so
  `MovementMap` and `DayGround` keep their overlays unchanged. The bundler
  needs MapLibre's worker imported with `?worker&url`; commit 2d49cf6b has the
  working wiring.
- Pin `@protomaps/basemaps` to the basemap version the files were built with,
  and upgrade the two together.
- **Map POIs are display only.** Never name a visit from a nearby café on the
  map. A nearest-POI label is a guess, and the place-name resolver was killed
  for exactly that.
- The theme can follow the house palette: a Protomaps flavor is only a table
  of colors.

## What the spike proved

All in a scratch crate, nothing on `wave`.

- The box serves `.pmtiles` from mmap in axum, and Protomaps light and dark
  render from local files with zero third-party requests.
- A 200-line Rust extractor matched `go-pmtiles extract` byte for byte. It is
  not needed now that files are pre-cut, but it shows the format is simple.
- Sizes above are dry-run measurements. A metro area was cut for real in
  seconds.

## Build order

1. **Box serves local files.** Reader registry, tile/font/sprite routes, the
   style in the SPA, the ⓘ credit, legacy-cache purge. Developed against
   hand-placed files.
2. **File host.** Adam provisions an OVH or Hetzner box (~250 GB disk,
   included bandwidth); nginx with `secure_link` and no access log; the
   monthly cut job with `go-pmtiles` and `index.json`.
3. **Signing.** `/maps/sign` on virtues-api, excluded from request tracing.
4. **Box downloads.** Importance scoring, tier selection, resumable
   checksummed downloads, the 4 GB cap, the 90-day refresh.
5. **Manual page:** where maps come from, and the privacy line: "Your server
   downloads maps in fixed regions, the same files every server in that region
   takes, so nothing it downloads says where in the region you live or what
   you look at. The server that hands them out keeps no logs."

## Decisions

1. **Privacy:** fixed pre-cut files, identical for everyone in a square, plus
   no logs on the signing route and the file host.
2. **Tiers:** home z15 (~300 km squares), visited z13 (~1,000 km squares),
   world z0–7. No whole-world option.
3. **Importance:** days present with a one-year half-life; 90+ lifetime days
   keeps a former home; 2+ days for visited.
4. **Cap:** 4 GB for home plus visited.
5. **Hosting:** files on our own OVH or Hetzner box, never AWS; virtues-api
   only signs. Everything gated; a lapsed subscription keeps what it has.
6. **OpenFreeMap:** removed, not shipped.
