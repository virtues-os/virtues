# virtues-qnnd

The NPU inference daemon for the **Radxa Dragon Q6A** (Qualcomm QCM6490,
Hexagon **v68** NPU) — the one fully-supported Virtues appliance board. It loads
QAIRT context binaries once and serves the box's **llama-server-compatible HTTP
inference contract** (`/health`, `/v1/models`, `/v1/embeddings`, `/v1/rerank`)
on loopback `:18181`/`:18182` — so virtues-core talks to a Dragon exactly the
way it talks to the llama-server sidecars or any BYO endpoint
(`VIRTUES_EMBED_URL`/`VIRTUES_RERANK_URL`), with no QNN-specific code path.

Internally the C++ engine (`csrc/qnn_server.cpp`) still runs its tiny binary
TCP loop (`--port 7788`, now an implementation detail); the Rust layer
(`src/engine.rs`, moved from virtues-core) owns tokenization, the gte/ColBERT
packing rules, and MaxSim scoring, and `src/http.rs` exposes the contract.
`--no-http` preserves the legacy TCP-only shape for the on-device dev tools.

Validated on-device: gte-small embed **3.8 ms/call**, colbert@256 rerank
**7.5 ms/call**, HTP turbo/burst power mode.

## What's in the repo vs. what isn't

- **In the repo:** our daemon source (`csrc/qnn_server.cpp`) and the cargo wrapper.
- **NOT in the repo (by license):** the Qualcomm QAIRT SDK headers/libs
  (Confidential/Proprietary) and the compiled `.bin` context binaries (large,
  delivered via the `models-*` release bucket like the GGUFs).

## Building

`build.rs` compiles the C++ against a QAIRT SDK located via `QNN_SDK_ROOT`:

```sh
QNN_SDK_ROOT=/qairt-extract/qairt/2.42.0.251225 cargo build -p virtues-qnnd --release
```

Without `QNN_SDK_ROOT` (all normal dev machines and non-Dragon CI legs), a stub
is compiled so the workspace still builds; the stub binary just errors if run.
Only builds with the SDK present drive the NPU.

Standalone (no cargo), for reference:

```sh
g++ -O2 -std=c++17 csrc/qnn_server.cpp -o virtues-qnnd -ldl \
    -I$QNN_SDK_ROOT/include -I$QNN_SDK_ROOT/include/QNN
```

## Running

```sh
virtues-qnnd gte_v68_vtcm2.bin cb256_v68_vtcm2.bin --burst [--port 7788]
```

Positional args are context binaries in model-index order:
`idx 0` = embed (gte-small, 384-d), `idx 1` = rerank (colbert@256). `--burst`
enables HTP performance/turbo power corners. The daemon `dlopen`s
`libQnnHtp.so` + `libQnnSystem.so` at runtime, so those (from the QAIRT SDK)
must be on the library path on the target box.

## Protocol (loopback TCP, little-endian)

```
request : u32 model_idx | u32 payload_bytes | payload   (concatenated raw input
                                                          tensors, native dtype;
                                                          batch = payload_bytes /
                                                          per-input byte size)
response: u32 status(0=ok) | u32 payload_bytes | payload (concatenated fp32
                                                          outputs, dequantized)
```

Inputs are packed int32 token IDs (tokenization happens client-side, in
virtues-core). Embed output is a 384-d fp32 vector (already L2-normalized).
Rerank output is 256×96 fp32 token embeddings for ColBERT late-interaction
(MaxSim is computed client-side).

## Release & runtime pipeline

All three pieces are free + public — no appliance image required (that's just
polish for a plug-and-play box you sell).

- **Daemon binary (CI):** the aarch64 release leg `wget`s the QAIRT **Community**
  SDK (a public, version-pinned download — `QAIRT_VERSION` in
  `release-linux.yml`), sets `QNN_SDK_ROOT`, and builds `virtues-qnnd` into the
  tarball. The build is header-only (the daemon `dlopen`s the runtime libs on the
  box). x86 / no-SDK builds compile the stub, so nothing else breaks.
- **`.bin`s + tokenizers (models bucket):** published out-of-band (like the
  GGUFs) via `tools/publish-qnn-models.sh` — currently the exact on-NPU-validated
  files from the lab Dragon. Regenerate from ONNX via **Qualcomm AI Hub** (free
  cloud compile, BYO model, artifacts yours) — recipes in the `models/` repo
  (`gte-small-q6a.toml`, `answerai-colbert-small-q6a.toml`).
- **Runtime libs (on the box):** `libQnnHtp.so` + the v68 skel etc. come from
  `pip install onnxruntime-qnn` (Qualcomm-maintained, PyPI — verified sufficient
  on-device) or a Radxa QAIRT install. The installer auto-detects them and points
  the unit's `LD_LIBRARY_PATH` + `ADSP_LIBRARY_PATH` there (`VIRTUES_QNN_LIB_DIR`
  to override); a sold appliance image can bake them onto the ldconfig path.

We do not re-host the Qualcomm libs; they're sourced from Qualcomm's own public
distributions (PyPI wheel / QAIRT SDK).

## Model-artifact contract

Context binaries are versioned, immutable, SHA256-pinned, and fetched by the
installer from the `models-*` GitHub release bucket (same mechanism as the
GGUFs). Current v68 artifacts:

| model index | file                  | role                       | dim         |
|-------------|-----------------------|----------------------------|-------------|
| 0           | `gte_v68_vtcm2.bin`   | gte-small embed            | 384         |
| 1           | `cb256_v68_vtcm2.bin` | answerai-colbert-small@256 | 256×96/tok  |

Recipes to regenerate them (off-box; the QAIRT converter/quantizer are x86 /
Windows only) live in the `models/` repo under `recipes/`.
