# Map tile cache

The box serves map tiles instead of the browser hitting a third party. It caches
each tile the first time it's needed, so **the browser only ever talks to the box**
(no location leak to a map provider) and **already-seen areas work offline**.

## How it works

```
browser → GET /api/map/tiles/light/{z}/{x}/{y} → box
            cache hit  → serve from disk        (never leaves the box)
            cache miss → fetch once from CartoDB → store → serve
```

- Stored at `map_tiles/{style}/{z}/{x}/{y}.png` in the lake root (so `virtues
  backup` includes it). The filesystem is the index — **no table, no migration**.
- Tiles are immutable, so we serve `Cache-Control: max-age=1y, immutable`; the
  browser caches too and repeat views never reach the box.
- Offline / upstream error → `502`; Leaflet's blank `errorTileUrl` shows grey
  gaps, not broken images.

## What it touches

- `virtues-core/src/server/api.rs` — `map_tile_handler` (+ route in `server/mod.rs`).
- `MovementMap.svelte` — Leaflet `tileLayer` points at `/api/map/tiles/light/...`,
  attribution kept.

## One caveat

CartoDB is the default source, named in a single `const`, and **swappable**. Keep
attribution (`© OpenStreetMap contributors © CARTO`). Lazy per-view caching is
fine; don't bulk-download regions against a free tier.

That's the whole thing.
