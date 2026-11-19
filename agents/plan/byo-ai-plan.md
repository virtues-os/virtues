# BYO AI plan

**Status:** Open. The route axis is not built, so BYO is still all-or-nothing:
every chat completion diverts if one credential exists. What remains is the
resolver and per-route audio encoder (Phase 2), many credentials (3b), the
"Test this slot" probes (4), and the per-slot route UI (6).

A second kind of credential, a ChatGPT plan reached through OpenAI's sign-in,
was spiked on 2026-11-18. It is proposed in its own section below, with nothing
built and its decisions open.

Shipped, and not re-described here: every chat completion forks at
`BearerClient` (no per-caller opt-in, no fallback to the wallet on failure),
and web search stays on the subscription;
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
| **Standard** | user-choosable *(already shipped)* | BYO | `slot_model_smoke` legs |
| **Deep** | user-choosable *(already shipped)* | BYO | `slot_model_smoke` legs |
| **Lite** | user-choosable *(already shipped)* | BYO | answers; no runaway reasoning tokens |
| **Image** | overridable (`image_model_id`); not yet a picker option | BYO | prompt → bytes decode as an image |
| **Omni** | **pinned** (the registry's Omni default, a Gemini flash) | BYO | transport only — reaches the id, accepts our audio shape |

The names are the registry's (`ModelSlot::all`). Standard was spelled `chat`
before 2026-10-05, and a saved credential may still carry that key.

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
| **Probe what varies, and nothing else** | Where the *model* varies because the user picked it (Standard, Deep, Lite, Image), test behavior. Where only the *route* varies and the model is pinned (Omni), test transport. A probe that tests more than what varies gives false confidence: a clean fixture clip would pass on models that fall apart on in-pocket muffled audio, which is the only case that matters. False confidence is worse than no probe. |
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

- BYO credentials become **many, not one**. `key = "__byo_ai_key__"`
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
  carries `lite_model_id`, `standard_model_id`, `deep_model_id`,
  `image_model_id`; five `*_route` columns (none exist yet) sit alongside them and skip a migration's worth of CRUD.
  A dedicated table for five fixed rows would be overengineering.
- Do **not** add `omni_model_id`.
- `SlotMap` grows nothing — Omni stays out of the cloud map by design.

### Phase 4 — The probes

The hard cases are already covered by tests. BYO should run those tests for
the user instead of expecting them to read a comment about `thought_signature`
and vercel/ai #11590. Surface as **"Test this slot,"** per slot, on demand and
on save.

- **Standard / Deep** — promote `slot_model_smoke` from an `#[ignore]`d test to
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

## Signing in with a ChatGPT plan (proposed, nothing built)

Nothing in this section is decided. On 2026-11-18 the owner asked whether a
person can bring their own OpenAI account, and a spike the same day showed the
mechanism works. This section records what was measured, proposes a design, and
lists the decisions that block it.

OpenAI calls the feature Sign in with ChatGPT, with an optional capability
named ChatGPT plan usage. It is a preview. The docs are at
`https://developers.openai.com/siwc/token-sharing-open-source`, and every page
there is also served as markdown at the same URL with `.md` appended.
Anthropic's terms forbid the equivalent for Claude plans, so this section
covers OpenAI only.

### What the app has today, the change, and why

**Today.** Bring your own AI is one form in Settings, under Billing: an
endpoint URL, an API key, and what the endpoint calls each model. A provider
bills an API key per token. A person who pays for ChatGPT Plus or Pro cannot
use that plan here. They would have to open an API account and pay a second
bill.

**The change.** A second way to fill the same setting: a button labeled
Continue with ChatGPT. The person signs in on OpenAI's page and allows Virtues
to use their plan. After that, chat and background writing are answered by the
models their plan includes and count against their plan's allowance.

**Why.** It moves the text share of the AI bill onto a plan the person already
pays for. It does not move audio, which this plan names as the largest line
item, so it adds to a key or the subscription and does not replace them.

**Who it serves.** The subscription stays the default, so nothing changes for
the narrative seeker or the quantified self unless they sign in. If they do,
replies come from a different model; tools, charts and structured output were
measured to work. The tech prosumer uses a plan they already pay for. The data
sovereign picks the vendor and the bill, and we make no privacy claim. For
someone in recovery, the usage-limit message carries no countdown, percent or
count.

### Measured against a live ChatGPT account, 2026-11-18

About twenty small requests from scripts kept outside the repo. The model slugs
are the ones the account listed that day.

| probe | result |
|---|---|
| sign-in with `client_id=dynamic_agent_client`, PKCE, and a callback on `http://127.0.0.1:1455/auth/callback` | A client id was issued with no waitlist, client secret or partner key. The request suggested the name Virtues, which the person can edit before approving. The plan-usage scope `chatgpt.tokens.use.direct` was granted. |
| `GET /v1/models` | 12 models under `models`, keyed by `slug`, 10 marked for display. Each entry lists the ChatGPT plans that may use it and its context window. |
| `POST /v1/responses`, plain text | 200 on `gpt-6.2-sol` and `gpt-6-luna`. Billed input tokens matched what was sent, so the route adds no prompt of its own. |
| function tools | 200 inside a `namespace` and at top level. The docs ask for namespaces. |
| a tool result sent back with the whole history in `input` | 200 and the right answer. `previous_response_id` is not allowed on this route and was not needed. |
| `text.format` with a JSON schema | 200, valid JSON |
| image input | 200 |
| OpenAI's hosted `web_search` tool | 200, with a citation |
| refresh grant | 200, a new access token and a replacement refresh token |
| `POST /v1/chat/completions` | 401 `rejected_by_access_enforcement` |
| `POST /v1/embeddings` | 401, the same |
| audio input | 400 "Audio input is not available", although the model's catalog entry lists audio as an input |
| `temperature` | 400 `{"detail":"Unsupported parameter: temperature"}`, a body with no `error` object |
| `max_output_tokens` | 200. The docs say to omit it. |
| `reasoning: {effort: "high", summary: "auto"}` | A reasoning item came back with encrypted content and an empty summary. |
| the same 5,776-token prefix sent twice | 0 cached tokens both times |

Not measured: exchanging the code from a different machine than the browser, a
usage-limit error, a long streamed turn, sign-in on a phone, and what the
server sees after a refresh token passes its 30 days. The docs list image
generation, audio and video input, file search and the Files API as
unsupported on this route.

### What the person sees

1. Bring your own AI offers two choices: Continue with ChatGPT, and the
   endpoint and key form that exists today.
2. The button opens OpenAI's sign-in page in the browser. The person signs in,
   names the app and approves.
3. Back in Virtues, one confirmation says that chat and background writing now
   use their ChatGPT plan. Below it are the models their plan offers, read
   from their account, with one picked for each of chat, deep research and
   background work. The first model in OpenAI's own order is picked to start
   with, and the person can change any of them. Nothing is typed.
4. Transcription and image generation carry on as they do today.
5. When the plan's allowance runs out, the reply says so in one sentence and
   links to ChatGPT's usage settings. Background writing waits and tries again
   later.
6. Disconnect, in the same place, ends the session with OpenAI and returns
   text work to the subscription.
7. If the session ends, the next reply says to sign in again. That happens
   when the person disconnects Virtues in ChatGPT's settings, or when the
   server has been off for more than 30 days. If OpenAI answers that the
   account or its workspace may not use plan usage, the page says so and does
   not send the person back through sign-in.

OpenAI's UI guidelines add requirements of their own: the button label, a
one-time confirmation after the first sign-in, the words "Using ChatGPT plan"
with a Manage usage link near the composer or the model selector, a Manage
usage link on our usage page, and a pricing page that says which of our plans
support it. Decision 4 below covers the one that conflicts with our own rules.

### What the server does

- **The server prepares and finishes the sign-in, and the app relays.** The
  server creates the attempt (state, nonce, PKCE verifier and its own host id)
  and returns the authorize URL. The app on the person's computer opens it,
  listens on `127.0.0.1` for the redirect, and posts the callback's `code`,
  `state` and `client_id` back. The server exchanges the code, verifies the ID
  token against OpenAI's published keys, and stores the tokens. The verifier
  and the tokens never pass through the app. Starting an attempt, finishing it
  and disconnecting each need owner proof, as saving and clearing a key do
  today. When the server and the browser are on one computer, the server
  listens on `127.0.0.1` itself.
- **A server with no app nearby uses the terminal.** `virtues` prints the URL.
  The person opens it on any computer, the redirect to `127.0.0.1` fails to
  load there, and they paste the address from the browser back into the
  terminal. It reaches the same server endpoint. OpenAI's docs describe a
  different procedure for a remote host: finish the sign-in on a local
  computer and copy the credential file across. Moving the code before the
  exchange keeps the tokens on the server, but it is our variant and it is not
  measured.
- **Tokens are one encrypted `credentials` row,** as the key is today. An
  access token lasts an hour. A refresh token lasts 30 days and is replaced on
  every refresh.
- **One process refreshes.** A refresh token works once. Applets and the
  transcription job run as separate processes that each build a
  `BearerClient`, so two of them refreshing at the same moment would spend the
  same token twice and end the session. The server refreshes under a lock, and
  every other process reads the current access token from the row.
- **Translation happens at the fork that exists.** `post_direct_upstream` and
  `stream_direct_upstream` in `virtues_api/client.rs` branch on the kind of
  credential. For a ChatGPT plan they turn the chat/completions body into a
  Responses body, send it to `/v1/responses` with `stream: true` and
  `store: false`, and turn the event stream back into the chat-completion
  chunks `agent/stream.rs` already reads. No caller changes. A non-streaming
  caller gets the stream collapsed into one chat-completion body.
- **What the translation covers.** System messages become `instructions`. Tool
  definitions lose their `function` wrapper. An assistant's tool calls become
  `function_call` items and tool results become `function_call_output` items.
  Image parts become `input_image`. `response_format` becomes `text.format`.
  `temperature` and `cache_control` are dropped. The reasoning item, with its
  encrypted content, rides in `reasoning_details` so the next step can send it
  back.
- **Routing is by slot.** This credential can serve the text slots (Standard,
  Deep and Lite) and nothing else. Under today's all-or-nothing fork it would
  also receive transcription and image generation, and refuse both. It needs
  the resolver from Phase 2: text slots go to the ChatGPT plan, and Image and
  Omni stay on the wallet, where web search already is. It is the first
  credential that cannot work without the route axis. A ChatGPT plan for text
  together with the person's own key for audio needs many credentials (Phase
  3b), because one credential is active at a time today.
