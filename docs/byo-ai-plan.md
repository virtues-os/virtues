# BYO AI — Plan of Record

*Design-locked 2026-08-05. The generation-side companion to
[`composable-inference.md`](composable-inference.md), which already ships the
same idea for embeddings and reranking: the user owns the endpoint, and we
validate it at the door. Nothing here is built yet; §"What is already true"
separates the two.*

## The one-sentence architecture

**A slot has two independent axes — which model fills it, and whose
credential pays for it — and BYO moves only the second.**

Today those two are conflated: the BYO credential carries a single
`default_model`, so turning BYO on is an all-or-nothing switch that also
overrides model choice. Splitting them is the whole plan. Once route and
model are separate, "BYO" stops being a mode and becomes a per-slot routing
decision, and every hard question below answers itself.

## The five slots under BYO

| slot | model axis | route axis | door check |
|---|---|---|---|
| **Chat** | user-choosable *(already shipped)* | BYO | `slot_model_smoke` legs |
| **Coding** | user-choosable *(already shipped)* | BYO | `slot_model_smoke` legs |
| **Lite** | user-choosable *(already shipped)* | BYO | answers; no runaway reasoning tokens |
| **Image** | user-choosable *(needs exposing)* | BYO | prompt → bytes decode as an image |
| **Omni** | **pinned to `google/gemini-3-flash`** | BYO | transport only — reaches the id, accepts our audio shape |

Omni stays pinned on the model axis because there is one right answer today,
not as paternalism. Local STT is not a substitute — in-pocket, muffled audio
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
| **Route and model are separate axes** | Conflating them is why BYO is all-or-nothing today. Separated, Omni can be BYO-routed while staying model-pinned — which is the only shape that both preserves quality and moves the money. |
| **The paved road is "bring a gateway," not "bring a provider key"** | Our request body already *is* the gateway contract (OpenAI chat/completions), and our slot ids are already `provider/model`. One gateway credential fills all five slots. A raw provider key fills them unevenly and requires per-provider request translation we have twice refused to write. |
| **Bedrock is reached through the user's gateway, never directly** | Bedrock is SigV4 — access key + secret + region — not a bearer token. A pasted API key can never work. Vercel AI Gateway and OpenRouter both do BYOK, so "I have Bedrock through work" is solved by a vendor whose job that is. |
| **Admission is by probe, not by trust** | Same doctrine as manual-mode inference: validate before serving, refuse on mismatch. The gateway's capability tags describe the model, not the shim it is reached through — that is the whole reason `slot_model_smoke` exists. |
| **Probe what varies, and nothing else** | Where the *model* varies because the user picked it (Chat, Coding, Lite, Image), test behavior. Where only the *route* varies and the model is pinned (Omni), test transport. A probe that reaches past what varies is inventing confidence: a clean fixture clip would pass on models that fall apart on in-pocket muffled audio, which is the only case that matters. False confidence is worse than no probe. |
| **`None` is not `false`** | `supports_vision()` and `pricing()` already return `Option` with the argument that callers must distinguish *cannot* from *do not know*. Under BYO, unknown becomes the common case rather than a cold-boot edge. This is the discipline most likely to get quietly dropped. |
| **BYO shows tokens, never dollars** | With no `usage.cost` we do not know the price. `providers.rs::calculate_cost` already establishes that we eat a blind spot rather than invent a rate; the box owes the user the same honesty. |
| **BYO is a setting, not a SKU** | One paid sub at $20/mo. BYO means credits go further and top-ups stop, not a discount. See the pricing decision — a cheap tier is a $0-credit price row, addable later in an afternoon, and not worth a permanent entitlement matrix now. |
| **Never claim BYO is more private** | Routing personal-life prompts through a work Bedrock account means the employer can read them. The honest claim is "you control the vendor and the bill." Same invariant class as the backup-key doctrine — a claim about what Virtues cannot see must be true or not made. |
| **The wallet survives BYO** | Exa, Places, Unsplash and Plaid are per-user vendor bills that BYO does nothing about. They keep drawing from the wallet at cost; the wallet just goes mostly idle. |

