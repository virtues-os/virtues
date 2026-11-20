# Map cut

The monthly job that produces the map files boxes download
(agents/record/map-tiles.md). It runs on the virtues-api server:
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

virtues-api can read the same layout from S3-compatible object storage
instead: `latest.json` at the top of a bucket and one `<build>/` prefix per
build. Set the five `VIRTUES_MAPS_S3_*` variables
(`services/virtues-api/src/config.rs`) with a key that can only read that
bucket. When they are set the directory is not read.

`upload.sh <build>` copies one build from `/srv/maps` to the bucket with a
key that can write to it, checks each file against the index's sha256 on the
way, and writes `latest.json` last, so the API never serves half a build.
On the dedicated server it is installed as `/opt/virtues-maps/upload.sh`,
with its key in `/etc/virtues/maps-writer.env`, for uploading a build by
hand. Production reads the bucket, and the monthly run on a temporary server
(below) is what normally fills it.

## The run on a temporary server

The cut needs about 310 GB of disk for an hour, which is more than any server
we keep for the services. `run.sh` is the whole job for a server that exists
only for the run: it installs the tools at pinned versions, runs `cut.py`,
uploads with `upload.sh`, removes all but the newest two builds from the
bucket with `prune.sh`, and reports each step to whoever started the server.

The operator's intranet starts it: on a schedule it creates the server with a
first-boot script that downloads this directory at one pinned commit, a
storage key made for the run, and a token for reporting back. It deletes the
server and the key on the final report, or after five hours without one.
`run.sh` cannot delete its own server.

A run given `PREFIX` uploads under `<bucket>/<PREFIX>/`. virtues-api reads
only the top of the bucket, so that is a rehearsal no box sees.

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
