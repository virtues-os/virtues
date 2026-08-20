#!/usr/bin/env bash
#
# build-mac-app.sh — build the one-DMG macOS app: Virtues.app bundling both
# helper sidecars (virtues-client, virtues-collector), signed + notarized.
#
# Produces:  <cargo target dir>/release/bundle/dmg/Virtues_*.dmg
#            (the target dir is whatever `.cargo/config.toml` says — printed at
#            the end of the run; it is NOT ./target in this repo)
#
# Steps:
#   1. cargo build -p virtues-client --release   -> sidecar
#   2. swift build -c release (apps/mac-source)  -> sidecar
#   3. tauri build (signs + notarizes via env)
#
# Signing + notarization are driven by environment variables so no Developer ID
# identity or Apple credentials live in the repo. Set these for a release build:
#
#   APPLE_SIGNING_IDENTITY   "Developer ID Application: Your Name (TEAMID)"
#   APPLE_TEAM_ID            your 10-char team id
#   # then EITHER an app-specific password:
#   APPLE_ID                 your-apple-id@example.com
#   APPLE_PASSWORD           app-specific password (appleid.apple.com)
#   # OR an App Store Connect API key:
#   APPLE_API_ISSUER, APPLE_API_KEY, APPLE_API_KEY_PATH
#
# Without APPLE_SIGNING_IDENTITY the script still builds an UNSIGNED .app/.dmg
# (fine for local testing; Gatekeeper will warn — right-click > Open).
#
# Usage:  tools/build-mac-app.sh [--target <triple>]   (default: host triple)

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TAURI_DIR="$REPO_ROOT/apps/web/src-tauri"
BINARIES_DIR="$TAURI_DIR/binaries"
SWIFT_DIR="$REPO_ROOT/apps/mac-source"

# Where cargo actually writes. NOT `$REPO_ROOT/target`: `.cargo/config.toml`
# points `build.target-dir` at the shared cache (~/.cargo/shared-target), so
# that path has not existed since it landed — this script assumed it and died
# on the sidecar's existence check. Ask cargo instead of guessing, so the
# answer stays right whoever sets the dir and however.
target_dir_for() {
  cargo metadata --no-deps --format-version 1 --manifest-path "$1" | jq -r .target_directory
}

TARGET="${1:-}"
if [[ "$TARGET" == "--target" ]]; then TARGET="${2:-}"; fi
if [[ -z "$TARGET" ]]; then
  TARGET="$(rustc -vV | awk '/^host:/{print $2}')"
fi
echo "→ target triple: $TARGET"

# ── 1. virtues-client (Rust) sidecar ────────────────────────────────────────
echo "→ building virtues-client (release)…"
cargo build -p virtues-client --release --manifest-path "$REPO_ROOT/Cargo.toml"
CLIENT_BIN="$(target_dir_for "$REPO_ROOT/Cargo.toml")/release/virtues-client"
[[ -x "$CLIENT_BIN" ]] || { echo "error: $CLIENT_BIN not found"; exit 1; }

# ── 2. virtues-collector (Swift) sidecar ────────────────────────────────────
# The collector links VirtuesIrohMac.xcframework (uniffi FFI) to reach the box
# over iroh. It's gitignored (a build artifact), so (re)generate it first —
# idempotent; safe on every build.
echo "→ building iroh FFI xcframework (macOS)…"
"$REPO_ROOT/crates/virtues-iroh-ffi/build-macos.sh"

echo "→ building virtues-collector (release)…"
( cd "$SWIFT_DIR" && swift build -c release )
COLLECTOR_BIN="$(cd "$SWIFT_DIR" && swift build -c release --show-bin-path)/virtues-collector"
[[ -x "$COLLECTOR_BIN" ]] || { echo "error: $COLLECTOR_BIN not found"; exit 1; }

# ── stage sidecars with the target-triple suffix tauri expects ──────────────
mkdir -p "$BINARIES_DIR"
cp "$CLIENT_BIN"    "$BINARIES_DIR/virtues-client-$TARGET"
cp "$COLLECTOR_BIN" "$BINARIES_DIR/virtues-collector-$TARGET"
chmod +x "$BINARIES_DIR/virtues-client-$TARGET" "$BINARIES_DIR/virtues-collector-$TARGET"
echo "✓ staged sidecars in $BINARIES_DIR"

# ── 3. tauri build (sign + notarize via env) ────────────────────────────────
TAURI_BIN="$REPO_ROOT/apps/web/node_modules/.bin/tauri"
[[ -x "$TAURI_BIN" ]] || { echo "error: tauri CLI not found — run pnpm install in apps/web"; exit 1; }

if [[ -n "${APPLE_SIGNING_IDENTITY:-}" ]]; then
  echo "→ signed build as: $APPLE_SIGNING_IDENTITY"
else
  echo "⚠ APPLE_SIGNING_IDENTITY unset — building UNSIGNED (Gatekeeper will warn)"
fi

( cd "$REPO_ROOT/apps/web" && "$TAURI_BIN" build )

echo ""
echo "✓ done. DMG:"
BUNDLE_DIR="$(target_dir_for "$TAURI_DIR/Cargo.toml")/release/bundle"
ls -1 "$BUNDLE_DIR/dmg/"*.dmg 2>/dev/null || \
  echo "  (no dmg — check the tauri build output above)"
echo "  app: $BUNDLE_DIR/macos/Virtues.app"
