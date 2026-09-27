#!/usr/bin/env python3
"""Cut one month's map files for the boxes (agents/plan/offline-maps-plan.md).

Reads one Protomaps daily planet build and writes, under MAPS_DIR/<build>/:

    world.pmtiles                   z0-7, the planet
    visited-z5-<x>-<y>.pmtiles      z0-13, one per z5 tile holding land
    home-z7-<x>-<y>.pmtiles         z0-15, one per z7 tile holding land
    assets.tar                      fonts + sprites (protomaps/basemaps-assets)
    index.json                      every file: tier, square, bytes, sha256

then points MAPS_DIR/latest.json at it, which is what virtues-api serves
(services/virtues-api/src/routes/maps.rs). Every file is a fixed square that
is identical for every box that takes it: nothing here depends on who asks.

The planet is downloaded once and cut locally, rather than range-read per
square: one sequential read of a build is the use Protomaps' docs point to.

    cut.py                        full monthly run (systemd timer)
    cut.py --remote --limit 3     dry run against build.protomaps.com, a few squares
"""

import argparse
import concurrent.futures
import datetime
import hashlib
import json
import math
import os
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
from collections import Counter
from pathlib import Path

from pmtiles.reader import MmapSource, Reader

BUILDS = "https://build.protomaps.com"
ASSETS_REPO = "https://github.com/protomaps/basemaps-assets"
TIERS = [  # tier, zoom of one file's square, deepest zoom the file holds
    ("visited", 5, 13),
    ("home", 7, 15),
]
LICENSE = "Map data (c) OpenStreetMap contributors, ODbL 1.0. https://www.openstreetmap.org/copyright"
ATTRIBUTION = '<a href="https://www.openstreetmap.org/copyright" target="_blank">&copy; OpenStreetMap</a>'


def log(msg: str) -> None:
    print(f"{time.strftime('%H:%M:%S')} {msg}", flush=True)


def newest_build() -> str:
    """The newest daily build that exists (Protomaps keeps about a week)."""
    today = datetime.date.today()
    for back in range(8):
        day = (today - datetime.timedelta(days=back)).strftime("%Y%m%d")
        # curl, not urllib: the same tool does the download, and a Python
        # without a CA bundle fails every HEAD in a way that reads as "no build".
        ok = subprocess.run(["curl", "-sfI", "--max-time", "30", f"{BUILDS}/{day}.pmtiles"], capture_output=True)
        if ok.returncode == 0:
            return day
    sys.exit("maps-cut: no Protomaps build found in the last week")


def bbox(z: int, x: int, y: int) -> str:
    """The tile's bounds as `w,s,e,n`, pulled a hair inside so the cut does
    not take the neighbours."""
    n = 2 ** z
    lon = lambda v: v / n * 360 - 180
    lat = lambda v: math.degrees(math.atan(math.sinh(math.pi * (1 - 2 * v / n))))
    e = 1e-6
    return f"{lon(x) + e},{lat(y + 1) + e},{lon(x + 1) - e},{lat(y) - e}"


def extract(pmtiles: str, src: str, out: Path, *args: str) -> None:
    if not any(a.startswith("--bbox=") for a in args) and out.name != "world.pmtiles":
        # Never a bbox-less cut of a square: that is the whole planet (138 GB).
        raise RuntimeError(f"refusing to cut {out.name} without a bbox")
    tmp = out.with_suffix(out.suffix + ".tmp")
    subprocess.run([pmtiles, "extract", src, str(tmp), *args, "-q"], check=True)
    tmp.rename(out)