## What is already true

More of the shape exists than it looks:

- **Slot resolution is already three-layered** — user override → cloud
  `SlotMap` → the compiled floor in `virtues-registry::models`. Chat, Lite and
  Coding are already user-overridable via `app_assistant_profile`
  (`chat_model_id`, `lite_model_id`, `coding_model_id`).
- **`recommended` already means the right thing** — `catalog.rs` flags the
  five slot models and nothing else, with the comment that everything else is
  "the BYO path: selectable, but its capability flags are the provider's own
  claim."
- **The verifier exists** — `slot_model_smoke` in `virtues-core/src/tools/mod.rs`
  drives a candidate with the real ~40-tool set and checks tool selection,
  valid names, parseable arguments, and parallel calls in one turn. It is
  `#[ignore]`d and run by hand before promoting a model.
- **The unknown-vs-false doctrine exists** — `model_catalog::supports_vision`
  and `::pricing` both return `Option` and say why.
- **One BYO route works** — `client.rs::stream()` consults
  `load_byo_credential` and calls `stream_direct_upstream`.
- **The credential store works** — encrypted at rest via `TokenEncryptor`,
  sudo-gated on `change_byo_key`, single active row at
  `source_id = "__byo_ai_key__"`.

## What must be built

### Phase 1 — Stop the leak *(correctness fix; do this regardless)*

`stream()` is the only path that honors BYO. These four still bill the wallet
while the UI says "BYO active":

- `api/compaction.rs` → `/v1/ai/chat/completions` via `post_json`
- `api/day_summary.rs` → same
- `api/image_gen.rs` → same
- `applets/transcription_resolution/transform.rs` → the Omni call

Funnel all of them through **one resolver**. Until this lands the BYO promise
is false by omission, and everything downstream inherits the lie.

**Shipping order: phase 1 can go alone, and probably should.** It is about a
day, it is pure correctness, it requires no product decisions, and it makes
the claim already on screen true. Phases 2–6 are speculative until a real BYO
user asks for them — and phases 4 (probes) and 6 (UI) are the two that will
grow without a bound if nothing is pulling on them.

### Phase 2 — The resolver

One function: `(slot) → (endpoint, credential, model_id)`.

- Model axis: user override → cloud `SlotMap` → compiled floor. Unchanged,
  except Omni ignores the first two by design.
- Route axis: per-slot BYO route → wallet. New.
- The two never consult each other.

Every AI call in the box goes through this. It is also the natural place to
tag which axis produced the answer, which the usage view needs.

**Plus a per-route audio encoder.** Vercel, OpenRouter and LiteLLM agree on
the OpenAI chat/completions shape, but *not* on how audio rides in it. We
currently send audio as `type: "image_url"` with a `data:audio/…` URI — a
Vercel-gateway quirk. OpenAI's spec, which OpenRouter follows, uses
`input_audio: {data, format}`. So the one call carrying the largest line item
is also the one most likely to 400 on a different gateway. A two-arm match on
route kind, ~60 lines, but it must exist before Omni can be routed anywhere
but Vercel.

### Phase 3 — Data model

Claim the migration number first (`make migration NAME=byo_slot_routes`), and
remember the placeholder is `.sql.pending` until the SQL is written.

One migration, all columns — there is no new table here.

- BYO credentials become **many, not one**. `source_id = "__byo_ai_key__"`
  becomes a family, keyed by a route name.

  **The justification is specifically that Bedrock has no Gemini.** A user
  whose work gateway is Bedrock cannot route Omni through it at all — and
  Omni is the largest line item, so they need a second, personal Google
  credential. That user is the exact persona this feature exists for. Absent
  that argument, one credential with per-slot on/off would cover nearly
  everyone for much less code; this is the one fact that makes many-
  credentials worth its invasiveness (it touches `box_status.rs`,
  `billing_state.rs`, `credentials.rs`, and `cli/commands/status_json.rs`,
  all of which assume a single active row).
