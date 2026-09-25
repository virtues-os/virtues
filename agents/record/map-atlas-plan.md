# Map tiles

**Today the maps have no basemap on any real box.** Every Leaflet map in the
app (`MovementMap`, `DayGround`) draws its own background, tracks, pins and
tooltips, and fetches no tiles from anyone. The box can now draw maps from its
own Protomaps files (`virtues-core/src/maps`), but no box has any until
downloads are built (`agents/plan/offline-maps-plan.md`). When that ships,
this record is rewritten to describe it.

`$lib/map/atlas.ts` is the one seam: `atlasLayer(style)` returns the basemap
layer, or null when the box holds no map files.

## History

**CARTO, until 2026-09-23.** The box proxied CARTO's raster basemaps
(`light_all`, `dark_all`) and cached each tile in the lake under `map_tiles/`.
On 2026-09-23 CARTO started requiring an API key and answered every keyless
request with **HTTP 200 and a grey "API KEY REQUIRED" image**. The proxy
trusted the status and cached the placeholder like a real tile, so every map
broke and the cache kept the breakage. Released boxes still carry that
`map_tiles/` cache; the Protomaps work deletes it.

The failure class to keep: a provider that changes terms answers with a
placeholder, not an error. Anything that caches upstream bytes must check
that the bytes are the kind it asked for.

**OpenFreeMap, 2026-09-23 to 2026-09-24.** A replacement proxied OpenFreeMap's
vector tiles through the box and drew them with MapLibre inside Leaflet
(commit 2d49cf6b, which reached `v0.1.10-staging.84`). It worked, but every
tile a person looked at was a request naming that street to OpenFreeMap, and
unseen areas needed the network. It was removed rather than shipped to stable.
Its MapLibre-in-Leaflet wiring is in that commit for the Protomaps layer to
reuse.

A per-box CARTO key was rejected: every owner would have to register with
CARTO, and one shared key baked into the binary would pool every box against a
single commercial quota.
