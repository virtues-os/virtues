#!/bin/bash
# Upload one map build from MAPS_DIR to the maps bucket, then point the
# bucket's latest.json at it (deploy/maps/README.md).
#
#   upload.sh <build>
#
# Each file is sent with the sha256 the build's index records, so the storage
# service rejects a corrupted upload. latest.json goes last, so virtues-api
# never serves half a build. Safe to run again: files already uploaded are
# skipped.
#
# Config, from ENV_FILE (default /etc/virtues/maps-writer.env, root-only):
#   S3_ENDPOINT  https://s3.<region>.<provider>
#   S3_BUCKET    the maps bucket
#   S3_REGION    the region name the signature carries
#   S3_ACCESS, S3_SECRET   a key that can write to that bucket and no other
# Optional: MAPS_DIR (/srv/maps), JOBS (6 uploads at once), RATE (curl's
# --limit-rate per upload, default 4M: six uploads stay under 200 Mbps).
set -u
ENV_FILE="${ENV_FILE:-/etc/virtues/maps-writer.env}"
. "$ENV_FILE"
MAPS_DIR="${MAPS_DIR:-/srv/maps}"
STATE="${STATE_DIR:-/var/lib/virtues-maps-upload}"

put() { # put <local file> <key> <sha256>
  curl -fsS --retry 5 --retry-all-errors --retry-delay 5 --limit-rate "${RATE:-4M}" -o /dev/null \
    --user "$S3_ACCESS:$S3_SECRET" --aws-sigv4 "aws:amz:$S3_REGION:s3" \
    -H "x-amz-content-sha256: $3" -T "$1" "$S3_ENDPOINT/$S3_BUCKET/$2"
}

# One file: what the run below starts in parallel.
if [ "${1:-}" = "--one" ]; then
  build=$2 sha=$3 name=$4
  if put "$MAPS_DIR/$build/$name" "$build/$name" "$sha"; then echo "$name" >> "$STATE/$build.done"
  else echo "$name" >> "$STATE/$build.failed"; fi
  exit 0
fi

build="${1:?usage: upload.sh <build>}"
dir="$MAPS_DIR/$build"
mkdir -p "$STATE"
touch "$STATE/$build.done"
: > "$STATE/$build.failed"
rm -f "$STATE/$build.FINISHED"

# Every file the index lists and this run has not uploaded yet, as "<sha256> <name>".
python3 - "$dir" "$STATE/$build.done" > "$STATE/$build.todo" <<'PY'
import json, os, sys
d, done = sys.argv[1], set(open(sys.argv[2]).read().split())
idx = {f["name"]: f["sha256"] for f in json.load(open(os.path.join(d, "index.json")))["files"]}
on_disk = set(os.listdir(d)) - {"index.json"}
assert on_disk == set(idx), f"index and directory disagree: {sorted(on_disk ^ set(idx))[:5]}"
for name, sha in sorted(idx.items()):
    if name not in done:
        print(sha, name)
PY
[ $? -eq 0 ] || { echo "could not read the index of build $build" | tee "$STATE/$build.FINISHED"; exit 1; }

echo "$(date -Is) uploading $(wc -l < "$STATE/$build.todo") files of build $build"
ENV_FILE="$ENV_FILE" MAPS_DIR="$MAPS_DIR" STATE_DIR="$STATE" xargs -a "$STATE/$build.todo" -P "${JOBS:-6}" -L 1 "$0" --one "$build"

failed=$(wc -l < "$STATE/$build.failed")
if [ "$failed" -eq 0 ]; then
  RATE=50M put "$dir/index.json" "$build/index.json" "$(sha256sum "$dir/index.json" | cut -d' ' -f1)" \
    && printf '{"build": "%s"}' "$build" > "$STATE/latest.json" \
    && RATE=50M put "$STATE/latest.json" "latest.json" "$(sha256sum "$STATE/latest.json" | cut -d' ' -f1)" \
    || failed=index
fi
echo "$(date -Is) uploaded=$(wc -l < "$STATE/$build.done") failed=$failed" | tee "$STATE/$build.FINISHED"
[ "$failed" = 0 ]
