# Offline maps — what is left

The box's own Protomaps maps are built end to end: the box serves its files,
virtues-api serves them to boxes, the monthly cut produces them, and the box
downloads what its owner's history calls for (2d6f3950, 1ee9fa7f, 5cb80a62,
e2733059). The design and why each rule is what it is:
[map-tiles.md](../record/map-tiles.md). The first build was cut on
2026-09-28 and a dev box synced from it on 2026-09-29; no released box has
files yet. Delete this plan when the list below is empty.

## Open

- **Fonts on a box.** The first build's `assets.tar` held symlinks, which
  the box refuses, so the synced box has tiles but no fonts or sprites.
  Fixed in `cut.py`; the next build has to land and a box unpack it. Owned
  by [cloud-consolidation-plan.md](cloud-consolidation-plan.md). The box
  leaves a half-unpacked `assets.new/` behind when it refuses an archive;
  harmless (nothing reads it), but it could clean up after itself.
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
