# Offline maps — what is left

The box's own Protomaps maps are built end to end: the box serves its files,
virtues-api serves them to boxes, the monthly cut produces them, and the box
downloads what its owner's history calls for (2d6f3950, 1ee9fa7f, 5cb80a62,
e2733059). The design and why each rule is what it is:
[map-tiles.md](../record/map-tiles.md). No real box has files yet. Delete this
plan when the list below is empty.

## Open

- **Run it.** The first cut on the cloud server and a real box's first sync.
  Owned by [cloud-consolidation-plan.md](cloud-consolidation-plan.md), which
  has the steps.
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
