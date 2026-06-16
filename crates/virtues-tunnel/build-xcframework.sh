#!/usr/bin/env bash
# Build VirtuesTunnel.xcframework + the generated Swift bindings for the iOS app.
#
# Produces, under crates/virtues-tunnel/generated/:
#   VirtuesTunnel.xcframework   — device (arm64) + simulator (arm64 + x86_64)
#   virtues_tunnel.swift        — the uniffi-generated Swift API
#
# Requirements (macOS): Xcode, and the iOS Rust targets:
#   rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
#
# This must run on macOS (xcodebuild). It is the verification step the headless
# Linux CI can't do; the iOS app's CI lane runs it before building the app.
set -euo pipefail

CRATE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$CRATE_DIR/../.." && pwd)"
OUT="$CRATE_DIR/generated"
LIB=libvirtues_tunnel.a
NAME=VirtuesTunnel

DEVICE=aarch64-apple-ios
SIM_ARM=aarch64-apple-ios-sim
SIM_X86=x86_64-apple-ios   # used as the x86_64 simulator slice

echo "==> Building staticlib for iOS targets"
for t in "$DEVICE" "$SIM_ARM" "$SIM_X86"; do
  ( cd "$ROOT" && cargo build --release -p virtues-tunnel --target "$t" )
done

rm -rf "$OUT" && mkdir -p "$OUT"

echo "==> Generating Swift bindings"
( cd "$ROOT" && cargo run --features bindgen --bin uniffi-bindgen -- generate \
    --library "target/$DEVICE/release/$LIB" \
    --language swift \
    --out-dir "$OUT" )

# uniffi emits: virtues_tunnel.swift, virtues_tunnelFFI.h, virtues_tunnelFFI.modulemap
MODMAP="$OUT/virtues_tunnelFFI.modulemap"
HEADERS="$OUT/headers"
mkdir -p "$HEADERS"
cp "$OUT/virtues_tunnelFFI.h" "$HEADERS/"
# An XCFramework needs the modulemap named module.modulemap in the headers dir.
cp "$MODMAP" "$HEADERS/module.modulemap"

echo "==> Fattening simulator slices (arm64 + x86_64)"
SIM_FAT="$OUT/sim/$LIB"
mkdir -p "$OUT/sim"
lipo -create \
  "$ROOT/target/$SIM_ARM/release/$LIB" \
  "$ROOT/target/$SIM_X86/release/$LIB" \
  -output "$SIM_FAT"

echo "==> Assembling $NAME.xcframework"
xcodebuild -create-xcframework \
  -library "$ROOT/target/$DEVICE/release/$LIB" -headers "$HEADERS" \
  -library "$SIM_FAT" -headers "$HEADERS" \
  -output "$OUT/$NAME.xcframework"

echo "==> Done:"
echo "    $OUT/$NAME.xcframework"
echo "    $OUT/virtues_tunnel.swift   (add to the Xcode target)"
