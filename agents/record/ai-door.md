# One AI door

**Written 2026-09-29.** How the box and the cloud proxy stopped guessing token
caps, and why the request path looks the way it does. Built 2026-09-08 (the
proxy) and 2026-09-14 (the box, the stream, the transport); the box half first
shipped in v0.1.7. Supersedes `agents/plan/ai-door-plan.md` (deleted).

---

## The story

On 2026-08-27 the Chat slot moved from a model that answers to a model that
thinks first. Nothing else changed. Every token cap in the tree had been set by
hand for the old model, and Anthropic counts thinking inside that cap, so two
background writers began spending their whole budget thinking and returning
nothing. Day summary noticed first because it runs hourly and bills. The
interview drafter noticed second, through a beta tester, because it runs once
per box. Both were "fixed" by raising a number to 16k. The number was never
the bug. **The model was chosen in one place and the cap was guessed in twelve
others**, and nothing told a caller when its guess stopped being true. Worse,
the proxy filled in `max_tokens: 4096` whenever a caller sent none, so live
chat, which deliberately sent no cap, ran under one anyway, on a model that
spends that cap on thinking.

## The invariant

> **Tokens are resolved, never guessed. A request names a thinking mode; the
> helper turns the mode and the model's catalog entry into a request; the
> proxy forwards it whole; the response says how it ended.**

## What was built

**The proxy forwards opaquely.** `services/virtues-api`'s
`providers::upstream_body` takes the box's body as JSON and rewrites only what
the proxy owns: `model`, `stream_options.include_usage`, a `temperature`
default, `provider_options` → `providerOptions` with zero-retention merged
over it, the empty-tools guard, and stripping the dead `thought_signature`.
Every other field passes through, so a field the box adds reaches the gateway
with no proxy edit, and one the gateway rejects comes back as its 400. It
never invents `max_tokens`: absent means the model's own window.

That shape was the second attempt. The first, the same morning, was a shared
`virtues-ai-wire` crate holding one request type for both sides. It treated
the symptom: the proxy still rebuilt the outgoing JSON field by field from the
shared struct, so a new field compiled on both ends and still never reached
the gateway — the allowlist had only moved one function over. Commit 0c265aea
deleted the crate the same day. The request builder is the box's alone
(`virtues_api/request.rs`); `ReasoningFacts` sits in `virtues-registry` beside
the slots it feeds, as a shape rather than a model fact.

**The catalog says who thinks.** The proxy parses the gateway's
`reasoning_options` and serves `ReasoningFacts` (thinks, can disable, effort
values) on every picker entry.

**Callers name a mode, not a number.** `Thinking::{Off, Low, Default, High}`.
`system_completion` takes a mode and a temperature and nothing else, and sends
no `max_tokens`. Day summary asks for Low; the interview drafter, chapter
extraction, entity articles, compaction, chat titles and bookmark enrichment
ask for Off. On the BYO route there is no catalog entry and no gateway, so the
helper sends at most `reasoning_effort`, never the gateway's `reasoning`
object. The output ceilings on titles (50) and inline edit (512) went too:
the catalog lists a thinking toggle for a model that cannot actually turn
thinking off, so `can_disable` is a claim, and no ceiling is safe on a claim.

**Every ending has a name.** The helper reads `finish_reason`, and `length`
is its own error ("ran out of room"), distinct from empty content, which had
been the only failure signal. The chat stream emits the whole UI-message
protocol: `start`, `start-step`/`finish-step` around each agent step,
`tool-output-error`, `finish` with a reason, and `abort` on Stop. A turn that
hit its limit says "reached its output limit" instead of looking complete.

**Thinking text is asked for, and thought is resumed.** Live chat sends its
own `temperature: 0.7` and attaches the catalog's per-provider display options
so Claude returns summarized thinking and Gemini includes thoughts. The stream
parser learned `delta.reasoning`, the gateway's spelling; it had only ever
read the DeepSeek-style `reasoning_content`, so the thinking block was empty
for reasons beyond the display flag. The gateway's `reasoning_details` are
stored on the assistant row (migration 0019) and echoed on resend. The
`thought_signature` plumbing, which had never reached a model because the
proxy never had the field, was deleted end to end.

**The transport sends the last message only**, or none on regenerate, with the
trigger. The box, on regenerate, deletes its trailing assistant rows before
answering. Before this it kept the old answer in history while the client had
removed it, so "regenerate" answered with the old reply in front of the model.
Temporary (ghost) chats are the exception by design: the box holds nothing for
them, so the wire carries the whole transcript.

## The gate

`ui_stream_fixture` in `api/chat.rs` writes the box's canonical turn and a
stopped turn to `apps/web/src/lib/ai/fixtures/` from the same serializer that
serves a real one, and fails when they are stale. `uiStream.test.ts` feeds
them through the SDK's own `DefaultChatTransport`, so the SDK's schema and
parser judge the box. It found the first real bug before a browser did: the
SDK forgets its open parts at every `finish-step`, so the one text part per
turn the box had always streamed could never carry step boundaries. Text and
reasoning parts now open and close per step.

## Decided no

- **Native tool approval.** The SDK's flow expects the server to read the
  approval off the wire and resume a paused loop, which is a loop redesign.
  The permission-then-regenerate flow works now that regenerate really
  regenerates.
- **Removing the reactivity patch** on the SDK's private `replaceMessage`. It
  needs a browser session to judge, not a test, so it stays with the pin.
- **Gateway reporting tags.** The gateway bills every tag write, and
  `app_ai_calls` already attributes spend per feature on the box for free.

## What still carries a default

The proxy still fills `temperature: 0.7` when absent. Boxes before v0.1.7 send
none, and dropping it would move their chats to the provider's 1.0 in a cloud
deploy nobody can see from the box. It goes once no box that old is served.

## Refuted, so nobody re-argues it

- **"Raise the cap to 16k everywhere."** It pays for thinking the job does not
  want and fails again on the next model with a larger default budget.
- **"Put `max_output_tokens` in the registry crate."** The crate's own test
  forbids model facts: they are gateway facts and go stale.
- **"Use the Lite slot for extraction."** Lite honors the owner's background
  pin, which may be a BYO endpoint with no retention promise. The interview
  promised zero retention in as many words.
- **"Read `reasoning_tokens` to size the cap."** Anthropic reports none
  through the gateway; the column reads zero whether or not thinking happened.
  The honest signal is `finish_reason` plus completion tokens.
- **"Reject unknown fields at the proxy."** The proxy serves every released
  box at once; a 400 on a field old boxes send is an outage. There is no
  shared request type either — that was tried and deleted (above). The proxy
  forwards what it does not own and lets the gateway judge the shape.
- **"Share one request struct between box and proxy."** See 0c265aea: a
  shared type that the proxy re-serializes is still an allowlist.
- **"Ghost mode works because the append fails."** It did not fail. The
  handler created the chat row first, so every ghost message persisted until
  the box read the flag it had been sent all along (fixed 2026-09-08).
