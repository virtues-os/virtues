# Local mode

**Written 2026-09-29.** A fourth chat mode, "Local", beside Chat, Deep research
and Sudo: a turn runs Qwen3-0.6B on the Dragon's NPU and nowhere else. It is
framed as a demonstration of where fully local AI is today, not as a
companion, and a card at the top of the chat says what it gets wrong. Built
2026-09-28 (first in `v0.1.10-staging.87`); local chats became saved chats
on 2026-09-29. Spikes ran 2026-09-24 to 09-28. Supersedes
`agents/plan/local-model-plan.md` (deleted). The code is
`virtues-core/src/local_model.rs` and `api/local_chat.rs`.

---

## Why a demonstration, not a feature

People asked for a 100% local mode, "even if it takes 45 minutes". The
spikes answer that the Dragon cannot run a model good enough for the
conversations people actually bring: relationships, faith, health, and
rewriting messages.

**What the box can run.** Only the NPU. The CPU belongs to the app, so CPU
inference (including streaming a large mixture-of-experts model off the NVMe)
is out. The GPU has no compute driver: `msm_dpu` is display-only and there is
no Vulkan ICD. The NPU runs fixed, precompiled graphs and tops out at a 4k
context.

**What those models do.** Eleven real owner conversations (advice, emotional
questions, theology, rewriting, a medical question, arithmetic) went through
Qwen3-0.6B, Qwen3-1.7B and MiniCPM5-1B on a Mac, at the NPU build's Q8
precision, with and without instructions and background. The failure classes,
in order of how much they matter:

- **Unsafe health advice.** With no instructions, 0.6B told the user a
  veterinary drug was safe for people. A short safety instruction fixed it,
  which is why the mode keeps one (`SAFETY_PROMPT`).
- **Invented numbers.** Every model produced precise-looking statistics that
  do not exist, and kept doing it when told not to. The prompt now asks for
  frequencies in words.
- **Deciding for the person.** Asked whether something was a dealbreaker, they
  answered "No, it's not" until a prompt forced the reply to end on a
  question — and that fix broke other answers.
- **Missing the task.** A draft returned unchanged when asked to rewrite it; a
  text to a partner read as a business email.
- **Wrong basic facts.** One arithmetic question answered 5.5, then 6, then 7
  across prompt versions.

Prompt tuning moved failures around rather than removing them. Qwen3-1.7B
with instructions was the best of the set and still unfit for personal
advice. MiniCPM5-1B scores well on public hallucination benchmarks and did
worst here: those benchmarks measure trivia, not advice.

**0.6B over 1.7B.** Both are unfit for advice, so the demo takes the one that
fits every Dragon: +1.7 GB of RAM against +2.9 GB (5 GB boards exist), 7.5
tok/s against 5.5, a 0.7 GB download against 1.7 GB.

## Measured on the NPU

Lab Dragon (5 GB) and a spare (12 GB), both QCS6490 / Hexagon v68.

| Qwen3-0.6B | 2k context | **4k context** | 16k context |
|---|---|---|---|
| loads | yes | **yes** | **no: crashed the cDSP** |
| prompt reading | 668 tok/s | ~365 tok/s | — |
| generation | 12.4 tok/s | **7.5 tok/s** | — |
| RAM added | +1.3 GB | **+1.7 GB** | — |

Speed is flat regardless of how full the context is: the compiled graphs
attend over the whole window every step, so the compiled size sets the speed.
Qwen3-1.7B at 4k: 264 tok/s reading, 5.5 generating, +2.9 GB. For comparison
the CPU (llama.cpp, four big cores) runs 1.7B at 17–19 tok/s reading and 5–8
generating. Without a daemon, first token costs 1.3–1.8 s per turn, model
load included. Qualcomm's default QnnHtp `poll` busy-waits about 2.6 CPU
cores for no gain; off, it is 0.13 cores at the same speed.

**The 16k crash.** Loading a context binary the cDSP could not map
(`fastrpc ... failed to map buffer`) crashed the cDSP. On process exit the
FastRPC driver freed reserved DMA pages, logged `BUG: Bad page state` sixty
times, and the box stopped answering on every interface until power-cycled.
It was not an OOM, so a memory cgroup would not have helped: the fault is in
the cDSP's address space. What follows from it: the mode loads exactly one
pinned bundle, proven to load on a 5 GB Dragon beside `qnnd`, with no setting
to change it; and the hardware watchdog (8a5ef0ab) reboots a hung Dragon.

