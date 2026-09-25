#!/usr/bin/env bash
# Cut a dev set of the box's own maps (agents/plan/offline-maps-plan.md) into
# data/maps/, which the dev core reads. Production boxes download the same
# three kinds of file, pre-cut, from our file host; this makes them locally.
#
#   tools/maps-dev.sh [LON LAT]      default: central Austin
#
# Needs the Protomaps CLI (`go install github.com/protomaps/go-pmtiles@latest`,
# then `go-pmtiles` or `pmtiles` on PATH) and git. Downloads about 450 MB from
# build.protomaps.com, one client reading one build, which is the use its docs
# point to.
set -euo pipefail

LON="${1:--97.7431}"
LAT="${2:-30.2672}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/data/maps"
PMTILES="$(command -v pmtiles || command -v go-pmtiles || true)"
[ -n "$PMTILES" ] || { echo "maps-dev: install the Protomaps CLI: go install github.com/protomaps/go-pmtiles@latest" >&2; exit 1; }

# Newest daily build that exists (Protomaps keeps about a week).
BUILD=""
for d in 0 1 2 3 4 5 6; do
  day="$(date -u -v-"${d}"d +%Y%m%d 2>/dev/null || date -u -d "-${d} day" +%Y%m%d)"
  if curl -sfI "https://build.protomaps.com/${day}.pmtiles" >/dev/null; then BUILD="$day"; break; fi
done
[ -n "$BUILD" ] || { echo "maps-dev: no Protomaps build found in the last week" >&2; exit 1; }
SRC="https://build.protomaps.com/${BUILD}.pmtiles"
echo "∴ build $BUILD, location $LON,$LAT"

# The z-level tile holding LON/LAT, and its bounds, as "x y w,s,e,n".
square() {
  python3 - "$1" "$LON" "$LAT" <<'PY'
import math, sys
z = int(sys.argv[1]); n = 2 ** z
lon, lat = float(sys.argv[2]), float(sys.argv[3])
x = int((lon + 180) / 360 * n)
y = int((1 - math.asinh(math.tan(math.radians(lat))) / math.pi) / 2 * n)
X = lambda v: v / n * 360 - 180
Y = lambda v: math.degrees(math.atan(math.sinh(math.pi * (1 - 2 * v / n))))
e = 1e-6  # stay inside the tile, so the cut does not take its neighbors
print(x, y, f"{X(x)+e},{Y(y+1)+e},{X(x+1)-e},{Y(y)-e}")
PY
}

mkdir -p "$OUT"
# Not named `cut`: a function by that name shadows the coreutils one used
# below, and the size report then re-enters it with no bbox, which extracts
# the whole planet (138 GB).
cut_file() { # name, extra args...
  local name="$1"; shift
  case "$name $*" in
    world.pmtiles*|*--bbox=*) ;;
    *) echo "maps-dev: refusing to cut $name without a bbox" >&2; exit 1 ;;
  esac
  "$PMTILES" extract "$SRC" "$OUT/$name.tmp" "$@" --download-threads=4 -q
  mv "$OUT/$name.tmp" "$OUT/$name"
  echo "✓ $name ($(du -h "$OUT/$name" | cut -f1))"
}

[ -f "$OUT/world.pmtiles" ] || cut_file world.pmtiles --maxzoom=7
read -r vx vy vb < <(square 5)
cut_file "visited-z5-${vx}-${vy}.pmtiles" --bbox="$vb" --maxzoom=13
read -r hx hy hb < <(square 7)
cut_file "home-z7-${hx}-${hy}.pmtiles" --bbox="$hb" --maxzoom=15

# Fonts and icons (OFL / BSD), from protomaps/basemaps-assets.
tmp="$(mktemp -d)"
git clone -q --depth 1 --filter=blob:none --sparse https://github.com/protomaps/basemaps-assets "$tmp"
git -C "$tmp" sparse-checkout set fonts sprites/v4
rm -rf "$OUT/assets"; mkdir -p "$OUT/assets"
cp -R "$tmp/fonts" "$OUT/assets/fonts"
cp -R "$tmp/sprites/v4" "$OUT/assets/sprites"
rm -rf "$tmp"
echo "✓ assets ($(du -sh "$OUT/assets" | cut -f1))"
echo "∴ done: $OUT"
