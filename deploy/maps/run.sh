#!/bin/bash
# The whole monthly map job, on a temporary server that exists only for the
# run: install the tools, cut the newest Protomaps build (cut.py), upload it
# to the maps bucket (upload.sh), remove old builds from the bucket
# (prune.sh), and report each step to whoever started the server.
#
# It is started by the server's first-boot script, which downloads this
# directory at one pinned commit and writes the config. Whoever started the
# server deletes it when this reports "done" or "failed", or when it has not
# reported for too long: this script cannot delete its own server.
#
# Config, from ENV_FILE (default /etc/virtues/maps-run.env, root-only):
#   RUN_ID        the caller's name for this run
#   REPORT_URL    where each step is POSTed as JSON
#   REPORT_TOKEN  bearer token for REPORT_URL, valid for this run only
#   S3_ENDPOINT, S3_BUCKET, S3_REGION, S3_ACCESS, S3_SECRET
#                 the maps bucket and a key that can write to it
#   PREFIX        optional. Upload under <bucket>/<PREFIX>/ instead of the top
#                 of the bucket. virtues-api reads only the top, so a run with
#                 a prefix is a rehearsal that no box sees.
# Output goes to LOG (default /var/log/virtues-maps-run.log); its tail is sent
# with a failure, and the whole file is uploaded to runs/<RUN_ID>.log.
set -u
ENV_FILE="${ENV_FILE:-/etc/virtues/maps-run.env}"
. "$ENV_FILE"
HERE="$(dirname "$(realpath "$0")")"
MAPS_DIR="${MAPS_DIR:-/srv/maps}"
LOG="${LOG:-/var/log/virtues-maps-run.log}"
VENV=/opt/virtues-maps/venv

# The versions the cut was written against. The binary is checked against the
# sha256 of the one that has cut every build so far.
PMTILES_VERSION=1.31.2
PMTILES_SHA256=a7e9ae10184d109c83f456ccdf6df4f3e2a64ba6cf69d9ed0f9f1840305055c1
PMTILES_PY=3.8.1

PHASE=setup
BUILD=""
STARTED=$(date +%s)

# report <state> [message]: state is running, done or failed. Never fails the run.
# The final report is what gets this server deleted, so it keeps trying for
# ten minutes; a step report gives up after one.
report() {
  local tries=2
  [ "$1" = running ] || tries=20
  python3 - "$1" "$PHASE" "${2:-}" "$BUILD" "$LOG" <<'PY' |
import json, sys
state, phase, message, build, log = sys.argv[1:6]
tail = ""
if state == "failed":
    try:
        tail = "".join(open(log, errors="replace").readlines()[-40:])[-3000:]
    except OSError:
        pass
print(json.dumps({"state": state, "phase": phase, "message": message, "build": build or None, "log": tail}))
PY
    curl -fsS -m 30 --retry "$tries" --retry-delay 30 --retry-all-errors -X POST "$REPORT_URL" \
      -H "Authorization: Bearer $REPORT_TOKEN" -H 'Content-Type: application/json' --data-binary @- > /dev/null \
      || echo "report: could not reach $REPORT_URL"
}

save_log() {
  curl -fsS -m 120 --user "$S3_ACCESS:$S3_SECRET" --aws-sigv4 "aws:amz:$S3_REGION:s3" \
    -T "$LOG" "$S3_ENDPOINT/$S3_BUCKET/runs/$RUN_ID.log" -o /dev/null || echo "the log did not upload"
}

# What the system did around a failure: package runs and unit restarts are
# the usual outside cause.
diagnose() {
  echo "--- up since $(uptime -s 2>/dev/null); last package runs:"
  grep -E '^(Start-Date|Commandline|End-Date)' /var/log/apt/history.log 2>/dev/null | tail -9
  journalctl --no-pager -o short-iso -n 15 -u virtues-maps-run -u unattended-upgrades -u apt-daily-upgrade 2>/dev/null
}

