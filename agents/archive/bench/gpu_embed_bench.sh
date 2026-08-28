#!/usr/bin/env bash
# Virtues embed bench orchestrator (see agents/archive/inference-bench-spec.md).
#
# CONTEXT — this is a CONFIRM/REFUTE run, not a fishing trip. A prior on-Orin
# session (2026-06-26) already found EmbeddingGemma embed is SLOWER on the Orin
# GPU than on CPU (CPU 19.5s vs GPU 23.96s for one encyclical), because the
# model's activations can't compute in fp16 → llama.cpp's CUDA path forces fp32
# and blows up the compute buffer. TensorRT hit the same fp32-forcing, so it's a
# model property, not a llama.cpp bug. Decision shipped: embed→CPU, rerank→GPU.
#
# This script re-tests ONLY the two things that prior run did not isolate:
#   (a) a TRUE bf16 GGUF (not fp16) — does bf16 dodge the fp32 buffer blowup?
#   (b) the BATCHED bulk-PDF regime (-ub 8192, many passages/dispatch) vs the
#       prior single-doc latency number — does batching let the GPU pull ahead?
# Base-rate expectation: GPU still loses (fp32-forcing is a model property). If
# either bf16-gpu or the batched throughput FLIPS that, it's new information.
#
# Runs ONE llama-server at a time on :28181 (so the Orin's ~7.6 GB unified pool
# is never contended → clean numbers), drives each with bench.py's Test B
# (100-page PDF embed throughput), then tears it down before the next cell.
#
# Matrix (default): the three cells that answer the question —
#   1. Q8_0  on GPU (-ngl 99)   the safe QAT default, offloaded
#   2. bf16  on GPU (-ngl 99)   does bf16 beat Q8_0 on sm_87 tensor cores?
#   3. Q8_0  on CPU (-ngl 0)    the baseline we're trying to beat
#
# WHY these flags (see the deep-research findings, 2026-06-26):
#   --pooling mean   EmbeddingGemma is mean-pooled; cls would corrupt output.
#   NEVER fp16       EmbeddingGemma activations (~800k) overflow fp16 (max ~65504)
#                    → inf/NaN → silently wrong embeddings. Use Q8_0/bf16/fp32.
#   -c/-b/-ub 8192   encoders force n_ubatch == n_batch and require ub >= tokens
#                    in the request, so -ub is THE throughput lever — size it to
#                    pack several ~512-tok passages per GPU dispatch.
#
# Usage (run ON the Jetson):
#   Q8_GGUF=/var/lib/virtues/models/embeddinggemma-300m-qat-Q8_0.gguf \
#   BF16_GGUF=/var/lib/virtues/models/embeddinggemma-300m-BF16.gguf \
#   ./gpu_embed_bench.sh --max-perf
#
# Flags:
#   --max-perf   sudo nvpmodel -m 0 + sudo jetson_clocks before the run (MAXN +
#                pinned clocks). Off by default since it changes box-wide state.
#   --with-a     also run Test A (live rerank) — needs a reranker GGUF in RERANK_GGUF.
set -euo pipefail

# ── Config (override via env) ────────────────────────────────────────────────
BIN="${BIN:-/usr/local/bin/llama-server}"
MODELS_DIR="${MODELS_DIR:-/var/lib/virtues/models}"
PORT="${PORT:-28181}"
CTX="${CTX:-8192}"          # -c / -b / -ub all set to this (encoder coupling)
EMBED_BATCH="${EMBED_BATCH:-8}"   # passages/call; 8 × ~512 tok = 4096 ≤ CTX
HOST="127.0.0.1"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

Q8_GGUF="${Q8_GGUF:-}"
BF16_GGUF="${BF16_GGUF:-}"
RERANK_GGUF="${RERANK_GGUF:-}"

MAX_PERF=0
WITH_A=0
for arg in "$@"; do
  case "$arg" in
    --max-perf) MAX_PERF=1 ;;
    --with-a)   WITH_A=1 ;;
    *) echo "unknown flag: $arg" >&2; exit 2 ;;
  esac
done

[ -x "$BIN" ] || { echo "llama-server not found/executable at $BIN" >&2; exit 1; }
command -v python3 >/dev/null || { echo "python3 required" >&2; exit 1; }

# ── Build the matrix: "LABEL|MODEL|NGL" ──────────────────────────────────────
MATRIX=()
if [ -n "$Q8_GGUF" ]; then
  [ -f "$Q8_GGUF" ] || { echo "Q8_GGUF not found: $Q8_GGUF" >&2; exit 1; }
  MATRIX+=("q8-gpu|$Q8_GGUF|99")
  MATRIX+=("q8-cpu|$Q8_GGUF|0")
else
  echo "WARN: Q8_GGUF unset — skipping Q8 cells" >&2