## How a turn runs

**No daemon.** Each turn is one run of Qualcomm's `genie-t2t-run`, started by
core, reading a prompt file and streaming the reply. The model's 1.7 GB
returns to the box when the turn ends. One turn at a time; a turn is refused
below ~2.2 GB free memory, or when the prompt would leave under 768 tokens of
the 4096 for the reply, rather than truncating history. Reply length is capped
(768, or 2048 with thinking) because the model does not always write an end
token and would otherwise hold the NPU for about ten minutes.

**The sandbox is the network claim.** `genie-t2t-run` is closed source, so
what it does cannot be read. Every turn runs under `unshare --net
--map-current-user`: a namespace whose only interface is a loopback that is
down. The NPU is a device node, not a network, so the model still reaches it.
A box that refuses unprivileged user namespaces is not offered the mode,
since every turn would fail closed. The plan had drawn a systemd
socket-activated unit with `PrivateNetwork=yes`; the shipped mechanism is the
namespace, which needs no unit and no install step.

**What the model is given.** Only `SAFETY_PROMPT`: no chat prompt, no
background about the owner, no record access, no tools, no model choice, no
gateway, no billing row. A model this small misuses what it is given, and the
demonstration is the model itself.

**Stored like any chat.** Since c3a895a0 a local chat is an ordinary chat that
happens to be answered on the box. The user's message goes through the same
`store_user_turn`, the reply is stored as an assistant row under
`local/qwen3-0.6b` with its reasoning, and the model reads history back from
the box. **So a saved local chat reaches everything a saved chat reaches:
titles, day summaries, search, and a later cloud turn if the person switches
the chat's mode** — and titles and summaries are written by cloud models. Its
replies are written on the server; its words do not stay there. The card says
exactly that ("It writes its replies on your server, not in a data center.
Your server keeps the chat like any other.") and no longer claims nothing
leaves. A temporary chat in Local mode keeps nothing, as any temporary chat.

The first build (4113dd07) did the opposite: local chats were ghost chats by
construction, locked to Local so their history could never ride a cloud turn.
That was dropped the next day in favor of Local being a mode like the others.

## Recipe and traps

- **Model.** Qwen3-0.6B (Apache-2.0), AI Hub Genie export for the QCS6490,
  4096 context, w8a16, published to the `models-1` release as two context
  binaries plus `tokenizer.json`, SHA-pinned in `MODEL_ASSETS`. The export
  device is "Dragonwing RB3 Gen 2 Vision Kit" (the same SoC). Sampling:
  thinking on 0.6 / 0.95 / top-k 20; off 0.7 / 0.8 / 20.
- **`qai-hub-models export` crashes** after downloading into a temp dir it then
  deletes. Pull linked binaries with
  `hub.get_job(<link job>).get_target_model().download()` and build the config
  with `llm_helpers.create_genie_config` plus
  `save_htp_config_for_genie_bundle({'hexagon':'v68','soc-model':'93'})`. On
  v68 the link logs `wtshare ... small.is_shared()` errors; the binaries still
  work.
- **Two runtimes.** AI Hub compiles with QAIRT 2.45, which needs the 2.45
  runtime; `qnnd` stays on 2.42. A 2.45 runtime loads 2.42 binaries, not the
  reverse. Both are fetched from Qualcomm by Range request and never
  re-hosted (`virtues-qairt`, the `GENIE` set); `HEAD` is refused with 403.
- **The runtime set is eight files, not six.** Without
  `libQnnHtpNetRunExtensions.so`, Genie logs "Unable to open backend extension
  library" and fails with device error 14001. It also needs the
  `libcdsprpc.so` link `qnnd` uses.
- **`genie-t2t-run` streams only under `stdbuf -o0`**, echoes the prompt as
  `[PROMPT]: …` before `[BEGIN]: `, and keeps generating after its stdout
  closes, so Stop kills the process. The echo is skipped by the prompt's byte
  length, never by searching for `[BEGIN]: `, because a person can type that.
  With thinking off the model can still open with a stray `</think>`.

## The ceiling is the silicon

Hexagon v68 takes 8-bit weights at a 4k context. Newer Qualcomm NPUs (v73+)
take 4-bit weights and 2–8B models, which is the only route found to a local
model fit for the conversations above.