- **Routes are columns, not a table.** `app_assistant_profile` already
  carries `chat_model_id`, `lite_model_id`, `coding_model_id`; five
  `*_route` columns sit alongside them and skip a migration's worth of CRUD.
  A dedicated table for five fixed rows is the overengineered choice.
- Add `image_model_id` to `app_assistant_profile`. Do **not** add
  `omni_model_id`.
- `SlotMap` grows nothing — Omni deliberately stays out of the cloud map.

### Phase 4 — The probes

The delicacy is already encoded in tests; BYO's job is to run them on the
user's behalf rather than make them read a comment about `thought_signature`
and vercel/ai #11590. Surface as **"Test this slot,"** per slot, on demand and
on save.

- **Chat / Coding** — promote `slot_model_smoke` from an `#[ignore]`d test to
  a callable prober. Same four legs.
- **Lite** — answers at all; no runaway reasoning tokens (the GLM-5.1 failure
  mode: ~300–460 per turn, uncontrollable, stacking into 20s+ stalls).
- **Image** — prompt, assert bytes return and decode.
- **Omni** — **transport only.** Two questions, both of which the gateway
  answers for free: does this endpoint reach `google/gemini-3-flash` (a 404
  says no), and does it accept our audio part shape (a 400 says no). Roughly
  0.2s of *generated silence* carries both — no fixture, no recorded voice,
  no privacy question, and it makes no claim about quality. Silence is
  already a first-class concept in that pipeline.

  There is deliberately **no quality probe** here. The model is pinned and
  already benched; a fixture clip would only prove the easy case. If skipping
  the save-time probe entirely is preferable, that is defensible too —
  transcription is a background queue job, so the next real chunk surfaces
  the provider's own 400 or 404 within the hour. The silence probe buys
  save-time feedback for ~30 lines. It is not a quality gate and must never
  be described as one.

A failed probe **blocks the route** for that slot and says which leg failed
in the provider's own words. It never silently falls back to the wallet —
that would spend the user's money to paper over their misconfiguration.

### Phase 5 — Honest accounting

- BYO traffic records **tokens and model, no cost**. The usage view shows
  "4.2M tokens on your key" and never a dollar figure.
- The wallet keeps showing dollars for whatever is still on it — Omni or Image
  if unrouted, plus Exa, Places, Unsplash, Plaid.
- One screen, two sections, no invented numbers.

### Phase 6 — The UI

- Rename the `custom` provider to **"AI Gateway"** and make it the first and
  recommended option, with real copy naming Vercel AI Gateway, OpenRouter,
  LiteLLM, Portkey, and work proxies.
- Fix or drop `anthropic` and `google`: `default_endpoint_for` points them at
  `/v1/messages` and `/v1beta` and then posts an OpenAI-shaped body. Both 400
  today. Shipping a dropdown entry that cannot work is worse than omitting it.
- Expose Image in the picker — `model_catalog::models()` currently filters it
  out with Omni as a "system slot."
- Per-slot route selector, with the wallet as the visible default.
- The employer-visibility warning, stated plainly.

## How much the gateways actually agree

Vercel, OpenRouter and LiteLLM are all deliberately OpenAI-shaped: same
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

The practical read: text chat/coding/lite is near-free to port. Audio is the
exception, and it is the exception that pays for the feature.

## Traps

- **`image_gen.rs` posts to `/v1/ai/chat/completions`.** That works on
  Vercel, OpenRouter and LiteLLM; it does **not** work on raw OpenAI, where
  image generation lives at `/v1/images/generations`. Another reason the
  gateway is the paved road and raw provider keys are the narrow door.
- **A probe passes once; data corrupts forever.** Keep the runtime
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
  declined this twice and should keep declining.
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
  no home.
- **Does a failed BYO route degrade or stop?** Locked above as "stop, never
  silently fall back to the wallet" — worth re-testing against real user
  reaction, because the alternative is a broken app rather than a surprise
  charge.
