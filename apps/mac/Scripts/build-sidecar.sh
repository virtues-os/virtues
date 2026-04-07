#!/bin/bash
set -euo pipefail

# Build the virtues-collector Swift package for both architectures
# and produce a universal binary for Tauri sidecar bundling.
#
# Usage: ./build-sidecar.sh [output-dir]
# Default output-dir: apps/web/src-tauri/binaries

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
MAC_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$MAC_DIR/../.." && pwd)"
OUTPUT_DIR="${1:-$REPO_ROOT/apps/web/src-tauri/binaries}"

PRODUCT_NAME="virtues-collector"
UNIVERSAL_NAME="${PRODUCT_NAME}-universal-apple-darwin"

echo "==> Building $PRODUCT_NAME for arm64..."
cd "$MAC_DIR"
swift build -c release --arch arm64

echo "==> Building $PRODUCT_NAME for x86_64..."
swift build -c release --arch x86_64

ARM64_BIN="$MAC_DIR/.build/arm64-apple-macosx/release/$PRODUCT_NAME"
X86_64_BIN="$MAC_DIR/.build/x86_64-apple-macosx/release/$PRODUCT_NAME"

if [ ! -f "$ARM64_BIN" ]; then
    echo "ERROR: arm64 binary not found at $ARM64_BIN"
    exit 1
fi

if [ ! -f "$X86_64_BIN" ]; then
    echo "ERROR: x86_64 binary not found at $X86_64_BIN"
    exit 1
fi

echo "==> Creating universal binary..."
mkdir -p "$OUTPUT_DIR"
lipo -create "$ARM64_BIN" "$X86_64_BIN" -output "$OUTPUT_DIR/$UNIVERSAL_NAME"
chmod +x "$OUTPUT_DIR/$UNIVERSAL_NAME"

echo "==> Verifying universal binary..."
lipo -info "$OUTPUT_DIR/$UNIVERSAL_NAME"
ls -lh "$OUTPUT_DIR/$UNIVERSAL_NAME"

echo "==> Done. Universal sidecar at: $OUTPUT_DIR/$UNIVERSAL_NAME"
