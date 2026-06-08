#!/usr/bin/env bash
# Fetch the embedder + reranker ONNX models from HuggingFace into a target
# directory. Run during image build so the box never needs to touch HF at
# runtime. Safe to re-run; uses curl with --location and skips files already
# present.
#
# Usage:
#   deploy/fetch-models.sh [TARGET_DIR] [PRECISION]
#
#   TARGET_DIR  defaults to /opt/virtues/models
#   PRECISION   int8 (default) | fp16
#
# PRECISION must match what virtues-core/src/search/accelerator.rs will resolve on the
# target hardware: the portable CPU image bakes `int8`; the Jetson/NVIDIA
# appliance image is built with PRECISION=fp16 (and core `--features cuda`).
# The resolver falls back to int8 if a baked fp16 file is absent, so baking the
# wrong one degrades rather than breaks.
#
# Layout: <TARGET_DIR>/<repo_basename>/<file>  — matches model_cache.rs lookup.
#
# `.onnx_data`: larger ONNX exports store weights in a sibling `.onnx_data`
# file that ORT discovers relative to the `.onnx` at load time. We fetch it
# best-effort (no --fail): models without external data simply don't have one.

set -euo pipefail

TARGET_DIR="${1:-/opt/virtues/models}"
PRECISION="${2:-${VIRTUES_BAKE_PRECISION:-int8}}"

case "$PRECISION" in
  int8)
    EMBEDDER_ONNX="onnx/model_quantized.onnx"
    RERANKER_ONNX="onnx/model_int8.onnx"
    ;;
  fp16)
    EMBEDDER_ONNX="onnx/model_fp16.onnx"
    RERANKER_ONNX="onnx/model_fp16.onnx"
    ;;
  *)
    echo "error: PRECISION must be int8 or fp16 (got '$PRECISION')" >&2
    exit 2
    ;;
esac

EMBEDDER_REPO="onnx-community/embeddinggemma-300m-ONNX"
RERANKER_REPO="jinaai/jina-reranker-v2-base-multilingual"

# fetch <repo> <file> <required|optional>
fetch_one() {
  local repo="$1" file="$2" mode="${3:-required}"
  local basename="${repo##*/}"
  local out="${TARGET_DIR}/${basename}/${file}"

  if [[ -s "$out" ]]; then
    echo "  skip (already present): ${basename}/${file}"
    return 0
  fi

  mkdir -p "$(dirname "$out")"
  local url="https://huggingface.co/${repo}/resolve/main/${file}?download=true"
  echo "  fetch (${mode}): $url"
  if [[ "$mode" == "optional" ]]; then
    # No --fail: a missing external-data sibling is normal, not an error.
    curl --location --silent --show-error --output "$out" "$url" || rm -f "$out"
    [[ -s "$out" ]] || echo "    (no ${file} — model has no external data, ok)"
  else
    curl --fail --location --silent --show-error --output "$out" "$url"
  fi
}

# fetch a model: required .onnx + tokenizer, best-effort .onnx_data sibling.
fetch_model() {
  local repo="$1" onnx="$2"
  echo "==> ${repo} (${onnx})"
  fetch_one "$repo" "$onnx" required
  fetch_one "$repo" "${onnx}_data" optional
  fetch_one "$repo" "tokenizer.json" required
}

echo "fetching models into ${TARGET_DIR} (precision=${PRECISION})"
fetch_model "$EMBEDDER_REPO" "$EMBEDDER_ONNX"
fetch_model "$RERANKER_REPO" "$RERANKER_ONNX"
echo "done"