fi
if [ -n "$BF16_GGUF" ]; then
  [ -f "$BF16_GGUF" ] || { echo "BF16_GGUF not found: $BF16_GGUF" >&2; exit 1; }
  MATRIX+=("bf16-gpu|$BF16_GGUF|99")
else
  echo "WARN: BF16_GGUF unset — skipping the bf16-vs-Q8 comparison" >&2
fi
[ "${#MATRIX[@]}" -gt 0 ] || { echo "no models to test — set Q8_GGUF and/or BF16_GGUF" >&2; exit 1; }

# ── Optional: unleash the hardware ───────────────────────────────────────────
if [ "$MAX_PERF" = 1 ]; then
  echo "→ MAXN + jetson_clocks (sudo)…"
  sudo nvpmodel -m 0 || echo "  (nvpmodel failed — not a Jetson? continuing)"
  sudo jetson_clocks || echo "  (jetson_clocks failed — continuing)"
fi

SERVER_PID=""
LOGFILE="$(mktemp)"
cleanup() { [ -n "$SERVER_PID" ] && kill "$SERVER_PID" 2>/dev/null || true; rm -f "$LOGFILE"; }
trap cleanup EXIT

start_server() {  # $1=model $2=ngl
  : > "$LOGFILE"
  "$BIN" --embedding --pooling mean -m "$1" \
    --host "$HOST" --port "$PORT" \
    -c "$CTX" -b "$CTX" -ub "$CTX" -np 1 --cache-ram 0 -ngl "$2" \
    >"$LOGFILE" 2>&1 &
  SERVER_PID=$!
  # wait for /health (≤90s); bail if the process died
  for _ in $(seq 1 90); do
    if ! kill -0 "$SERVER_PID" 2>/dev/null; then
      echo "  server exited during startup — log tail:" >&2; tail -20 "$LOGFILE" >&2; return 1
    fi
    if curl -fsS "http://$HOST:$PORT/health" >/dev/null 2>&1; then return 0; fi
    sleep 1
  done
  echo "  server never became healthy — log tail:" >&2; tail -20 "$LOGFILE" >&2; return 1
}

stop_server() { [ -n "$SERVER_PID" ] && kill "$SERVER_PID" 2>/dev/null || true; wait "$SERVER_PID" 2>/dev/null || true; SERVER_PID=""; }

# ── Run the matrix ───────────────────────────────────────────────────────────
echo "=== embed bench :: ctx=$CTX embed-batch=$EMBED_BATCH max-perf=$MAX_PERF ==="
RESULTS=()
for cell in "${MATRIX[@]}"; do
  IFS='|' read -r label model ngl <<< "$cell"
  echo
  echo "── cell: $label  (ngl=$ngl)  $(basename "$model")"
  if ! start_server "$model" "$ngl"; then
    RESULTS+=("$label|START-FAILED"); continue
  fi
  # Confirm GPU offload actually happened (the silent-CPU-fallback trap)
  if [ "$ngl" != 0 ]; then
    if grep -qiE "offloaded .*layers? to|CUDA[0-9]" "$LOGFILE"; then
      echo "  ✓ offload: $(grep -iE 'offloaded .*layers? to' "$LOGFILE" | tail -1 | sed 's/^[[:space:]]*//')"
    else
      echo "  ⚠ no offload line in log — may be on CPU. tail:"; grep -iE "backend|cuda|layer" "$LOGFILE" | tail -5
    fi
  fi
  # Drive it — Test B only (embed throughput) unless --with-a
  bench_args=(--label "$label" --embed-url "http://$HOST:$PORT" --embed-batch "$EMBED_BATCH" --skip-a)
  out="$(python3 "$HERE/bench.py" "${bench_args[@]}" 2>&1)" || true
  echo "$out"
  line="$(printf '%s\n' "$out" | grep -i '100-page embed' | tail -1 || true)"
  RESULTS+=("$label|${line:-no-result}")
  stop_server
done

# ── Summary ──────────────────────────────────────────────────────────────────
echo
echo "================= SUMMARY (100-page embed wall-clock) ================="
for r in "${RESULTS[@]}"; do
  IFS='|' read -r label line <<< "$r"
  printf '  %-10s %s\n' "$label" "$line"
done
echo "======================================================================"
echo "Read against the prior finding (CPU won):"
echo "  • If both GPU cells stay SLOWER than q8-cpu → prior result confirmed,"
echo "    embed→CPU stands. Stop chasing the GPU; ship the GGUFs."
echo "  • If bf16-gpu beats q8-cpu but q8-gpu doesn't → the fp32-forcing was the"
echo "    culprit and bf16 dodges it; reconsider embed→GPU with a bf16 GGUF."
echo "  • If a GPU cell wins ONLY at this batch size → it's a throughput (PDF)"
echo "    win, not a latency one; keep CPU for trickle, GPU for big foreground embeds."
