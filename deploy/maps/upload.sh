#!/bin/bash
# Upload one map build from MAPS_DIR to the maps bucket, then point the
# bucket's latest.json at it (deploy/maps/README.md).
#
#   upload.sh <build>
#
# Each file is sent with the sha256 the build's index records, so the storage
# service rejects a corrupted upload. latest.json goes last, and only when
# every file the index lists is uploaded, so virtues-api never serves half a
# build. Safe to run again: files already uploaded to this bucket are skipped.
# The last line of output, also written to <state>/<build>.result, says how
# it ended; the exit status is 0 only when latest.json was written.
#
# Config, from ENV_FILE (default /etc/virtues/maps-writer.env, root-only):
#   S3_ENDPOINT  https://s3.<region>.<provider>
#   S3_BUCKET    the maps bucket
#   S3_REGION    the region name the signature carries
#   S3_ACCESS, S3_SECRET   a key that can write to that bucket and no other
# Optional: MAPS_DIR (/srv/maps), JOBS (6 uploads at once), RATE (curl's
# --limit-rate per upload, default 4M: six uploads stay under 200 Mbps),
# FORCE=1 to point latest.json at a build older than the one it names.
set -u
ENV_FILE="${ENV_FILE:-/etc/virtues/maps-writer.env}"
. "$ENV_FILE"
MAPS_DIR="${MAPS_DIR:-/srv/maps}"
STATE_DIR="${STATE_DIR:-/var/lib/virtues-maps-upload}"
# What is uploaded is recorded per bucket: a run against one bucket says
# nothing about another.
STATE="$STATE_DIR/$S3_BUCKET"
SELF="$(realpath "$0")"

s3() { curl -fsS --user "$S3_ACCESS:$S3_SECRET" --aws-sigv4 "aws:amz:$S3_REGION:s3" "$@"; }

put() { # put <local file> <key> <sha256>
  s3 --retry 5 --retry-delay 5 --retry-connrefused --limit-rate "${RATE:-4M}" -o /dev/null \
    -H "x-amz-content-sha256: $3" -T "$1" "$S3_ENDPOINT/$S3_BUCKET/$2"
}

# One file: what the run below starts in parallel.
if [ "${1:-}" = "--one" ]; then
  build=$2 sha=$3 name=$4
  if put "$MAPS_DIR/$build/$name" "$build/$name" "$sha"; then echo "$name" >> "$STATE/$build.done"
  else echo "$name" >> "$STATE/$build.failed"; fi
  exit 0
fi

build="${1:-}"
[[ "$build" =~ ^[0-9]{8}$ ]] || { echo "usage: upload.sh <build>   (a build is a date, YYYYMMDD)" >&2; exit 2; }
dir="$MAPS_DIR/$build"
mkdir -p "$STATE"
exec 9> "$STATE/lock"
flock -n 9 || { echo "another upload to $S3_BUCKET is running" >&2; exit 1; }
touch "$STATE/$build.done"
: > "$STATE/$build.failed"
rm -f "$STATE/$build.result"

finish() { echo "$(date -Is) $1" | tee "$STATE/$build.result"; exit "$2"; }

# Every file the index lists that is not uploaded yet, as "<sha256> <name>".
# Fails when the directory and the index disagree.
todo() {
  python3 - "$dir" "$STATE/$build.done" <<'PY'
import json, os, sys
d, done = sys.argv[1], set(open(sys.argv[2]).read().split())
idx = {f["name"]: f["sha256"] for f in json.load(open(os.path.join(d, "index.json")))["files"]}
on_disk = set(os.listdir(d)) - {"index.json"}
assert on_disk == set(idx), f"index and directory disagree: {sorted(on_disk ^ set(idx))[:5]}"
for name, sha in sorted(idx.items()):
    if name not in done:
        print(sha, name)
PY
}

published=$(s3 "$S3_ENDPOINT/$S3_BUCKET/latest.json" 2>/dev/null | python3 -c 'import json, sys; print(json.load(sys.stdin)["build"])' 2>/dev/null || true)
if [ -n "$published" ] && [ "$build" \< "$published" ] && [ "${FORCE:-0}" != 1 ]; then
  finish "not uploaded: the bucket serves build $published, which is newer than $build (FORCE=1 to go back)" 1
fi

todo > "$STATE/$build.todo" || finish "not uploaded: could not read the index of build $build" 1
echo "$(date -Is) uploading $(wc -l < "$STATE/$build.todo") files of build $build to $S3_BUCKET"
ENV_FILE="$ENV_FILE" MAPS_DIR="$MAPS_DIR" STATE_DIR="$STATE_DIR" \
  xargs -r -a "$STATE/$build.todo" -P "${JOBS:-6}" -L 1 "$SELF" --one "$build"
started=$?

# Publish only when every file the index lists is recorded as uploaded. An
# empty .failed is not enough: an upload job that never started records nothing.
left=$(todo | wc -l)
uploaded=$(wc -l < "$STATE/$build.done")
if [ "$started" -ne 0 ] || [ "$left" -ne 0 ]; then
  finish "not published: uploaded=$uploaded left=$left failed=$(wc -l < "$STATE/$build.failed")" 1
fi
RATE=50M put "$dir/index.json" "$build/index.json" "$(sha256sum "$dir/index.json" | cut -d' ' -f1)" \
  || finish "not published: uploaded=$uploaded, but index.json did not upload" 1
printf '{"build": "%s"}' "$build" > "$STATE/latest.json"
RATE=50M put "$STATE/latest.json" "latest.json" "$(sha256sum "$STATE/latest.json" | cut -d' ' -f1)" \
  || finish "not published: uploaded=$uploaded, but latest.json did not upload" 1
finish "published build $build: uploaded=$uploaded" 0
