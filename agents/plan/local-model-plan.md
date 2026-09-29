# Local mode — a small model on the server's own NPU, as a demonstration

**Status: planned, spikes done (2026-09-24 to 09-28).** A fourth chat mode,
"Local", beside Chat, Deep research and Sudo. A turn in Local mode runs
Qwen3-0.6B on the Dragon's NPU and nowhere else. It is framed honestly as a
demonstration of where fully local AI is today, not as a companion: a card at
the top of the chat says what it is, what it gets wrong, and why.

Delete this plan when the mode ships; what survives is a record of the
measurements and a manual page.

---

## 1. Why a demonstration, not a feature

People asked for a 100% local mode, "even if it takes 45 minutes". The spikes
answer that the Dragon cannot run a model good enough for the conversations
people actually bring: relationships, faith, health, and rewriting messages.

**What the box can run.** Only the NPU is available. The CPU belongs to the
app, so CPU inference (including streaming a large mixture-of-experts model
from the NVMe) is ruled out. The GPU has no compute driver (`msm_dpu` is
display-only, and there is no Vulkan ICD). The NPU runs fixed, pre-compiled
graphs and tops out at a 4k context (§2).

**What those models do.** Eleven real owner conversations (advice,
emotional questions, theology, rewriting, a medical question, arithmetic) went
through Qwen3-0.6B, Qwen3-1.7B and MiniCPM5-1B on a Mac, at the same Q8
precision as the NPU build, with and without instructions and background.
The failure classes, in order of how much they matter:

- **Unsafe health advice.** With no instructions, 0.6B told the user a
  veterinary drug was safe for people. A short safety instruction fixed it,
  which is why §4 keeps one.
- **Invented numbers.** Every model produced precise-looking statistics that
  do not exist, and kept doing it under instructions not to.
- **Deciding for the person.** Asked whether something was a dealbreaker,
  the models answered "No, it's not" until a prompt forced the reply to end
  on a question. That fix then broke other answers.
- **Missing the task.** They returned a draft unchanged when asked to rewrite
  it, and read a text to a partner as a business email.
- **Wrong basic facts.** One model answered the same arithmetic question 5.5,
  then 6, then 7 across prompt versions.

Prompt tuning moved failures around rather than removing them. Qwen3-1.7B with
instructions was the best of the set and still unfit for personal advice.
MiniCPM5-1B scores well on public hallucination benchmarks and did worst here:
those benchmarks measure trivia, not advice. The public benchmarks don't test
the conversations that matter, so the only real test is this one.

**So the mode tells the truth.** A demonstration that is honest about its
limits says more about Virtues than a weak feature would. Only the box's own hardware is in scope.

**0.6B over 1.7B.** Both are unfit for advice, so the demo takes the one that
fits every Dragon: +1.7 GB of RAM against +2.9 GB (5 GB boards exist), 7.5
tok/s against 5.5, and a 0.7 GB download against 1.7 GB.

## 2. What the spikes measured

Lab Dragon (5 GB) and a spare (12 GB), both QCS6490 / Hexagon v68, kernel
6.18.2-3-qcom.

| Qwen3-0.6B on the NPU | 2k context | **4k context** | 16k context |
|---|---|---|---|
| loads | yes | **yes** | **no: crashed the cDSP** |
| prompt reading | 668 tok/s | ~365 tok/s | — |
| generation | 12.4 tok/s | **7.5 tok/s** | — |
| RAM added | +1.3 GB | **+1.7 GB** | — |

Speed is flat regardless of how full the context is. The compiled graphs
attend over the whole window on every step, so the compiled context size sets
the speed.

Qwen3-1.7B at 4k (spare board, for reference): loads, 1.3 s load, 264 tok/s
reading, 5.5 tok/s generation, +2.9 GB.

CPU for comparison (llama.cpp, 4 big cores): Qwen3-1.7B reads 17–19 tok/s and
generates 5–8 tok/s.

### What the 16k crash taught

Loading a context binary the cDSP could not map (`fastrpc ... failed to map
buffer`) crashed the cDSP. On process exit, the FastRPC driver freed reserved
DMA pages, logged `BUG: Bad page state` sixty times, and the box stopped
answering on every interface until it was power-cycled. It was not an OOM.
Three rules follow:

- **Only ship a model and context proven to load on a 5 GB Dragon with `qnnd`
  running.** The pinned bundle is the only thing the mode can load; there is
  no setting to change it.
- **The hardware watchdog is the backstop.** It shipped in 8a5ef0ab
  (`v0.1.10-staging.86`): every Dragon now reboots itself 30 s after a hard
  hang, and `qnnd` exits after three consecutive failed executes so systemd
  restarts it.
- **A memory cgroup does not protect against this.** The crash is in the
  cDSP's address space, not in RAM.

### Traps

- **AI Hub compiles with QAIRT 2.45**, which needs the 2.45 runtime; `qnnd`
  stays on 2.42. A 2.45 runtime loads 2.42 binaries, not the reverse.
  URL: `.../Qualcomm_AI_Runtime_Community/All/2.45.0.260326/v2.45.0.260326.zip`;
  `HEAD` is refused with 403, so use a ranged `GET`.
