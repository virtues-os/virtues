# Map tile cache

The box serves the map instead of the browser hitting a third party. It caches
each piece the first time it's needed, so **the browser only ever talks to the
box** (no location leak to a map provider) and **already-seen areas work
offline**.

## How it works

```
browser (MapLibre inside Leaflet) → GET /api/map/… → box
            cache hit  → serve from the lake     (never leaves the box)
            cache miss → fetch once from OpenFreeMap → check type → store → serve
```

| Route | What | Cached at |
|---|---|---|
| `/api/map/style/:style` | `light` (Positron) or `dark`, rewritten so every URL names the box | `map_atlas/style/` |
| `/api/map/vt/:z/:x/:y` | OpenMapTiles vector tile, z0 to z14 | `map_atlas/vt/` |
| `/api/map/fonts/:fontstack/:range` | label glyphs | `map_atlas/fonts/` |
| `/api/map/sprite/:set/:file` | icon sheet | `map_atlas/sprite/` |

- The lake is the index. **No table, no migration**, and `virtues backup`
  carries it.
- OpenFreeMap versions tile URLs by build. The box reads the current build from
  its TileJSON and keeps it for six hours. The build is **not** in the cache
  key, so tiles cached from an older build keep serving, offline included.
- Only the style is revalidated (`no-cache`). Everything else is served
  `immutable`, because the box's copy never changes.
- Offline or upstream error → `502`. MapLibre leaves the gap and draws the rest.

## What it touches

- `virtues-core/src/server/atlas.rs`: the handlers and the style rewrite.
- `apps/web/src/lib/map/atlas.ts`: `atlasLayer(style)` returns a Leaflet layer
  (MapLibre GL via `@maplibre/maplibre-gl-leaflet`). Used by `MovementMap` and
  `DayGround`. Tracks, pins and tooltips stay ordinary Leaflet overlays.
- Attribution rides on the style's source (`OpenFreeMap © OpenMapTiles Data
  from OpenStreetMap`), which the binding copies into Leaflet's control. A map
  using the atlas keeps `attributionControl: true`.

## The terms

OpenFreeMap needs no key and no registration, allows commercial use, and sets
no request limit. Its terms forbid automated bulk collection. Lazy per-view
caching is fine; **do not add a prefetch-a-region feature against it.** If a box
ever needs a whole region offline, serve a Protomaps `.pmtiles` extract from
the lake instead.

## History: CARTO, until 2026-09-23

The first version proxied CARTO's raster basemaps (`light_all`, `dark_all`).
On 2026-09-23 CARTO started requiring an API key: 1M requests a month free for
commercial use, then $500 a month. Every keyless request after that got
**HTTP 200 with a grey "API KEY REQUIRED" image**. The old handler trusted the
status and cached the placeholder like a real tile, so every map broke and
the cache kept the breakage.

Two things came out of that:

- `fetch_checked` refuses to cache anything whose content type is not the one
  asked for. A provider changing terms answers with a placeholder, not an
  error, and the type check catches every placeholder that is not itself a
  valid map object.
- The old raster cache (`map_tiles/`) is deleted the first time a new box
  serves a map style. Browsers had cached the watermarks under
  `/api/map/tiles/…` for a year (`immutable`); the new routes use new paths, so
  those copies are never asked for again.

A per-box CARTO key was rejected: every owner would have to register with
CARTO, and one shared key baked into the binary would pool every box against a
single commercial quota.
