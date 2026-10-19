#!/usr/bin/env bash
# Publish the Dragon Q6A (Hexagon v68) QNN inference artifacts to the models
# release bucket — the context binaries + tokenizers the installer's `install_qnn`
# fetches (SHA256-verified, same mechanism as the GGUFs).
#
# The artifacts come from the lab Dragon (they're the exact, on-NPU-validated
# files — see crates/virtues-qnnd). This script pulls them, stages them under the
# installer's expected asset names with .sha256 sidecars, and uploads them to the
# models tag with `gh`. It is idempotent (`gh release upload --clobber`).
#
# Usage:  MODELS_TAG=models-1 SRC=radxa-box:/home/radxa/npu ./tools/publish-qnn-models.sh
#   SRC        scp source dir holding the .bins + tok_gte/ + tok_colbert/
#   MODELS_TAG GitHub release tag the installer's VIRTUES_MODELS_BASE points at
#   REPO       owner/repo (default virtues-os/virtues)
set -euo pipefail

SRC="${SRC:-radxa-box:/home/radxa/npu}"
MODELS_TAG="${MODELS_TAG:-models-1}"
REPO="${REPO:-virtues-os/virtues}"

# (asset-name  source-path-relative-to-SRC) — asset-name MUST match the installer
# (config.rs: qnn_embed_bin / qnn_rerank_bin / qnn_tokenizers).
ASSETS=(
  "gte_v68_vtcm2.bin        gte_v68_vtcm2.bin"
  "cb256_v68_vtcm2.bin      cb256_v68_vtcm2.bin"
  "tok_gte-tokenizer.json   tok_gte/tokenizer.json"
  "tok_colbert-tokenizer.json tok_colbert/tokenizer.json"
)

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
echo "→ staging QNN artifacts from $SRC"
for row in "${ASSETS[@]}"; do
  read -r asset rel <<<"$row"
  scp -q "$SRC/$rel" "$work/$asset"
  ( cd "$work" && sha256sum "$asset" > "$asset.sha256" )
  printf "  %-28s %10s bytes\n" "$asset" "$(stat -c%s "$work/$asset" 2>/dev/null || stat -f%z "$work/$asset")"
done

echo "→ uploading to $REPO release '$MODELS_TAG'"
gh release view "$MODELS_TAG" --repo "$REPO" >/dev/null 2>&1 \
  || gh release create "$MODELS_TAG" --repo "$REPO" --prerelease \
       --title "$MODELS_TAG" --notes "Model + inference artifacts (not tied to code releases)."
for row in "${ASSETS[@]}"; do
  read -r asset _ <<<"$row"
  gh release upload "$MODELS_TAG" --repo "$REPO" --clobber \
    "$work/$asset" "$work/$asset.sha256"
done
echo "✓ published: ${#ASSETS[@]} QNN artifacts (+ .sha256) to $MODELS_TAG"