def land_squares(world: Path, z: int) -> list[tuple[int, int]]:
    """The z-level tiles that hold anything but open sea.

    Protomaps stores identical tiles once, and open ocean is the one tile that
    repeats thousands of times at every zoom. So the most common tile at a
    zoom is the sea; every tile that differs from it holds land or coast.
    """
    with open(world, "rb") as f:
        reader = Reader(MmapSource(f))
        tiles = {}
        for x in range(2 ** z):
            for y in range(2 ** z):
                data = reader.get(z, x, y)
                if data:
                    tiles[(x, y)] = hashlib.sha256(data).hexdigest()
    sea, _ = Counter(tiles.values()).most_common(1)[0]
    return sorted(sq for sq, h in tiles.items() if h != sea)


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def assets_tar(out: Path) -> None:
    with tempfile.TemporaryDirectory() as tmp:
        subprocess.run(["git", "clone", "-q", "--depth", "1", "--filter=blob:none", "--sparse", ASSETS_REPO, tmp], check=True)
        subprocess.run(["git", "-C", tmp, "sparse-checkout", "set", "fonts", "sprites/v4"], check=True)
        with tarfile.open(out, "w") as tar:
            tar.add(Path(tmp) / "fonts", arcname="fonts")
            tar.add(Path(tmp) / "sprites/v4", arcname="sprites")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--maps-dir", default=os.environ.get("VIRTUES_MAPS_DIR", "/srv/maps"))
    ap.add_argument("--build", help="Protomaps build date YYYYMMDD (default: newest)")
    ap.add_argument("--remote", action="store_true", help="range-read the build instead of downloading it (dry runs only)")
    ap.add_argument("--limit", type=int, help="cut at most N squares per tier (dry runs only)")
    ap.add_argument("--jobs", type=int, default=6)
    ap.add_argument("--keep", type=int, default=2, help="builds to keep, the newest included")
    args = ap.parse_args()

    pmtiles = shutil.which("pmtiles") or shutil.which("go-pmtiles")
    if not pmtiles:
        sys.exit("maps-cut: install the Protomaps CLI (go install github.com/protomaps/go-pmtiles@latest)")

    maps_dir = Path(args.maps_dir)
    build = args.build or newest_build()
    final = maps_dir / build
    if final.exists():
        log(f"build {build} is already cut; nothing to do")
        return
    work = maps_dir / f"{build}.partial"
    work.mkdir(parents=True, exist_ok=True)
    log(f"∴ cutting build {build} into {final}")

    src = f"{BUILDS}/{build}.pmtiles"
    planet = maps_dir / f"planet-{build}.pmtiles"
    if not args.remote:
        if not planet.exists():
            log("downloading the planet (~140 GB, resumable)")
            subprocess.run(["curl", "-fsSL", "--retry", "5", "-C", "-", "-o", f"{planet}.part", src], check=True)
            Path(f"{planet}.part").rename(planet)
        src = str(planet)

    files = []
    world = work / "world.pmtiles"
    if not world.exists():
        extract(pmtiles, src, world, "--maxzoom=7")
    files.append({"name": world.name, "tier": "world", "z": 0, "x": 0, "y": 0})
    log("✓ world.pmtiles")

    jobs = []
    for tier, sz, maxz in TIERS:
        squares = land_squares(world, sz)
        log(f"{tier}: {len(squares)} land squares at z{sz}")
        if args.limit:
            squares = squares[: args.limit]
        for x, y in squares:
            name = f"{tier}-z{sz}-{x}-{y}.pmtiles"
            files.append({"name": name, "tier": tier, "z": sz, "x": x, "y": y})
            if not (work / name).exists():  # a rerun resumes where it stopped
                jobs.append((work / name, f"--bbox={bbox(sz, x, y)}", f"--maxzoom={maxz}"))

    done = 0
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
        futures = [pool.submit(extract, pmtiles, src, out, b, m) for out, b, m in jobs]
        for f in concurrent.futures.as_completed(futures):
            f.result()  # the first failure stops the run; a rerun resumes
            done += 1
            if done % 100 == 0 or done == len(jobs):
                log(f"  {done}/{len(jobs)} squares cut")

    assets = work / "assets.tar"
    if not assets.exists():
        assets_tar(assets)
    files.append({"name": assets.name, "tier": "assets", "z": 0, "x": 0, "y": 0})

    for f in files:
        p = work / f["name"]
        f["bytes"] = p.stat().st_size
        f["sha256"] = sha256(p)
    index = {"build": build, "license": LICENSE, "attribution": ATTRIBUTION, "files": files}
    (work / "index.json").write_text(json.dumps(index, indent=1))
    total = sum(f["bytes"] for f in files)
    log(f"✓ index.json: {len(files)} files, {total / 1e9:.1f} GB")

    # Publish atomically: the build directory appears whole, then latest.json
    # is swapped in one rename, so virtues-api never serves half a build.
    work.rename(final)
    latest = maps_dir / "latest.json.tmp"
    latest.write_text(json.dumps({"build": build}))
    latest.rename(maps_dir / "latest.json")
    log(f"∴ published build {build}")

    if planet.exists():
        planet.unlink()
    builds = sorted(p for p in maps_dir.iterdir() if p.is_dir() and p.name.isdigit() and len(p.name) == 8)
    for old in builds[: -args.keep]:
        shutil.rmtree(old)
        log(f"removed old build {old.name}")


if __name__ == "__main__":
    main()
