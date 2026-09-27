# Map cut

The monthly job that produces the map files boxes download
(agents/plan/offline-maps-plan.md). It runs on the virtues-api server:
`cut.py` writes `/srv/maps/<build>/` and `/srv/maps/latest.json`, and
virtues-api serves them from `/v1/maps/*` (`services/virtues-api/src/routes/maps.rs`),
behind the box's bearer key and without logging.

## Install

```sh
sudo useradd --system --home /srv/maps --shell /usr/sbin/nologin virtues-maps
sudo install -d -o virtues-maps -g virtues-maps /srv/maps
sudo install -d /opt/virtues-maps
sudo cp cut.py /opt/virtues-maps/
sudo python3 -m venv /opt/virtues-maps/venv
sudo /opt/virtues-maps/venv/bin/pip install pmtiles
# go-pmtiles: the release binary for linux/amd64, installed as /usr/local/bin/pmtiles
sudo cp virtues-maps-cut.service virtues-maps-cut.timer /etc/systemd/system/
sudo systemctl daemon-reload && sudo systemctl enable --now virtues-maps-cut.timer
```

virtues-api reads the same directory: run its container with
`-v /srv/maps:/srv/maps:ro` and `VIRTUES_MAPS_DIR=/srv/maps`.

## What a run does

1. Picks the newest Protomaps daily build and downloads it once (~140 GB).
2. Cuts `world.pmtiles` (z0–7), then one file per land square: z5 squares
   at z13 (`visited-*`, ~750 of them) and z7 squares at z15 (`home-*`, ~9,200).
   Land is every square whose tile differs from the open-sea tile.
3. Bundles fonts and sprites into `assets.tar`, writes `index.json` with a
   sha256 per file, and publishes by renaming the directory, then
   `latest.json`.
4. Deletes the planet and all but the newest two builds.

A failed run leaves `<build>.partial/` and resumes where it stopped. Dry run
on a laptop, a few squares, no planet download:

```sh
python3 cut.py --maps-dir /tmp/maps --remote --limit 2
```

Disk: ~140 GB for the planet during a run, ~175 GB per build, two builds kept.
