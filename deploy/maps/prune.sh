#!/bin/bash
# Remove old map builds from the maps bucket, keeping the newest ones.
#
#   prune.sh [keep]      builds to keep, the newest included (default 2)
#
# A build is a top-level prefix named YYYYMMDD. The build latest.json names is
# never removed, and nothing is removed when latest.json is missing or names
# a build the bucket does not have: that is a bucket to look at by hand.
# Config comes from ENV_FILE, as for upload.sh.
set -u
ENV_FILE="${ENV_FILE:-/etc/virtues/maps-writer.env}"
. "$ENV_FILE"
KEEP="${1:-2}"
[[ "$KEEP" =~ ^[1-9][0-9]*$ ]] || { echo "usage: prune.sh [keep]   (keep is 1 or more)" >&2; exit 2; }

# S3_BUCKET may be "<bucket>/<path>": listing takes the path as a prefix.
BUCKET="${S3_BUCKET%%/*}"
BASE="${S3_BUCKET#"$BUCKET"}"; BASE="${BASE#/}"; BASE="${BASE:+$BASE/}"

s3() { curl -fsS --user "$S3_ACCESS:$S3_SECRET" --aws-sigv4 "aws:amz:$S3_REGION:s3" "$@"; }
quote() { python3 -c 'import sys, urllib.parse; print(urllib.parse.quote(sys.argv[1], safe=""))' "$1"; }

# list <prefix> <xml tag> [delimiter]: every value of that tag across all pages.
list() {
  local token="" page q
  while :; do
    q="list-type=2&max-keys=1000&prefix=$(quote "$1")${3:+&delimiter=$(quote "$3")}${token:+&continuation-token=$(quote "$token")}"
    page=$(s3 "$S3_ENDPOINT/$BUCKET?$q") || return 1
    grep -o "<$2>[^<]*" <<< "$page" | sed "s/<$2>//"
    token=$(grep -o '<NextContinuationToken>[^<]*' <<< "$page" | sed 's/<NextContinuationToken>//')
    [ -n "$token" ] || break
  done
}

published=$(s3 "$S3_ENDPOINT/$S3_BUCKET/latest.json" 2>/dev/null | python3 -c 'import json, sys; print(json.load(sys.stdin)["build"])' 2>/dev/null) \
  || { echo "prune: latest.json is unreadable; nothing removed" >&2; exit 1; }
prefixes=$(list "$BASE" Prefix /) || { echo "prune: could not list the bucket; nothing removed" >&2; exit 1; }
builds=$(sed "s|^$BASE||; s|/\$||" <<< "$prefixes" | grep -E '^[0-9]{8}$' | sort)
grep -qx "$published" <<< "$builds" || { echo "prune: latest.json names $published, which the bucket does not have; nothing removed" >&2; exit 1; }

status=0
for build in $(head -n -"$KEEP" <<< "$builds"); do
  [ "$build" = "$published" ] && continue
  keys=$(list "$BASE$build/" Key) || { echo "prune: could not list build $build" >&2; status=1; continue; }
  n=$(grep -c . <<< "$keys")
  export S3_ACCESS S3_SECRET S3_REGION S3_ENDPOINT BUCKET
  if xargs -r -P 16 -I{} sh -c 'curl -fsS -o /dev/null -X DELETE --user "$S3_ACCESS:$S3_SECRET" --aws-sigv4 "aws:amz:$S3_REGION:s3" "$S3_ENDPOINT/$BUCKET/$1"' _ {} <<< "$keys"; then
    echo "$(date -Is) removed build $build ($n objects)"
  else
    echo "prune: some objects of build $build were not removed" >&2; status=1
  fi
done
exit $status