- **The runtime set is eight files, not six.** `genie-t2t-run`,
  `libGenie.so`, `libQnnHtp.so`, `libQnnSystem.so`, `libQnnHtpV68Stub.so`,
  `libQnnHtpV68CalculatorStub.so` and **`libQnnHtpNetRunExtensions.so`**
  (without it: "Unable to open backend extension library", then device error
  14001) on the host side, plus `libQnnHtpV68Skel.so` on the DSP side. It also
  needs the `libcdsprpc.so` link that `qairt.rs::link_cdsprpc` already makes
  for `qnnd`.
- `qai-hub-models export` crashes after downloading into a temp dir it then
  deletes. Pull linked binaries with
  `hub.get_job(<link job>).get_target_model().download()`, and build the config
  with `llm_helpers.create_genie_config` plus
  `save_htp_config_for_genie_bundle({'hexagon':'v68','soc-model':'93'})`.
  The export device is "Dragonwing RB3 Gen 2 Vision Kit" (the same QCS6490).
- On v68 the recipe compiles w8a16 and the link logs `wtshare ...
  small.is_shared()` errors; the binaries still work. Part 1 of the bundle is
  only the embedding lookup.
- `genie-t2t-run` streams only under `stdbuf -o0`, echoes the prompt as
  `[PROMPT]: …` before `[BEGIN]: `, and **keeps generating after its stdout
  closes** (§4).

## 3. Model

Qwen3-0.6B (Apache-2.0), AI Hub Genie export for the QCS6490, 4096 context,
w8a16. The exact export command goes in
`models/recipes/qwen3-0.6b-genie-q6a.toml`. Sampling: thinking on 0.6 / 0.95 /
top-k 20; thinking off 0.7 / 0.8 / 20.

## 4. Architecture

**No daemon.** Each turn is one run of Qualcomm's own `genie-t2t-run`, started
by systemd in a sandbox with no network:

```
composer (mode = local) ── SSE ──▶ virtues-core ── /run/virtues-local-model.sock ──▶ systemd (Accept=yes)
                                   agent turn for mode "local"                          └─ virtues-local-model@.service
                                                                                           genie-turn.sh → genie-t2t-run
```

- **`virtues-local-model.socket`**: a Unix socket owned by `virtues`, mode
  0660, `Accept=yes`, `MaxConnections=1`. Each connection spawns a fresh
  `virtues-local-model@.service` whose stdin and stdout are the connection.
  Core writes the whole prompt, half-closes, and reads tokens until EOF.
- **Stop is closing the socket.** The wrapper runs the model into a fifo and
  relays with `cat`. When the reader hangs up, `cat` dies on the next token and
  the wrapper kills the model. Measured: gone 0.16 s after the socket closes.
- **Busy is systemd's.** A second connection during a turn is reset.
- **Memory frees itself.** The model lives only as long as the turn. Before
  connecting, core checks `MemAvailable` against 1.7 GB plus a 512 MB floor.
- **Stats come free.** The wrapper ends the stream with `\x1e` and Genie's
  `--profile` JSON.
- **The sandbox is the privacy claim.** `PrivateNetwork=yes`,
  `RestrictAddressFamilies=AF_UNIX`, `IPAddressDeny=any`, plus `qnnd`'s
  hardening. `genie-t2t-run` is closed source; the kernel is what guarantees it
  sends nothing. Verified: the same answer came back inside a namespace whose
  only interface was loopback.

Measured cost of no daemon: first token 1.3–1.8 s per turn, including the
model load and re-reading the conversation.

**Core.** Mode `local` is a fourth `AgentModeId`. For it, the agent turn skips
`model_choice`, tools and retrieval entirely:

- **A short safety prompt, not the chat prompt.** Answer briefly; describe
  amounts in words, not numbers; give only well-established health facts and
  point to a doctor or pharmacist; on personal decisions, help the person
  think and don't decide for them. It carries no background about the owner
  and no record access. The demo's point is the model itself, and a model this
  small misuses what it is given. A test asserts this prompt is the only
  system message a local turn sends.
- Builds the ChatML prompt from the chat so far (answers only, never earlier
  thinking; thinking off ends the prompt with the empty `<think>` block).
- Parses the stream: skips the prompt echo by its byte length (the person can
  type `[BEGIN]: ` themselves), splits reasoning from text at `</think>`, and
  stops at `[END]`.
- Counts tokens with `tokenizers` on the shipped `tokenizer.json`, and refuses
  a turn that would overflow 4096 before a process starts.

**A local chat never reaches a cloud model.** This is the design's hard
invariant and the reason the mode is more than a composer chip:

- A chat that starts in Local stays Local. The mode picker does not offer a
  switch mid-chat, because the next cloud turn would carry the local history.
- v1 does not store local chats. The transcript lives in the page and is sent
  whole each turn. So nothing downstream can read them: not title generation,
  day summaries, the wiki, embeddings, or backups. Storing them is §9's
  question and needs one enforced exclusion, not a set of remembered ones.
- Nothing in the local path calls virtues-api, the proxy, or a slot.

