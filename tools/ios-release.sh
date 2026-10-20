#!/usr/bin/env bash
# Build a signed iOS IPA for TestFlight.
#
# Produces:  apps/web/src-tauri/gen/apple/build/arm64/Virtues.ipa
# Upload:    drag that file into Transporter. This script never uploads: a
#            local build publishes under your Apple ID. The `ios-v*` tag
#            workflow (.github/workflows/release-ios.yml) runs this same script
#            signed with the team's App Store Connect API key, then uploads.
#
# This exists because the path was tribal knowledge and went two weeks stale
# unnoticed (1.2.2 shipped 2026-07-21; the battery work merged 07-29 and never
# left the Mac). Every step below is here because skipping it cost a build:
#
#   1. Flatten the app icons. App Store validation rejects an alpha channel on
#      the large app icon EVEN WHEN FULLY OPAQUE. Ours measured alpha 254-255
#      and still failed upload with a 409. `tauri icon` reintroduces RGBA on
#      every run, so this must happen after any icon regeneration and before
#      the archive.
#   2. Verify the ARCHIVE, not the sources. The 409 above was invisible in the
#      source PNGs' own check; what matters is the compiled Assets.car.
#   3. iOS must override `resources` to [] (apps/web/src-tauri/tauri.ios.conf.json).
#      The base tauri.conf.json ships the compiled *macOS* icon set (Assets.car
#      + AppIcon.icns) as bundle resources, and platform configs MERGE over the
#      base — so without that override Tauri copies a macOS asset catalog into
#      the iOS app at assets/, and App Store rejects the upload with
#        90562: Invalid Bundle. One of the nested bundles is built for a
#        platform which is different from the main bundle platform.
#      Latent from 2026-08-13 (the icons landing) until it bit 1.2.8 on 08-27.
#      It must be an ARRAY, not `{}`: Tauri DEEP-MERGES objects, so an empty
#      map contributes nothing and the base's entries survive (tried on 1.2.9,
#      caught by the verifier). An array replaces wholesale — the same reason
#      tauri.linux/windows.conf.json override `icon` as arrays. JSON has no
#      comments and Tauri rejects comment keys, so that bare `resources: []`
#      looks like a stub — it is not. The archive verifier now fails on any
#      non-ios asset catalog or stray .icns, which is what caught both tries.
#
# Usage:
#   tools/ios-release.sh            # build at the current version
#   tools/ios-release.sh 1.2.6      # set the version first, then build
#
# Environment:
#   BUILD_NUMBER=<digits>   CFBundleVersion (default: a UTC timestamp)
#   IOS_UNSIGNED=1          build with --no-sign, for a dry run that has no
#                           signing identity; the IPA cannot be uploaded
#   APPLE_API_KEY, APPLE_API_ISSUER, APPLE_API_KEY_PATH
#                           sign with an App Store Connect API key instead of
#                           the Xcode account (read by Tauri; CI sets them)

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WEB="$REPO_ROOT/apps/web"
CONF="$WEB/src-tauri/tauri.ios.conf.json"
BUILD_DIR="$WEB/src-tauri/gen/apple/build"
IPA="$BUILD_DIR/arm64/Virtues.ipa"

say() { printf '∴ %s\n' "$*"; }
die() { printf '✖ %s\n' "$*" >&2; exit 1; }

command -v jq >/dev/null || die "jq required"
command -v python3 >/dev/null || die "python3 required (icon flattening)"

