---
title: Faster search on a GPU or NPU
description: Search runs on your server's CPU by default. If the machine has a GPU or NPU, run the embedding model there with llama.cpp and point Virtues at it - the steps for NVIDIA, AMD, Intel, other GPUs, and NPUs.
updated: 2026-10-08
---

Search runs on your server's CPU unless you change it. The installer sets up a
small local engine and its model, and that is enough for everyday use: a few
minutes of work a day for a typical record.

A GPU or NPU makes two things much faster: every search, and the first index
of your history, which on a CPU can take hours. The installer tells you when it
finds one and links here. Setting it up takes about ten minutes, and you can
do it any time after installing.

## How it works

You run the embedding model on the accelerator with
[llama.cpp](https://github.com/ggml-org/llama.cpp)'s `llama-server`, then point
Virtues at it with one command. Virtues never installs GPU or NPU software for
you, because it can't test every machine. What it does is check the server you
give it before switching.

The same four steps apply to every accelerator below. Only the llama.cpp build
you download in step 1 changes.

**1. Download llama.cpp for your hardware.** llama.cpp publishes ready-made
Linux builds on its [releases page](https://github.com/ggml-org/llama.cpp/releases).
Use the build we run or a newer one. Unpack it anywhere:

```bash
curl -fLO https://github.com/ggml-org/llama.cpp/releases/download/b11507/llama-b11507-bin-ubuntu-vulkan-x64.tar.gz
tar -xzf llama-b11507-bin-ubuntu-vulkan-x64.tar.gz
```

**2. Download the model** Virtues uses, so your index stays valid:

```bash
curl -fLO https://huggingface.co/ggml-org/embeddinggemma-2-GGUF/resolve/bfcd298762cc34d0357ece5ebdd31791a3a374d8/embeddinggemma-2-Q8_0.gguf
```

**3. Start the server on the accelerator.** `-ngl 99` puts the whole model on
it:

```bash
./llama-b11507/llama-server --embedding --pooling mean \
  -m embeddinggemma-2-Q8_0.gguf -ngl 99 \
  --host 127.0.0.1 --port 8080 -c 2048 -b 2048 -ub 2048 -np 1 --cache-ram 0
```

Keep it running across reboots the way you run any service, for example with a
systemd unit.

**4. Point Virtues at it**, then restart:

```bash
sudo -u virtues virtues configure-inference --embed-url http://127.0.0.1:8080
sudo systemctl restart virtues
```

The command compares the new server's output with the current one. If both run
the same model it says so, and your index is kept. If it reports a different
model while both servers load the same file, the accelerator is computing the
model wrong - some GPU backends overflow its 16-bit math. Stop there and keep
the CPU.

## NVIDIA GPUs

Download the CUDA build for your machine (`ubuntu-cuda-13.4-x64` on a PC,
`ubuntu-cuda-13.4-arm64` on Arm), with the matching NVIDIA driver installed.
On a Jetson, build llama.cpp from source with CUDA enabled, following
llama.cpp's own build guide.

## AMD GPUs

Download the ROCm build (`ubuntu-rocm-10.0-x64`) if ROCm supports your card,
or the Vulkan build (`ubuntu-vulkan-x64`), which runs on most Radeon GPUs with
the standard Mesa drivers.

## Intel GPUs and NPUs

For Arc and integrated GPUs, use the SYCL build (`ubuntu-sycl-fp32-x64`; the
fp32 build, because the model overflows 16-bit math) or the Vulkan build. For
Intel's NPU, llama.cpp publishes an OpenVINO build
(`ubuntu-openvino-2026.4.1-x64`).

## Other GPUs

Mali, Adreno, and other GPUs with a Vulkan driver can try the Vulkan build
(`ubuntu-vulkan-arm64` or `-x64`). Mobile GPU drivers vary a lot, so step 4's
check matters most here. On our own Dragon, the Adreno GPU's Vulkan driver
hangs, which is why the Dragon uses its NPU instead.

## NPUs

Most NPUs need their vendor's runtime, not llama.cpp. Any server works if it
offers an OpenAI-style `/v1/embeddings` endpoint on your machine or network:
run the same model there, then do step 4 with its URL. On a Radxa Dragon,
Virtues already runs search on the NPU, and none of this applies.
