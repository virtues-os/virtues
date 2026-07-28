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
- **Runtime libs (on the box):** `libQnnHtp.so` + the v68 skel etc. come from the
  same QAIRT **Community** SDK the daemon is built against — the public,
  unauthenticated, version-pinned zip named by `QAIRT_VERSION`. Take the host
  libs from `lib/aarch64-*-linux-*/` and the DSP skel from
  `lib/hexagon-v68/unsigned/`; the four host libs the daemon actually needs
  total ~6 MB and the skel ~9 MB, so a Range-extract beats pulling the 1.44 GB
  zip. The installer auto-detects whatever is on the box and points the unit's
  `LD_LIBRARY_PATH` + `ADSP_LIBRARY_PATH` there (`VIRTUES_QNN_LIB_DIR` to
  override); a sold appliance image can bake them onto the ldconfig path. A
  Radxa QAIRT install also works, with the caveat that its version must match
  the QAIRT the `.bin` context binaries were compiled with.

  **Why not `pip install onnxruntime-qnn`**, which this document recommended
  until 2026-07-27: it is a real option — Qualcomm publishes that package
  themselves and it *does* ship `manylinux_2_34_aarch64` wheels (from 2.3.0
  onward) carrying `libQnnHtp.so`, `libQnnSystem.so`, the V68 stub and the V68
  skel. Three reasons we don't use it. The wheel's libs are QAIRT
  **2.48.40.260702** while the `.bin` context binaries are compiled against
  **2.42.0.251225**, and that pairing is the one thing that must not drift. It
  is a 78 MB wheel carrying six Hexagon skels (V68…V81) where we need one. And
  it puts a Python environment in the boot path of the inference daemon.

  The older advice also failed for a duller reason worth remembering: a README
  telling a human to run `pip install` installs nothing. Nothing automated it,
  so boxes simply had no runtime libs — which is what `qairt.rs` now fixes.

We do not re-host the Qualcomm libs. That is a license constraint, not a
preference: the SDK's `LICENSE.pdf` ("AI Stack License") grants distribution of
the Software in object code as incorporated in your own application, and then
expressly withholds any license to distribute it **on a standalone basis** —
which is exactly what publishing bare `.so` assets to our `models-*` bucket
would be. The libs come from Qualcomm's own public distribution, fetched per
box.

That reading is confirmed across both of Qualcomm's own channels: the
`Qualcomm_LICENSE.pdf` bundled beside the binaries in the onnxruntime-qnn wheel
is **byte-identical** (same SHA256) to the SDK's `LICENSE.pdf`. The wheel's own
MIT licence covers the ONNX Runtime plugin code, not the `.so` files — the same
terms follow the Qualcomm binaries down every path they ship. Qualcomm
redistributing their own libraries is Qualcomm exercising rights they hold; it
grants us nothing.

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
