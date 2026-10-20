#!/usr/bin/env python3
"""Verify a built iOS archive before it is uploaded to TestFlight.

Checks the ARCHIVE, not the sources that produced it. That distinction is the
whole point: on 2026-08-05 an upload failed validation with

    Invalid large app icon. The large app icon in the asset catalog in
    "Virtues.app" can't be transparent or contain an alpha channel. (409)

while the source PNGs looked fine to a casual check. What App Store validation
reads is the compiled `Assets.car`, so that is what this reads.

Usage:
    tools/verify-ios-archive.py <build-dir> <expected-version> [--build N] [--unsigned]

`--build` also requires that CFBundleVersion. `--unsigned` is for a build made
with `tauri ios build --no-sign` (the workflow's dry run): it skips the checks
that need a distribution signature and keeps the rest.

Exits non-zero with a one-line reason on the first failure.
"""

import argparse
import json
import plistlib
import subprocess
import tempfile
import sys
from pathlib import Path


def fail(msg: str) -> "NoReturn":  # noqa: F821
    print(f"✖ {msg}", file=sys.stderr)
    raise SystemExit(1)


# The app target's name in the Xcode project, which names the archive and the
# app's entitlements file.
SCHEME = "virtues_iOS"

# Entitlements App Store validation or the OS will not take without a purpose
# string. A missing HealthKit string fails the upload with a 409 (both of them,
# though this app never writes to Health); a missing FinanceKit one SIGABRTs on
# the first FinanceKit call.
PURPOSE_STRINGS = {
    "com.apple.developer.healthkit": (
        "NSHealthShareUsageDescription",
        "NSHealthUpdateUsageDescription",
    ),
    "com.apple.developer.financekit": ("NSFinancialDataUsageDescription",),
}


def check_version(app: Path, expected: str, build: "str | None" = None) -> None:
    with (app / "Info.plist").open("rb") as fh:
        info = plistlib.load(fh)
    got = info.get("CFBundleShortVersionString")
    if got != expected:
        fail(f"archive says {got}, expected {expected}")
    got_build = info.get("CFBundleVersion")
    if build is not None and got_build != build:
        fail(f"archive is build {got_build}, expected {build}")
    print(f"  version {got} (build {got_build})")


def check_extensions(app: Path) -> None:
    """Every embedded extension carries the app's exact version.

    App Store validation rejects an extension whose CFBundleShortVersionString
    or CFBundleVersion differs from its app's. Tauri writes the version into
    the app's Info.plist only; the extension's is copied over by a build phase
    ("Stamp the app's version" in VirtuesControls), so this reads the result.
    """
    with (app / "Info.plist").open("rb") as fh:
        info = plistlib.load(fh)
    want = (info.get("CFBundleShortVersionString"), info.get("CFBundleVersion"))
    appexes = sorted((app / "PlugIns").glob("*.appex"))
    for appex in appexes:
        with (appex / "Info.plist").open("rb") as fh:
            ext = plistlib.load(fh)
        got = (ext.get("CFBundleShortVersionString"), ext.get("CFBundleVersion"))
        if got != want:
            fail(f"{appex.name} is version {got}, the app is {want}")
    print(f"  {len(appexes)} extension(s), versions match the app")


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
    """Every bundle in the export is distribution-signed, the app for TestFlight.

    Read per bundle because the summary lists the app and each extension
    separately. "Apple Distribution" is matched as a substring so a
    cloud-managed certificate (the CI path, which signs with an App Store
    Connect API key) passes under whatever label the summary gives it, while a
    development certificate still fails.
    """
    summary = build_dir / "DistributionSummary.plist"
    if not summary.is_file():
        fail(f"no DistributionSummary.plist in {build_dir}")
    with summary.open("rb") as fh:
        data = plistlib.load(fh)

    records = next(iter(data.values()), None)
    if not records:
        fail("DistributionSummary.plist has no entries")

    for rec in records:
        name = rec.get("name", "?")
        cert_type = rec.get("certificate", {}).get("type") or ""
        if "Apple Distribution" not in cert_type:
            fail(f"{name} signed with '{cert_type}', expected Apple Distribution")
        ents = rec.get("entitlements", {})
        if ents.get("get-task-allow") is not False:
            fail(f"{name}: get-task-allow is true — that is a debug build")
        if name.endswith(".app") and ents.get("beta-reports-active") is not True:
            fail(f"{name}: beta-reports-active is not set — this build cannot go to TestFlight")

    print(f"  {len(records)} bundle(s) signed Apple Distribution, TestFlight-eligible")


def source_entitlements(project: Path, name: str) -> dict:
    """The entitlements a bundle was declared with: `<name>/<name>.entitlements`.

    The same convention the app target's "Keep extension entitlements on an
    unsigned archive" phase uses to find an extension's file.
    """
    path = project / name / f"{name}.entitlements"
    if not path.is_file():
        fail(f"no entitlements file at {path} to check {name} against")
    with path.open("rb") as fh:
        return plistlib.load(fh)


def check_purpose_strings(app: Path, project: Path) -> None:
    with (app / "Info.plist").open("rb") as fh:
        info = plistlib.load(fh)
    declared = source_entitlements(project, SCHEME)
    keys = [
        key
        for ent, required in PURPOSE_STRINGS.items()
        if declared.get(ent)
        for key in required
    ]
    missing = [key for key in keys if not str(info.get(key) or "").strip()]
    if missing:
        fail(f"Info.plist lacks {', '.join(missing)}, which the declared entitlements require")
    print(f"  {len(keys)} purpose string(s) present for the declared entitlements")


