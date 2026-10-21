# BYO AI plan

**Status:** Open. The route axis is not built, so BYO is still all-or-nothing:
any `/v1/ai/*` call diverts if one credential exists. What remains is the
resolver and per-route audio encoder (Phase 2), many credentials (3b), the
"Test this slot" probes (4), and the per-slot route UI (6).

Shipped, and not re-described here: every `/v1/ai/*` call forks at
`BearerClient` (no per-caller opt-in, no fallback to the wallet on failure);
`normalize_upstream_error`; the per-slot model map on the credential
(`apply_byo_model` — a model id is an address on one gateway, so the route
carries the address and the slot keeps the role); tokens-not-dollars for BYO
usage; the endpoint-URL field replacing the provider dropdown; the
employer-visibility warning. The idea is the same one
[`composable-inference.md`](../build/composable-inference.md) ships for
embeddings and reranking: the user owns the endpoint, and we validate it
before serving it.

**Known limitation worth carrying into the manual:** BYO audio transcription
does not work. The `transcription_resolution` applet sends audio as an
`image_url` data URI, the Vercel-gateway idiom; OpenAI-compatible endpoints
want `input_audio` (chat attachments already send that shape, from
`api/compaction.rs`). Fixing it needs the route axis, because the encoder has
to know where the call is going. It fails with a visible error —
`normalize_upstream_error` turns the provider's 2xx-with-error body into a 502
carrying their message — but it cannot succeed in that shape.

## Architecture in one sentence

**A slot has two independent axes — which model fills it, and whose
credential pays for it — and BYO moves only the second.**

Today the route axis has one position: a single active BYO credential takes
every AI call. Splitting the two axes per slot is the whole plan. Once route and
model are separate, "BYO" is a per-slot routing decision instead of a global
mode, and that settles most of the hard questions below.

## The five slots under BYO

