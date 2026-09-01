#!/usr/bin/env bash
# Build a signed iOS IPA for TestFlight.
#
# Produces:  apps/web/src-tauri/gen/apple/build/arm64/Virtues.ipa
# Upload:    drag that file into Transporter. Deliberately NOT automated —
#            it publishes under your Apple ID, so it stays a human action.
#
# This exists because the path was tribal knowledge and went two weeks stale
# unnoticed (1.2.2 shipped 2026-07-21; the battery work merged 07-29 and never
# left the Mac). Every step below is here because skipping it cost a build:
#
#   1. FFI framework FIRST. `tauri ios build` does not build VirtuesIroh —
#      a stale xcframework links silently against old symbols.
#   2. Flatten the app icons. App Store validation rejects an alpha channel on
#      the large app icon EVEN WHEN FULLY OPAQUE. Ours measured alpha 254-255
#      and still failed upload with a 409. `tauri icon` reintroduces RGBA on
#      every run, so this must happen after any icon regeneration and before
#      the archive.
#   3. Verify the ARCHIVE, not the sources. The 409 above was invisible in the
#      source PNGs' own check; what matters is the compiled Assets.car.
#   4. iOS must override `resources` to [] (apps/web/src-tauri/tauri.ios.conf.json).
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

# ── 0. version ──────────────────────────────────────────────────────────────
# App Store Connect rejects a repeat of a (version, build) pair, and Tauri
# writes both fields from this one value — so a rebuild at the same version
# cannot be uploaded, however good it is.
if [[ $# -ge 1 ]]; then
  NEW="$1"
  tmp="$(mktemp)"
  jq --arg v "$NEW" '.version = $v' "$CONF" > "$tmp" && mv "$tmp" "$CONF"
  say "version set to $NEW"
fi
VERSION="$(jq -r .version "$CONF")"
say "building iOS $VERSION"

# ── 1. iroh FFI framework ───────────────────────────────────────────────────
say "building VirtuesIroh.xcframework"
"$REPO_ROOT/crates/virtues-iroh-ffi/build-ios.sh"

# ── 2. flatten app icons ────────────────────────────────────────────────────
say "flattening app icons to RGB"
python3 "$REPO_ROOT/tools/strip-icon-alpha.py"

# ── 3. archive + export ─────────────────────────────────────────────────────
# `beforeBuildCommand` in tauri.ios.conf.json runs `pnpm build`, which also
# stamps build/.virtues-bundle.json — the manifest the box serves for OTA.
say "archiving (this is the long one)"
( cd "$WEB" && pnpm tauri ios build --export-method app-store-connect )

[[ -f "$IPA" ]] || die "no IPA at $IPA — check the build output above"

# ── 4. verify the artifact, not the inputs ──────────────────────────────────
say "verifying the archive"

# Version, icon opacity in the COMPILED catalog, and signing — all read off
# the archive rather than the sources that produced it, which is the only
# reading that predicts what App Store validation will say.
# A FAILED verification must not leave an uploadable artifact behind. The IPA
# is written before this runs, at a path the operator has already been told to
# drag into Transporter — so on 2026-08-27 a rejected build's IPA sat there
# through the next attempt and got uploaded, burning a version on a bug that
# was already fixed. Delete it: nothing to drag is the only safe failure.
if ! python3 "$REPO_ROOT/tools/verify-ios-archive.py" "$BUILD_DIR" "$VERSION"; then
  rm -f "$IPA"
  die "verification failed — IPA deleted so it cannot be uploaded by mistake"
fi
size="$(du -h "$IPA" | cut -f1)"
printf '\n'
say "$VERSION ready — $IPA ($size)"
printf '  Upload: open Transporter and drag the IPA in.\n'