# The final report makes the caller delete the storage key, so the log is
# uploaded first.
fail() {
  echo "✖ $PHASE: $1"
  diagnose
  save_log
  report failed "$1"
  exit 1
}

# This script runs once per server. A second start means something restarted
# its service and killed the first run partway, so say that and stop.
MARK=/run/virtues-maps-run.started
if [ -e "$MARK" ]; then
  fail "run.sh was started a second time; a restart of its service stopped the first run"
fi
touch "$MARK"

echo "$(date -Is) map run $RUN_ID on $(nproc) cores, $(df -BG --output=avail / | tail -1 | tr -d ' ') free"
report running

# Ubuntu's background upgrades start within the first hour of a new server.
# They take the package lock, and their restart hook (needrestart) restarts
# every service whose libraries changed, this one included. Switch both off
# for the life of the server, and finish any package run they were stopped in.
export DEBIAN_FRONTEND=noninteractive NEEDRESTART_SUSPEND=1
systemctl mask --now apt-daily.timer apt-daily-upgrade.timer apt-daily.service apt-daily-upgrade.service unattended-upgrades.service > /dev/null 2>&1
dpkg --configure -a > /dev/null 2>&1
APT="apt-get -o DPkg::Lock::Timeout=600 -y -qq"
$APT update && $APT install python3-venv git curl > /dev/null || fail "could not install packages"
curl -fsSL --retry 5 "https://github.com/protomaps/go-pmtiles/releases/download/v$PMTILES_VERSION/go-pmtiles_${PMTILES_VERSION}_Linux_x86_64.tar.gz" \
  | tar -xz -C /usr/local/bin pmtiles || fail "could not download pmtiles $PMTILES_VERSION"
echo "$PMTILES_SHA256  /usr/local/bin/pmtiles" | sha256sum -c --quiet || fail "the pmtiles binary is not the pinned one"
python3 -m venv "$VENV" && "$VENV/bin/pip" install -q "pmtiles==$PMTILES_PY" || fail "could not install the pmtiles reader"

# upload.sh and prune.sh read the bucket from this file. With PREFIX the
# destination is a path inside the bucket.
install -d -m 700 /etc/virtues
( umask 077; cat > /etc/virtues/maps-writer.env <<EOF
S3_ENDPOINT=$S3_ENDPOINT
S3_BUCKET=$S3_BUCKET${PREFIX:+/$PREFIX}
S3_REGION=$S3_REGION
S3_ACCESS=$S3_ACCESS
S3_SECRET=$S3_SECRET
EOF
)

PHASE=cutting
report running
JOBS=$(( $(nproc) > 2 ? $(nproc) - 2 : 1 ))
PATH="/usr/local/bin:$PATH" "$VENV/bin/python" "$HERE/cut.py" --maps-dir "$MAPS_DIR" --jobs "$JOBS" --keep 1 || fail "the cut failed"
BUILD=$(python3 -c 'import json, sys; print(json.load(open(sys.argv[1]))["build"])' "$MAPS_DIR/latest.json") || fail "the cut left no latest.json"
# The file count and size the index records, e.g. "9920 178.2 GB".
read -r FILES SIZE < <(python3 -c 'import json, sys
files = json.load(open(sys.argv[1]))["files"]
print(len(files), "%.1f GB" % (sum(f["bytes"] for f in files) / 1e9))' "$MAPS_DIR/$BUILD/index.json") || fail "the cut left no index.json"

PHASE=uploading
report running
MAPS_DIR="$MAPS_DIR" JOBS=16 RATE=1G "$HERE/upload.sh" "$BUILD" || fail "the upload did not publish build $BUILD"

PHASE=pruning
report running
note=""
"$HERE/prune.sh" 2 || note=" Old builds were not removed from the bucket; see the log."

PHASE="done"
mins=$(( ($(date +%s) - STARTED) / 60 ))
echo "$(date -Is) ✓ build $BUILD: $FILES files, $SIZE, $mins minutes"
save_log
report "done" "Build $BUILD: $FILES files, $SIZE, in $mins minutes.$note"