| slot | model axis | route axis | admission check |
|---|---|---|---|
| **Chat** | user-choosable *(already shipped)* | BYO | `slot_model_smoke` legs |
| **Coding** | user-choosable *(already shipped)* | BYO | `slot_model_smoke` legs |
| **Lite** | user-choosable *(already shipped)* | BYO | answers; no runaway reasoning tokens |
| **Image** | overridable (`image_model_id`); not yet a picker option | BYO | prompt → bytes decode as an image |
| **Omni** | **pinned** (the registry's Omni default, a Gemini flash) | BYO | transport only — reaches the id, accepts our audio shape |

Omni stays pinned on the model axis because only one model does this job well
today. Local STT is not a substitute — in-pocket, muffled audio
needs multimodal understanding, and nothing local is close. Every non-Gemini
audio-in model is rejected by the gateway besides. Revisit when that changes;
until then a user choice here can only make the result worse.

But Omni is also the **largest single line item** — plausibly $10–20/mo on
its own, likely the majority of a subscriber's AI spend. So routing Omni is
not optional: a BYO path that cannot move Omni saves roughly a third of the
bill and is not worth building. The user brings a Google AI Studio key, or a
gateway that carries Gemini, and pays Google directly for the same model we
would have picked.

## Locked decisions

| decision | rationale |
|---|---|
| **Route and model are separate axes** | Conflating them is why BYO is all-or-nothing today. Separated, Omni can be BYO-routed while staying model-pinned — the only design that keeps quality and also moves the cost to the user. |
| **The recommended path is a gateway credential; a raw provider key is secondary** | Our request body already *is* the gateway contract (OpenAI chat/completions), and our slot ids are already `provider/model`. One gateway credential fills all five slots. A raw provider key fills them unevenly and requires per-provider request translation we have twice refused to write. |
| **Bedrock is reached through the user's gateway, never directly** | Bedrock is SigV4 — access key + secret + region — not a bearer token. A pasted API key can never work. Vercel AI Gateway and OpenRouter both do BYOK, so "I have Bedrock through work" is solved by a vendor whose job that is. |
| **Every route is probed before it is admitted** | Same rule as manual-mode inference: validate before serving, refuse on mismatch. The gateway's capability tags describe the model, not the shim it is reached through — that is why `slot_model_smoke` exists. |
| **Probe what varies, and nothing else** | Where the *model* varies because the user picked it (Chat, Coding, Lite, Image), test behavior. Where only the *route* varies and the model is pinned (Omni), test transport. A probe that tests more than what varies gives false confidence: a clean fixture clip would pass on models that fall apart on in-pocket muffled audio, which is the only case that matters. False confidence is worse than no probe. |
| **Keep `None` distinct from `false`** | `supports_vision()` and `pricing()` already return `Option` with the argument that callers must distinguish *cannot* from *do not know*. Under BYO, unknown becomes the common case rather than a cold-boot edge. This is the rule most likely to get quietly dropped. |
| **BYO shows tokens, never dollars** | With no `usage.cost` we do not know the price. `providers.rs::calculate_cost` already leaves cost unknown rather than invent a rate; BYO usage must do the same. |
| **BYO is a setting, with no separate SKU** | One paid sub at $20/mo. BYO makes credits last longer and removes the need for top-ups; it is not a price discount. See the pricing decision — a cheap tier is a $0-credit price row, addable later in an afternoon, and not worth a permanent entitlement matrix now. |
| **Never claim BYO is more private** | Routing personal-life prompts through a work Bedrock account means the employer can read them. The accurate claim is "you control the vendor and the bill." Same class of rule as the backup-key rule — a claim about what Virtues cannot see must be true or not made. |
| **The wallet survives BYO** | Exa, Places, Unsplash and Plaid are per-user vendor bills that BYO does nothing about. They keep drawing from the wallet at cost; the wallet just goes mostly idle. |

## What must be built

### Phase 2 — The resolver

One function: `(slot) → (endpoint, credential, model_id)`.

- Model axis: user override → cloud `SlotMap` → compiled floor. Unchanged,
  except Omni ignores the first two by design.
- Route axis: per-slot BYO route → wallet. New.
- The two never consult each other.

Every AI call in the box goes through this. It is also the natural place to
tag which axis produced the answer, which the usage view needs.

**Plus a per-route audio encoder.** Vercel, OpenRouter and LiteLLM agree on
the OpenAI chat/completions shape, but *not* on how audio rides in it. The
transcription applet sends audio as `type: "image_url"` with a `data:audio/…` URI — a
Vercel-gateway quirk. OpenAI's spec, which OpenRouter follows, uses
`input_audio: {data, format}`. So the one call carrying the largest line item
is also the one most likely to 400 on a different gateway. A two-arm match on
route kind, ~60 lines, but it must exist before Omni can be routed anywhere
but Vercel.

### Phase 3b — Many credentials *(still deferred)*

Claim the migration number first (`make migration NAME=byo_slot_routes`), and
remember the placeholder is `.sql.pending` until the SQL is written.

One migration, all columns — there is no new table here.

- BYO credentials become **many, not one**. `source_id = "__byo_ai_key__"`
  becomes a family, keyed by a route name.

  **The reason is that Bedrock has no Gemini.** A user
  whose work gateway is Bedrock cannot route Omni through it at all — and
  Omni is the largest line item, so they need a second, personal Google
  credential. That user is the exact persona this feature exists for. Without
  that case, one credential with per-slot on/off would cover nearly
  everyone for much less code; this is the one fact that makes many-
  credentials worth its invasiveness (it touches `box_status.rs`,
  `billing_state.rs`, `credentials.rs`, and `cli/commands/status_json.rs`,
  all of which assume a single active row).
- **Routes are columns on an existing table.** `app_assistant_profile` already
  carries `chat_model_id`, `lite_model_id`, `coding_model_id`,
  `image_model_id`; five `*_route` columns (none exist yet) sit alongside them and skip a migration's worth of CRUD.
  A dedicated table for five fixed rows would be overengineering.
- Do **not** add `omni_model_id`.
- `SlotMap` grows nothing — Omni stays out of the cloud map by design.

### Phase 4 — The probes

The hard cases are already covered by tests. BYO should run those tests for
the user instead of expecting them to read a comment about `thought_signature`
and vercel/ai #11590. Surface as **"Test this slot,"** per slot, on demand and
on save.

- **Chat / Coding** — promote `slot_model_smoke` from an `#[ignore]`d test to
  a callable prober. Same four legs.
- **Lite** — answers at all; no runaway reasoning tokens (the GLM-5.1 failure
  mode: ~300–460 per turn, uncontrollable, stacking into 20s+ stalls).
- **Image** — prompt, assert bytes return and decode.
- **Omni** — **transport only.** Two questions, both of which the gateway
  answers for free: does this endpoint reach the pinned Omni id (a 404
  says no), and does it accept our audio part shape (a 400 says no). Roughly
  0.2s of *generated silence* carries both — no fixture, no recorded voice,
  no privacy question, and it makes no claim about quality. Silence is
  already a first-class concept in that pipeline.

  There is **no quality probe** here, by design. The model is pinned and
  already benched; a fixture clip would only prove the easy case. If skipping
  the save-time probe entirely is preferable, that is defensible too —
  transcription is a background queue job, so the next real chunk surfaces
  the provider's own 400 or 404 within the hour. The silence probe buys
  save-time feedback for ~30 lines. It is not a quality gate and must never
  be described as one.

A failed probe **blocks the route** for that slot and says which leg failed
in the provider's own words. It never silently falls back to the wallet —
that would spend the user's money to paper over their misconfiguration.

### Phase 6 — The UI

- **Example URLs go in help text, not in code.** An example URL that goes
  stale in help text is only wrong; one that goes stale as a shipped option
  breaks the feature. Name Vercel AI Gateway, OpenRouter, LiteLLM
  and a local Ollama, and state the contract in one sentence — we recommend
  gateways because one credential covers all five slots.
- Expose Image in the picker — `model_catalog.rs` still filters it out with
  Omni as a "system slot."
- Per-slot route selector, with the wallet as the visible default.

## How much the gateways actually agree

Vercel, OpenRouter and LiteLLM are all OpenAI-shaped: same
endpoint, same message array, same SSE streaming, bearer auth. Bedrock agrees
on none of it (`Converse`/`InvokeModel`, SigV4, its own id namespace) and is
reached *through* one of the three, never directly.

Where the three diverge, costliest first:

| divergence | consequence |
|---|---|
| **Audio part shape** — `image_url` data-URI (Vercel) vs `input_audio` (OpenAI/OpenRouter) | The per-route encoder above. Hits Omni, i.e. the money. |
| **Model id namespace** — `xai/grok-4.5` vs `x-ai/grok-4.5` vs admin aliases | Each route carries its own ids; ours never resolve by assumption. |
| **Base path** — OpenRouter `/api/v1`, others `/v1` | Store the full endpoint URL, never construct it. The `custom` provider already does this correctly. |
| **Cost reporting** — Vercel `usage.cost`, OpenRouter opt-in, LiteLLM a response header | Cost is *sometimes* knowable on BYO. Show it opportunistically; never depend on it, never invent it. |
| **Param quirks** — reasoning params vary; the Vercel gateway rejects `response_format` for Gemini | Per-route, discovered by 400. Do not pre-model them. |

In practice, text chat/coding/lite is near-free to port. Audio is the
exception, and audio (Omni) is the cost that makes the feature worth building.

### Probed against live OpenRouter, 2026-08-05

The ids below are the slot defaults of that date; the current ones live in
`crates/virtues-registry/src/models.rs`.

We tested these with a free key:

| probe | result |
|---|---|
| our body shape + `x-ai/grok-4.5` | **200, `"OK"`** — the contract is right |
| `xai/grok-4.5` (our namespace) | 400 `is not a valid model ID` |
| `google/gemini-3-flash` (Omni's pin) | 400 — **OpenRouter does not carry it at all** |
| audio as `image_url` data-URI (ours) | **200** with `error.message = "Failed to load image from data:audio/wav…"` |
| audio as `input_audio` (OpenAI spec) | 402 — recognized *as audio*, gated on credit |

What the probes showed:

1. **The per-route audio encoder is required.** Our shape is not just
   unsupported — the upstream tries to decode audio as an *image*.
2. **The audio failure was the only quiet one.** Wrong model id,
   missing model, unfunded account all fail loudly. The audio rejection arrived
   at HTTP 200, passed `is_success()`, and would have surfaced as
   "missing choices[0].message.content" — the applet reporting a parse error for
   what was an upstream rejection, then retrying every cron tick and re-billing the
   user's key. Fixed by `normalize_upstream_error`: a 2xx carrying `error` and
   no `choices` becomes a 502 with the upstream's own message.
3. **The model-id differences go beyond spelling.** Of our five slot ids, one
   matches verbatim (`google/gemini-3-pro-image`), one is respelled
   (`xai/`→`x-ai/`), and two have **no equivalent** — `zai/glm-4.7-flash` and
   `google/gemini-3-flash`. So a BYO route needs its own model id per slot,
   and the Omni pinning rule needs a caveat: pinning the model means BYO Omni
   works only on routes that carry that exact id. OpenRouter users would need
   `google/gemini-3-flash-preview` or a Gemini 3.5/3.6 flash. Vercel and a
   direct Google AI Studio key are the clean paths.

## Traps

- **`image_gen.rs` posts to `/v1/ai/chat/completions`.** That works on
  Vercel, OpenRouter and LiteLLM; it does **not** work on raw OpenAI, where
  image generation lives at `/v1/images/generations`. Another reason to
  recommend gateways over raw provider keys.
- **A probe checks one moment, but bad transcription data stays stored
  permanently.** Keep the runtime
  silence/length post-check on transcription regardless of which model
  produced it.
- **Purpose tagging is vestigial.** `X-Virtues-Purpose` is telemetry only —
  do not build per-slot accounting on it.
- **Gateway capability tags describe the model, not the shim.** Gemini 3
  advertised `tool-use` and 400'd on parallel calls. Never admit a slot on a
  tag alone.

## Explicitly out of scope

- **Per-provider request translation.** If a user wants Anthropic-native or
  Google-native shapes, they point BYO at a gateway that speaks them. We have
  refused this twice and should keep refusing.
- **Local chat models as a supported slot.** Enthusiast lane at most; tool
  calling through OpenAI-compat shims is exactly what `slot_model_smoke`
  exists to catch.
- **Local STT for Omni.** Tested and rejected — see above.
- **A Lite/cheap SKU.** Revisit only if BYO users measurably churn at $20.

## Open questions

- **Do we measure relay bytes per box?** Not needed for BYO itself, but it is
  the prerequisite for ever pricing a cheaper tier, and BYO users are the ones
  most likely to push data.
- **Should a BYO route be probed on a schedule, not just on save?** Keys get
  revoked and work gateways change policy. A silent 401 mid-day currently has
  nowhere to surface.
- **Does a failed BYO route degrade or stop?** Locked above as "stop, never
  silently fall back to the wallet" — worth re-testing against real user
  reaction, because the alternative is a broken app rather than a surprise
  charge.
