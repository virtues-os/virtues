# Map tiles

Written 2026-09-29; the design built 2026-09-25 to 09-29 (2d6f3950, 1ee9fa7f,
5cb80a62, e2733059). Every Leaflet map (`MovementMap`, `DayGround`) draws its
basemap from Protomaps files the box holds itself, so no tile provider learns
which streets anyone looks at and the maps work offline. `$lib/map/atlas.ts`
is the one seam: `atlasLayer(style)` returns the basemap layer, or null when
the box holds no files. Until the first cut runs on the cloud server
([cloud-consolidation-plan.md](../plan/cloud-consolidation-plan.md)), no real
box has files and the maps draw their own background only.

## Everybody downloads the same bytes

A box never asks for *its* area. It downloads fixed, pre-cut files, and every
box in the same square takes the identical file. A full log of every download
could only say which squares a box took, which is coarser than what its IP
address already tells any server it talks to. The residual, travel, is covered
by keeping no logs: the virtues-api maps routes sit outside `TraceLayer` (checked at
`RUST_LOG=debug`: no line from either), and the server's Caddy must not log
them either. The whole planet at z12 (18 GB) or z13
(36 GB) was the zero-leak alternative, dropped as too large to offer.

## Three tiers

| Tier | For | Detail | Square | Size per file |
|---|---|---|---|---|
| Home | where you live and have lived | z15, every café and shop | z7, ~300 km | ~190–610 MB |
| Visited | anywhere you spent real time | z13, every street and block | z5, ~1,000 km | ~140 MB–1.5 GB |
| World | everywhere else | z0–7 | one file | ~190 MB |

Rendered side by side (2026-09-24 build): z15 adds nearly every café, bar and
shop downtown over z14, and is near-identical on a residential street, so z14
has no tier. z13 keeps every block and main street name but drops POIs, right
for places passed through and wrong for home. Each tier is its own route and
MapLibre source because a source has one max zoom; the detail sources have
their background layers removed, or they paint over the world outside their
square.

## Which squares a box takes (`virtues-core/src/maps/sync.rs`)

The unit is **days present**: a UTC day counts for a square when
`data_location_point` has points in six or more distinct ten-minute buckets
there. Not summed visit durations (visits overlap; on the demo copy their
durations summed to about seven times their union), and not visit counts,
which reward errands. score = Σ 0.5^(age ÷ 365 days). Home takes the top z7
squares with 7+ days, or 90+ lifetime days whatever their age, within 2 GB;
visited takes z5 squares with 2+ days (one day is a drive-through); home plus
visited is capped at 4 GB, lowest score evicted first. Files refresh when the
published build is 90+ days newer, into `.tmp`, checked against the index's
sha256, swapped atomically.

## Serving

`deploy/maps/cut.py`, a monthly timer on the cloud server, cuts the newest
Protomaps build into `/srv/maps/<build>/`; virtues-api serves `/v1/maps/index`
and `/v1/maps/<build>/<file>` (with `Range`) behind `BearerAuth`. A lapsed
subscription gets a 402, so no new files, but what the box holds keeps
working. A free self-hosted box can point `VIRTUES_MAPS_SOURCE` at its own copy
of the same layout. The files are OpenStreetMap data under the ODbL; the index
carries the licence line and every map shows `© OpenStreetMap` in a compact ⓘ.

Map POIs are display only. A visit is never named from a nearby café on the
map: a nearest-POI label is a guess, and the place-name resolver was killed
for exactly that.

## Before Protomaps

**CARTO, until 2026-09-23.** The box proxied CARTO's raster basemaps and
cached each tile under `map_tiles/`. On 2026-09-23 CARTO started requiring an
API key and answered every keyless request with **HTTP 200 and a grey "API KEY
REQUIRED" image**. The proxy trusted the status and cached the placeholder like
a real tile, so every map broke and the cache kept the breakage. The failure
class to keep: a provider that changes terms answers with a placeholder, not
an error, so anything that caches upstream bytes must check they are the kind
it asked for. A per-box CARTO key was rejected: every owner would have to
register, and one shared key in the binary would pool every box against one
commercial quota.

**OpenFreeMap, 2026-09-23 to 09-24.** Proxied vector tiles drawn with MapLibre
inside Leaflet (2d49cf6b, reached `v0.1.10-staging.84`). Every tile viewed was
a request naming that street to a third party, so it was removed rather than
shipped to stable; its MapLibre-in-Leaflet wiring is what the Protomaps layer
reuses. The box deletes both old caches (`map_tiles/`, `map_atlas/`) on first
use.