# ── 0. version and build number ─────────────────────────────────────────────
# App Store Connect rejects a repeat of a (version, build) pair, and within one
# version a build number must go up. Tauri derives CFBundleVersion from
# `version` unless `bundle.iOS.bundleVersion` is set, so it is set here, to a
# UTC timestamp: that rises whether the build runs here or in CI, which a CI
# run number would not, and a re-upload of a version needs no bump.
if [[ $# -ge 1 ]]; then
  NEW="$1"
  tmp="$(mktemp)"
  jq --arg v "$NEW" '.version = $v' "$CONF" > "$tmp" && mv "$tmp" "$CONF"
  say "version set to $NEW"
fi
VERSION="$(jq -r .version "$CONF")"
BUILD_NUMBER="${BUILD_NUMBER:-$(date -u +%Y%m%d%H%M)}"
[[ "$BUILD_NUMBER" =~ ^[0-9]+$ ]] || die "BUILD_NUMBER must be digits, got '$BUILD_NUMBER'"
say "building iOS $VERSION ($BUILD_NUMBER)"

# ── 1. flatten app icons ────────────────────────────────────────────────────
say "flattening app icons to RGB"
python3 "$REPO_ROOT/tools/strip-icon-alpha.py"

# ── 2. archive + export ─────────────────────────────────────────────────────
# `beforeBuildCommand` in tauri.ios.conf.json bakes the SPA into
# apps/web/build-ios/ (scripts/bake-desktop-ui.sh --out build-ios), whose
# .virtues-bundle.json — the same manifest shape the box serves for OTA — is
# baked into the binary as its own `version`.
#
# STAMP IT, or the binary bakes a SPA calling itself `dev`. That is not
# cosmetic: `web_bundle::version_gate` refuses any bundle it cannot prove is
# forward, and `dev` orders against nothing — a TestFlight build with an
# unstamped SPA takes no OTA update, ever. (It also reported `version=dev` in
# the X-Virtues-Client header, so every TestFlight phone showed as `dev` on the
# Devices page.)
#
# The NEAREST TAG, not `git describe`'s full form. `--abbrev=0` deliberately
# drops the `-40-g6b999a11` offset: git puts that in the semver PRERELEASE
# field, where an alphanumeric identifier outranks a numeric one — so
# `0.1.7-staging.78-40-gabc` sorts ABOVE `0.1.7-staging.79`, and a phone stamped
# that way would refuse every later staging bundle the box ever serves. The
# nearest clean tag understates a build that is ahead of it, which is the
# harmless direction: same tag compares equal and the content hash decides.
#
# `v[0-9]*` because this repo publishes several tag lines into one namespace
# (`mac-v*`, `win-edge`, `edge`) and only the box/SPA line is orderable here.
#
# AN OPERATOR-SET GIT_DESCRIBE WINS — deriving over the top of it would hand
# back a build that is not the one asked for, silently. The physical-device
# recipe stamps `v<box-tag>.local1` on purpose: one prerelease identifier above
# the version the box serves, so `drop_stale_overlay` evicts the box's overlay
# at startup and local UI changes are actually visible on the phone. The nearest
# tag alone compares EQUAL to the box's and loses to the overlay already applied.
if [[ -n "${GIT_DESCRIBE:-}" ]]; then
  say "SPA stamped $GIT_DESCRIBE (preset — not derived)"
else
  GIT_DESCRIBE="$(git -C "$REPO_ROOT" describe --tags --match 'v[0-9]*' --abbrev=0 2>/dev/null || echo dev)"
  if [[ "$GIT_DESCRIBE" == dev ]]; then
    printf '⚠ %s\n' "no v* tag reachable — the SPA stamps as 'dev', and a 'dev' bake takes no OTA update and cannot evict an overlay already on the device"
  else
    say "SPA stamped $GIT_DESCRIBE (nearest tag)"
  fi
fi
GIT_COMMIT="${GIT_COMMIT:-$(git -C "$REPO_ROOT" rev-parse HEAD 2>/dev/null || echo dev)}"
export GIT_DESCRIBE GIT_COMMIT

TAURI_ARGS=(--export-method app-store-connect
  --config "{\"bundle\":{\"iOS\":{\"bundleVersion\":\"$BUILD_NUMBER\"}}}")
VERIFY_ARGS=(--build "$BUILD_NUMBER")
if [[ -n "${IOS_UNSIGNED:-}" ]]; then
  TAURI_ARGS+=(--no-sign)
  VERIFY_ARGS+=(--unsigned)
  say "unsigned: a dry run, not uploadable"
fi

say "archiving (this is the long one)"
( cd "$WEB" && pnpm tauri ios build "${TAURI_ARGS[@]}" )

[[ -f "$IPA" ]] || die "no IPA at $IPA — check the build output above"

# ── 3. verify the artifact, not the inputs ──────────────────────────────────
say "verifying the archive"

# Version, icon opacity in the COMPILED catalog, and signing — all read off
# the archive rather than the sources that produced it, which is the only
# reading that predicts what App Store validation will say.
# A FAILED verification must not leave an uploadable artifact behind. The IPA
# is written before this runs, at a path the operator has already been told to
# drag into Transporter — so on 2026-08-27 a rejected build's IPA sat there
# through the next attempt and got uploaded, burning a version on a bug that
# was already fixed. Delete it: nothing to drag is the only safe failure.
if ! python3 "$REPO_ROOT/tools/verify-ios-archive.py" "$BUILD_DIR" "$VERSION" "${VERIFY_ARGS[@]}"; then
  rm -f "$IPA"
  die "verification failed — IPA deleted so it cannot be uploaded by mistake"
fi
size="$(du -h "$IPA" | cut -f1)"
printf '\n'
say "$VERSION ($BUILD_NUMBER) ready — $IPA ($size)"
if [[ -z "${IOS_UNSIGNED:-}" && -z "${CI:-}" ]]; then
  printf '  Upload: open Transporter and drag the IPA in.\n'
fi