- **Models come from the account.** The server reads `/v1/models` at sign-in
  and when the settings page opens, and the credential's slot map stores the
  person's picks. No model id is compiled in. Each entry carries a context
  window, which fills the number the key form asks the person to type.
- **Errors arrive in three shapes.** A request refused before it is admitted
  has `{"detail": "…"}`. One refused after has `{"error": {"code": …}}`. A
  failure after streaming starts is a `response.failed` event. Today a
  refusal from the owner's endpoint reaches the chat as the raw body, and
  `normalize_upstream_error` only rewrites a success status that carries
  `error`. The translation turns all three shapes into one sentence for the
  person. A usage limit is a 429 with
  `subscription_sharing_usage_limit_exceeded`. As with a key, a failed call
  never re-runs on the wallet.
- **Usage rows record tokens and no cost,** as for a key.

### How it fits the rest of this plan

- "Per-provider request translation" is listed under out of scope. This
  section proposes one exception to it (decision 2).
- The route axis becomes a prerequisite. A key can live without it; this
  credential cannot.
- A plan's models have not been through `slot_model_smoke`. Run it before the
  settings page recommends any of them.
- "Never claim BYO is more private" applies. The pages read do not say which
  data policy covers plan-usage requests, the person's ChatGPT data controls
  or the API's terms. Read that before writing the manual page or the entry in
  [`privacy-boundary.md`](../build/privacy-boundary.md), and until then say
  nothing about retention.