**Installer, Dragon only.** `install_local_model` beside `install_qnn`:
- The eight runtime files by the existing Range fetch in `qairt.rs`,
  SHA-pinned and never re-hosted, under a `lib-2.45/` sibling of `qnnd`'s.
- `genie-turn.sh` and the two units in the release tarball.
- The model is not fetched at install. It downloads on first use of the mode
  from the `models-1` release, with consent (§6). Extend
  `tools/publish-qnn-models.sh` rather than writing a second script.
- Non-Dragon servers do not offer the mode.

## 5. The mode in the chat

The composer's mode chip gains "Local" (Shift+Tab cycles to it). Choosing it on
a new chat puts the card (§6) at the top of the thread, above the first turn,
and sends every turn to the local runner.

**The thinking UI** reuses `ThinkingMark` and `ThinkingBlock`:

- **Reading**, before the first token: the ∴ at depth 3 with the label
  "Reading". It shows every turn, because every turn loads the model (1.3 s
  short, up to about 8 s near a full context). There is no progress bar,
  because Genie reports nothing until the first token.
- **Thinking**: the ∴ **stays at depth 3 and never shows 4**, because four
  dots means the turn went out to the record or a tool, and this mode can't.
  Beside it, one line showing the last complete clause of the reasoning,
  swapping clause by clause, with a pinned height. To the right, `14s · 212
  tokens` in tabular figures. Tapping the line opens the full reasoning.
- **Answered**: the mark lands to one dot, and the line becomes "Thought for
  41s" through `ThinkingBlock`'s own `formatDuration`.
- **Under each reply**: `212 tokens · 7.5 a second · on the NPU`. The
  machinery is the demonstration.

A context meter beside the composer (`1,204 of 4,096 tokens`). At 100% the
composer is replaced by the "full" state, never a silently truncated history.
Reduced motion holds the mark still and swaps the clause without a fade.

## 6. Copy

Held to `agents/build/voice.md` § UI copy. Lines marked ⚠ state a behavior
and are checked against the shipping build before they merge.

**The card**, at the top of every local chat:

> **A small model, on your server**
>
> This is Qwen3 0.6B, running on your server's NPU, the chip in it built for running models. Nothing you type here leaves your server. ⚠
>
> It's here to show where fully local AI is today. It's slow, it invents facts, and it gets simple things wrong. Don't rely on it for health, money, or personal decisions.
>
> The models in Chat run in data centers and are far larger. Fitting that quality into a server like this is one of the most active problems in AI right now.

The last paragraph describes the field and promises nothing, which keeps it
inside the claims rule.

| Where | String |
|---|---|
| Mode chip | Local |
| Mode description | A small model on your server's NPU |
| First use | The local model is 0.7 GB and downloads once to your server. After that, it runs without an internet connection. ⚠ |
| First use, button | Download the local model |
| Downloading | Downloading the local model · 320 MB of 0.7 GB |
| Reading | Reading |
| Thought | Thought for 41s |
| Reply caption | 212 tokens · 7.5 a second · on the NPU |
| Meter | 1,204 of 4,096 tokens |
| Leaving the chat | Local chats aren't saved. Leaving this chat clears it. ⚠ |

| Error | String |
|---|---|
| Context full | This chat is full. Start a new local chat to keep going. |
| Not enough memory | Your server needs 2.2 GB of free memory to start the local model and has 1.1 GB. Try again in a few minutes. |
| Download failed | Couldn't download the local model. Check your server's internet connection, then try again. |
| Stopped unexpectedly | The local model stopped before it finished. Your chat is still here, so send your message again. |
| In use | The local model is answering in another window. Stop it there or wait for it to finish. |

## 7. Phases and gates

**Phase 1: runtime, units, core turn** (no UI). Gate on a fresh 5 GB Dragon
install: `curl -N` streams a local turn; closing it kills the model within a
token; a concurrent turn gets the busy error; `qnnd` embed latency stays within
10% during a generation; memory is back to baseline when a turn ends; a prompt
containing `[BEGIN]: ` parses correctly; the unit reports `PrivateNetwork=yes`.

**Phase 2: the mode, the card, the thinking UI.** Gate: on real glass, all
thinking states shown, reduced motion checked, `design-lint` and `check-copy`
clean, and a test that a local chat can't switch to a cloud mode.

**Phase 3: ship.** A manual page and a record of these measurements, then
delete this plan.

About a day and a half across phases 1–2.

## 8. Decisions made here

- A demonstration with an honest card, not a companion.
- Qwen3-0.6B at 4k, one pinned bundle, every Dragon.
- A short safety prompt; no owner background, no record access, no tools.
- No daemon: one sandboxed `genie-t2t-run` per turn, with no network.
- A local chat stays local and v1 does not store it.
- The model downloads on first use, with consent.
- The ∴ never shows depth 4 in this mode.

## 9. Open questions

- **Storing local chats.** Only with one enforced exclusion from every
  cloud-bound read.
- **Box v2.** Newer Qualcomm NPUs (v73+) take 4-bit weights and 2–8B models;
  that is the only way the box itself answers privately.
