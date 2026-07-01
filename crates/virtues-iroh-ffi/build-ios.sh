#!/usr/bin/env bash
# Build virtues-iroh-ffi for iOS and assemble VirtuesIroh.xcframework + the
# generated Swift bindings, consumed by the iOS app (apps/ios).
#
# Produces under crates/virtues-iroh-ffi/generated/:
#   - VirtuesIroh.xcframework   (device slice + fat simulator slice)
#   - VirtuesIroh.swift         (uniffi-generated Swift wrapper)
#
# Requires: rustup iOS targets (aarch64-apple-ios, aarch64-apple-ios-sim,
# x86_64-apple-ios), xcodebuild (macOS). Idempotent; safe to re-run.
set -euo pipefail

CRATE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$CRATE_DIR/../.." && pwd)"
TARGET_DIR="$WORKSPACE_ROOT/target"
OUT="$CRATE_DIR/generated"
LIB=libvirtues_iroh_ffi.a
NAME=VirtuesIroh

DEVICE=aarch64-apple-ios
SIM_ARM=aarch64-apple-ios-sim
SIM_X86=x86_64-apple-ios

# Match the app's minimum so the linker doesn't warn about newer-versioned
# objects. Keep in sync with IPHONEOS_DEPLOYMENT_TARGET in the Xcode project.
export IPHONEOS_DEPLOYMENT_TARGET=18.0

echo "==> building staticlib for iOS targets (release)"
for t in "$DEVICE" "$SIM_ARM" "$SIM_X86"; do
  cargo build -p virtues-iroh-ffi --release --target "$t"
done

echo "==> lipo simulator slices into one fat archive"
SIM_DIR="$CRATE_DIR/build/sim"
mkdir -p "$SIM_DIR"
lipo -create \
  "$TARGET_DIR/$SIM_ARM/release/$LIB" \
  "$TARGET_DIR/$SIM_X86/release/$LIB" \
  -output "$SIM_DIR/$LIB"

echo "==> generating Swift bindings from the device library"
BIND="$CRATE_DIR/build/swift"
rm -rf "$BIND" && mkdir -p "$BIND"
cargo run -p virtues-iroh-ffi --bin uniffi-bindgen -- \
  generate --library "$TARGET_DIR/$DEVICE/release/$LIB" \
  --language swift --out-dir "$BIND"

# uniffi emits <mod>.swift, <mod>FFI.h, <mod>FFI.modulemap. An XCFramework needs
# a headers dir with the C header + a file literally named module.modulemap.
echo "==> staging headers"
HDR="$CRATE_DIR/build/headers"
rm -rf "$HDR" && mkdir -p "$HDR"
cp "$BIND"/*FFI.h "$HDR/"
# Normalise the modulemap name (uniffi names it *FFI.modulemap).
cp "$BIND"/*.modulemap "$HDR/module.modulemap"

echo "==> assembling $NAME.xcframework"
rm -rf "$OUT/$NAME.xcframework"
mkdir -p "$OUT"
xcodebuild -create-xcframework \
  -library "$TARGET_DIR/$DEVICE/release/$LIB" -headers "$HDR" \
  -library "$SIM_DIR/$LIB"                     -headers "$HDR" \
  -output "$OUT/$NAME.xcframework"

echo "==> copying generated Swift wrapper"
cp "$BIND"/*.swift "$OUT/$NAME.swift"
# Also drop it into the iOS app's synchronized source tree so Xcode compiles it
# (objectVersion-77 filesystem-synchronized groups auto-include files under it).
APP_SWIFT="$WORKSPACE_ROOT/apps/ios/Virtues/Managers/Tunnel/$NAME.swift"
if [ -d "$(dirname "$APP_SWIFT")" ]; then
  cp "$BIND"/*.swift "$APP_SWIFT"
  echo "    $APP_SWIFT"
fi

echo "==> done:"
echo "    $OUT/$NAME.xcframework"
echo "    $OUT/$NAME.swift"