- The thinking display would be blank, because no summary text came back.
- No caching was measured, so a long chat may draw its whole history from the
  allowance again on every turn. A comment in `virtues_api/client.rs` records
  the chat's system prompt at about 15k tokens on 2026-09-24. Whether
  `prompt_cache_key` changes the caching is not measured.

### Slices

1. Sign-in on the server: the attempt, the exchange, ID-token verification,
   storage, the single refresher, disconnect, and the terminal path. This
   slice also answers whether the code can be exchanged from a different
   machine than the browser.
2. The translation, with fixture tests for the request and for the stream, and
   no network.
3. The resolver from Phase 2, with text slots on the ChatGPT plan and Image
   and Omni left where they are.
4. The settings page: the button, the confirmation, the model list, the
   usage-limit message, disconnect. The Mac app relays the callback.
5. The probes on a plan's models, then the manual page and the privacy
   boundary entry.

Slices 1 and 2 do not depend on each other. Nothing reaches a release before
decision 1 has an answer.

### Decisions that block it

1. **May a paid product use the self-serve sign-in?** OpenAI's docs describe
   it for "open-source and locally hosted apps" and send "paid or remotely
   hosted" apps to an interest form. Virtues runs on the person's own server
   and charges a subscription. Example: Nick pays for Virtues and for ChatGPT
   Plus, and signs in with ChatGPT on the server in his closet. Only OpenAI
   can say which path that is. Submit the interest form and ask, and ask in
   the same message whether unattended background jobs are allowed.
2. **Do we write one provider translation after refusing two?** This plan
   turned down Anthropic-native and Google-native request shapes. The
   Responses translation is the same kind of work for one vendor. Example: a
   later change adds a field to the chat body; with a translator, that field
   has to be mapped a second time or it silently stops reaching ChatGPT-plan
   users.
3. **Which work runs on the plan by default?** OpenAI's docs say a Plus
   plan's five-hour limit is shared across every app that uses the plan, and
   that the person can cap one app's weekly share in ChatGPT's settings.
   Example: David Okafor has Plus. If day summaries and applets run on his
   plan overnight, his own morning session in ChatGPT could start at the
   limit. The choices are chat only with background work as an opt-in, or all
   text work.
4. **Where do the words "Using ChatGPT plan" go?** OpenAI asks for them near
   the composer or the model selector. Our rule is that nothing a person looks
   at marks where work runs. Proposed: inside the model picker beside the
   model's name, which already names the vendor, and nowhere on the chat
   itself.
5. **Is sign-in from the Mac app and the terminal enough for a first
   version?** Example: a person with a server and only an iPhone would use the
   terminal path until sign-in on the phone is measured.

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

- **Only the chat-completions path diverts.** `diverts_to_byo` in
  `virtues_api/client.rs` matches `/v1/ai/chat/completions` and nothing else,
  because the owner's endpoint is one chat-completions URL. Web search
  (`/v1/ai/search`) stays on the subscription for that reason. A new route
  under `/v1/ai/` also stays on the subscription until someone adds it there,
  so adding one means deciding which side it belongs on and saying so on the
  settings page.
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
  refused this twice and should keep refusing. The ChatGPT plan section
  proposes one exception; this rule stands until the owner decides it.
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
