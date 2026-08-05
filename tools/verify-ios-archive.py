#!/usr/bin/env python3
"""Verify a built iOS archive before it is uploaded to TestFlight.

Checks the ARCHIVE, not the sources that produced it. That distinction is the
whole point: on 2026-08-05 an upload failed validation with

    Invalid large app icon. The large app icon in the asset catalog in
    "Virtues.app" can't be transparent or contain an alpha channel. (409)

while the source PNGs looked fine to a casual check. What App Store validation
reads is the compiled `Assets.car`, so that is what this reads.

Usage:
    tools/verify-ios-archive.py <build-dir> <expected-version>

Exits non-zero with a one-line reason on the first failure.
"""

import json
import plistlib
import subprocess
import sys
from pathlib import Path


def fail(msg: str) -> "NoReturn":  # noqa: F821
    print(f"✖ {msg}", file=sys.stderr)
    raise SystemExit(1)


def check_version(app: Path, expected: str) -> None:
    with (app / "Info.plist").open("rb") as fh:
        info = plistlib.load(fh)
    got = info.get("CFBundleShortVersionString")
    if got != expected:
        fail(f"archive says {got}, expected {expected}")
    build = info.get("CFBundleVersion")
    print(f"  version {got} (build {build})")


def check_icons(app: Path) -> None:
    """Every app icon carrying pixels must be opaque.

    App Store validation rejects an alpha *channel* on the app icon even when
    every pixel is fully opaque — ours measured alpha 254-255 throughout and
    still 409'd, so "it looks opaque" is not a defence.
    """
    car = app / "Assets.car"
    if not car.is_file():
        fail(f"no Assets.car in {app}")

    out = subprocess.run(
        ["xcrun", "assetutil", "--info", str(car)],
        capture_output=True,
        text=True,
    )
    if out.returncode != 0:
        fail(f"assetutil failed: {out.stderr.strip().splitlines()[:1]}")

    entries = json.loads(out.stdout)
    named = [e for e in entries if isinstance(e, dict) and "AppIcon" in str(e.get("Name", ""))]

    # Only entries carrying pixels can be opaque or not. The catalog also holds
    # "MultiSized Image" index records per idiom — metadata pointing at the real
    # images, with no PixelWidth and no Opaque field. Counting those as failures
    # rejects a good archive: the 1.2.5 build that uploaded cleanly has four.
    images = [e for e in named if e.get("PixelWidth")]
    if not images:
        fail("no AppIcon image entries in Assets.car")

    bad = [e for e in images if e.get("Opaque") is not True]
    if bad:
        sizes = ", ".join(f"{e.get('PixelWidth')}x{e.get('PixelHeight')}" for e in bad[:4])
        fail(f"{len(bad)} app icon(s) carry alpha ({sizes}) — upload will 409. "
             f"Run tools/strip-icon-alpha.py and rebuild.")

    print(f"  {len(images)} icon images, all opaque")


def check_signing(build_dir: Path) -> None:
    summary = build_dir / "DistributionSummary.plist"
    if not summary.is_file():
        fail(f"no DistributionSummary.plist in {build_dir}")
    with summary.open("rb") as fh:
        data = plistlib.load(fh)

    entry = next(iter(data.values()), None)
    if not entry:
        fail("DistributionSummary.plist has no entries")
    rec = entry[0]

    cert_type = rec.get("certificate", {}).get("type")
    if cert_type != "Apple Distribution":
        fail(f"signed with '{cert_type}', expected Apple Distribution")

    ents = rec.get("entitlements", {})
    if ents.get("beta-reports-active") is not True:
        fail("beta-reports-active is not set — this build cannot go to TestFlight")
    if ents.get("get-task-allow") is not False:
        fail("get-task-allow is true — that is a debug build")

    print(f"  signed {cert_type}, TestFlight-eligible")


def main() -> int:
    if len(sys.argv) != 3:
        fail("usage: verify-ios-archive.py <build-dir> <expected-version>")
    build_dir = Path(sys.argv[1])
    expected = sys.argv[2]

    app = build_dir / "virtues_iOS.xcarchive/Products/Applications/Virtues.app"
    if not app.is_dir():
        fail(f"no archived app at {app}")

    check_version(app, expected)
    check_icons(app)
    check_signing(build_dir)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