def signed_entitlements(bundle: Path) -> dict:
    out = subprocess.run(
        ["codesign", "-d", "--entitlements", "-", "--xml", str(bundle)],
        capture_output=True,
    )
    if out.returncode != 0:
        fail(f"{bundle.name} is not signed: {out.stderr.decode().strip().splitlines()[-1:]}")
    return plistlib.loads(out.stdout) if out.stdout.strip() else {}


def check_entitlements(app: Path, project: Path) -> None:
    """Every bundle is signed with every entitlement its target declares.

    The CI path signs with an App Store Connect API key, where Tauri archives
    unsigned and `xcodebuild -exportArchive` keeps only the entitlements an
    existing signature carries. Tauri covers the app binary and the Xcode
    project covers the extensions, and a gap in either does not fail the build,
    the upload or TestFlight processing: the extension just loses its app group
    and misbehaves on the phone (tauri-apps/tauri#15663). This is the one
    place it can be seen before upload.
    """
    bundles = [(app, SCHEME)] + [
        (appex, appex.stem) for appex in sorted((app / "PlugIns").glob("*.appex"))
    ]
    for bundle, name in bundles:
        want = source_entitlements(project, name)
        got = signed_entitlements(bundle)
        wrong = sorted(key for key, value in want.items() if got.get(key) != value)
        if wrong:
            fail(f"{bundle.name} is signed without {', '.join(wrong)} from {name}.entitlements")
    print(f"  {len(bundles)} bundle(s) carry their declared entitlements")


def check_nested_platforms(app: Path) -> None:
    """No macOS artifact may ride along inside the iOS app.

    App Store validation rejects this as

        90562: Invalid Bundle. One of the nested bundles is built for a
        platform which is different from the main bundle platform.

    which happened on 2026-08-27: `tauri.conf.json` ships the compiled macOS
    icon set as bundle `resources`, platform configs MERGE over the base, and
    `tauri.ios.conf.json` did not override the key — so a macOS `Assets.car`
    landed in the iOS app at `assets/`. Every other check here passed it,
    because version, icon opacity and signing were all genuinely fine.

    Two reads, because the failure has two visible faces: any compiled asset
    catalog whose own Platform is not iOS, and any `.icns` at all (a macOS-only
    icon format — iOS carries its icons in the catalog Xcode compiles).
    """
    strays = sorted(app.rglob("*.icns"))
    if strays:
        names = ", ".join(str(p.relative_to(app)) for p in strays[:3])
        fail(f"macOS .icns inside the iOS app: {names}")

    catalogs = sorted(app.rglob("Assets.car"))
    for car in catalogs:
        out = subprocess.run(
            ["xcrun", "assetutil", "--info", str(car)],
            capture_output=True,
            text=True,
        )
        if out.returncode != 0:
            # Unreadable catalog is not this check's business to adjudicate.
            continue
        try:
            entries = json.loads(out.stdout)
        except json.JSONDecodeError:
            continue
        platforms = {
            e.get("Platform")
            for e in entries
            if isinstance(e, dict) and e.get("Platform")
        }
        bad = {p for p in platforms if p and p != "ios"}
        if bad:
            rel = car.relative_to(app)
            fail(f"{rel} is built for {sorted(bad)}, not ios (App Store 90562)")

    print(f"  {len(catalogs)} asset catalog(s), all ios, no .icns")


def main() -> int:
    parser = argparse.ArgumentParser(description="Verify a built iOS archive and its IPA.")
    parser.add_argument("build_dir", type=Path)
    parser.add_argument("expected_version")
    parser.add_argument("--build", help="the CFBundleVersion the build was given")
    parser.add_argument("--unsigned", action="store_true", help="built with --no-sign")
    args = parser.parse_args()
    build_dir = args.build_dir
    expected = args.expected_version
    # gen/apple, where the Xcode project and the entitlements files live.
    project = build_dir.parent

    app = build_dir / f"{SCHEME}.xcarchive/Products/Applications/Virtues.app"
    if not app.is_dir():
        fail(f"no archived app at {app}")

    check_version(app, expected, args.build)
    check_extensions(app)
    check_icons(app)
    check_nested_platforms(app)
    check_purpose_strings(app, project)
    if not args.unsigned:
        check_signing(build_dir)

    # And again on the IPA, because the IPA is what gets uploaded and it is
    # EXPORTED from the archive rather than being the archive. On 2026-08-27 a
    # run whose archive was clean still shipped a stale IPA, because a failed
    # verification had left the previous attempt's file sitting at this exact
    # path for Transporter to pick up. Checking the payload closes the gap
    # between "what we inspected" and "what Apple receives".
    ipa = build_dir / "arm64/Virtues.ipa"
    if ipa.is_file():
        with tempfile.TemporaryDirectory() as tmp:
            subprocess.run(
                ["unzip", "-q", str(ipa), "-d", tmp],
                check=True,
                capture_output=True,
            )
            payload = sorted(Path(tmp).glob("Payload/*.app"))
            if not payload:
                fail(f"{ipa.name} has no Payload/*.app")
            print("  — and the exported IPA:")
            check_version(payload[0], expected, args.build)
            check_extensions(payload[0])
            check_nested_platforms(payload[0])
            check_purpose_strings(payload[0], project)
            if not args.unsigned:
                check_entitlements(payload[0], project)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
