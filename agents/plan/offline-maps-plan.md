# Offline maps — what is left

The box's own Protomaps maps are built end to end: the box serves its files,
virtues-api serves them to boxes, the monthly cut produces them, and the box
downloads what its owner's history calls for (2d6f3950, 1ee9fa7f, 5cb80a62,
e2733059). The design and why each rule is what it is:
[map-tiles.md](../record/map-tiles.md). The first build was cut on
2026-09-28 and a box on a prerelease synced from it on 2026-09-29; no
stable release has the sync yet, so no box on one has
files yet. Delete this plan when the list below is empty.

## Open

- **A refused fonts archive leaves `assets.new/` behind.** Harmless (nothing
  reads it), but `unpack_assets` could remove its staging directory when it
  bails.
- **Manual page:** where maps come from, and the privacy line: "Your server
  downloads maps in fixed regions, the same files every server in that region
  takes, so nothing it downloads says where in the region you live or what
  you look at. The server that hands them out keeps no logs." Only once a
  released box has files.
- **Setup and new boxes.** A new box has the world file only until its owner
  has spent days somewhere. Somewhere in Setup or the first map should say the
  map fills in over the first days.
- **Verify on real data:** the MapLibre worker (imported `?worker&url`) is
  unverified on iOS, and the thresholds (six ten-minute buckets a day, seven
  days for home, two for visited) were tuned on synthetic tracks, not a real
  history. The overlapping `data_location_visit` rows seen on the demo copy
  should be confirmed on dragon before being treated as a collector bug.
- **Basemap version.** `@protomaps/basemaps` is pinned (`apps/web/package.json`),
  but `index.json` does not record which basemap version a build was cut for,
  so nothing stops the two drifting. Record it, or upgrade them together by
  hand.
