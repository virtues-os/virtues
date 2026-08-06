#!/usr/bin/env bash
# Build virtues-iroh-ffi for macOS and assemble VirtuesIrohMac.xcframework + the
# generated Swift bindings, consumed by the Mac collector (apps/mac-source) so it
# reaches the box over iroh (no bearer — auth is the device's allowlisted key).
#
# Produces under crates/virtues-iroh-ffi/generated/:
#   - VirtuesIrohMac.xcframework   (macos-arm64[ + x86_64] slice)
#   - VirtuesIroh.swift            (uniffi-generated Swift wrapper; same as iOS)
#
# Requires: rustup target aarch64-apple-darwin (and optionally x86_64), xcodebuild.
# Idempotent; safe to re-run.
set -euo pipefail

CRATE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$CRATE_DIR/../.." && pwd)"
# Where cargo actually writes. NOT `$WORKSPACE_ROOT/target`: `.cargo/config.toml`
# redirects `build.target-dir` to the shared cache, so that path stopped
# existing and every per-target lookup below missed. Ask cargo.
TARGET_DIR="$(cargo metadata --no-deps --format-version 1 \
  --manifest-path "$WORKSPACE_ROOT/Cargo.toml" | jq -r .target_directory)"
OUT="$CRATE_DIR/generated"
LIB=libvirtues_iroh_ffi.a
NAME=VirtuesIrohMac

ARM=aarch64-apple-darwin
X86=x86_64-apple-darwin

export MACOSX_DEPLOYMENT_TARGET=12.0

echo "==> building staticlib for macOS (release)"
cargo build -p virtues-iroh-ffi --release --target "$ARM"

MAC_DIR="$CRATE_DIR/build/mac"
mkdir -p "$MAC_DIR"
# Universal if the x86_64 target is installed + builds; otherwise arm64-only
# (fine for Apple Silicon Macs — the common case).
if rustup target list --installed 2>/dev/null | grep -q "$X86" \
   && cargo build -p virtues-iroh-ffi --release --target "$X86" 2>/dev/null; then
  echo "==> lipo macOS slices into one fat archive"
  lipo -create \
    "$TARGET_DIR/$ARM/release/$LIB" \
    "$TARGET_DIR/$X86/release/$LIB" \
    -output "$MAC_DIR/$LIB"
else
  echo "==> x86_64 target unavailable — arm64-only archive"
  cp "$TARGET_DIR/$ARM/release/$LIB" "$MAC_DIR/$LIB"
fi

echo "==> generating Swift bindings"
BIND="$CRATE_DIR/build/swift-mac"
rm -rf "$BIND" && mkdir -p "$BIND"
cargo run -p virtues-iroh-ffi --bin uniffi-bindgen -- \
  generate --library "$TARGET_DIR/$ARM/release/$LIB" \
  --language swift --out-dir "$BIND"

echo "==> staging headers"
HDR="$CRATE_DIR/build/headers-mac"
rm -rf "$HDR" && mkdir -p "$HDR"
cp "$BIND"/*FFI.h "$HDR/"
cp "$BIND"/*.modulemap "$HDR/module.modulemap"

echo "==> assembling $NAME.xcframework"
rm -rf "$OUT/$NAME.xcframework"
mkdir -p "$OUT"
xcodebuild -create-xcframework \
  -library "$MAC_DIR/$LIB" -headers "$HDR" \
  -output "$OUT/$NAME.xcframework"

echo "==> copying generated Swift wrapper into the Mac collector"
APP_SWIFT="$WORKSPACE_ROOT/apps/mac-source/Sources/Core/VirtuesIroh.swift"
cp "$BIND"/*.swift "$APP_SWIFT"
echo "    $APP_SWIFT"

echo "==> done: $OUT/$NAME.xcframework"
