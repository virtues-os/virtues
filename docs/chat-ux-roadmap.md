# Chat UX Roadmap

Status: planning · Created 2026-06-25 · Branch `staging`

This is the master plan for the chat surface overhaul. It is organized into
**tracks (A–L)**. Each track is independently shippable and ends in a
**verification gate** — we stop, run the app, confirm it works, and only then
move to the next track. Nothing here is "big bang."

The headline going in: the architecture is already ahead of the typical 2026
"best practice" checklists. We have streamdown with incomplete-markdown
buffering, SSE via AI SDK v6, tool-calls-as-message-parts, token-based
server-side cancellation (already correct), pgvector embeddings, and a Yjs-CRDT
page-editing tool. This roadmap is **polish, coherence, and closing specific
gaps** — not catching up.

---

## Operating principles

1. **One track at a time, verify before advancing.** Each track below has an
   explicit "Verification gate." We do not start the next track until the
   current gate passes in the running app.
2. **Feel first.** The user's stated pain is "the overall UI/UX anim is the
   feel-bad part." Track A targets that directly and ships first.
3. **No hardcoded per-type colors.** Pills/badges use a type *icon* + one
   *semantic* theme accent. Arbitrary per-type colors don't compose across
   themes. (See `feedback_no_hardcoded_badge_colors`.)
4. **Reuse what exists.** We own the hard parts (CRDT, embeddings infra, media
   storage, cancellation, actions paradigm). Most tracks are wiring, not
   greenfield.

---

## Coverage matrix — every original ask maps to a track

| Original ask (from brief)                                   | Track |
| ----------------------------------------------------------- | ----- |
| Highlighting (select-to-reference)                          | D     |
| Bettering "add to space" options                            | K     |
| @ entity links / pills bettering                            | F     |
| Streamdown betterments                                      | A     |
| Add files/videos/audio + drag + model capabilities          | E     |
| Generating images and videos                                | E     |
| Fixing the "default name" of the agent                      | A     |
| System prompt management and observability                  | I     |
| Code executable                                             | H     |
| AI + chat editing a page together (anim & flow)             | G     |
| All chats "semantic" embedded for vector search             | B     |
| Making sure our tools work and are good                     | J     |
| Sweeping UI/UX changes incl. sidebar and window             | L     |
| Cancelling AI mid-run                                        | A*    |
| Querying/queueing messages before AI is done                | C     |

\* Cancellation is **already functionally complete** (partial message persists,
subagents abort, token cleaned up on all paths). Only a cosmetic "stopped"
marker remains — folded into Track A.

---

## Recommended sequence

```
A  (feel quick wins)         ← ship first, it's the named pain
B  (embed chats)             ← small, compounding; unblocks F's "@chat"
C  (queue while streaming)   ← highest interaction-feel win
D  (highlight-to-reference)  ← connective tissue across chat/space/page
E  (multimodal in/out)       ← large but high-value
F  (@ universal palette)     ← depends on B for @chat context
K  (spaces / add-to-space)   ← UX rework, pairs with F
J  (tools quality)           ← standardize rendering + approval + errors
I  (system-prompt observ.)   ← debuggability for everything above
G  (co-creation choreography)← the differentiator; do once feel is solid
H  (code execution)          ← new substrate (Podman); larger
L  (shell redesign)          ← ongoing, taste-driven; last + continuous
```

Hard dependencies: **F depends on B** (for `@chat` to inject context). Everything
else is independently orderable; the sequence above is by impact × readiness.

---

# Track A — "Feel" quick wins

**Goal:** Eliminate the named "feel-bad," all low-risk and pre-approved.

### A1. Re-enable tables
- **Current:** `controls={{ table: false }}` disables tables in
  [CitedMarkdown.svelte:93](../apps/web/src/lib/components/CitedMarkdown.svelte#L93)
  and [Markdown.svelte:56](../apps/web/src/lib/components/Markdown.svelte#L56).
  The model emits tables; we throw them away.
- **Change:** Add a custom table renderer snippet (same mechanism as the existing
  `link` and `inlineCitationPreview` snippets) — styled, horizontally scrollable,
  theme-aware. Re-enable for both components.

### A2. Layout-thrash sweep (the real "feel-bad")
- **Current root cause:** [chatInstances.svelte.ts:199](../apps/web/src/lib/stores/chatInstances.svelte.ts#L199)
  spreads `{ ...message }` on **every delta**, dirtying the entire message
  subtree per token (full reconciliation, not just the streaming text). Plus:
  zero `contain`/`content-visibility` anywhere, no `overflow-anchor`, and
  position-based part keys.
- **Change:**
  - Replace the per-delta object spread with a targeted mutation that updates
    only the streaming text/part, not the whole message object. (Investigate
    whether a `$state.raw` + explicit invalidation or a part-level reactive
    container removes the need to clone.)
  - Add CSS containment: `contain: content` on `.message-wrapper`
    ([ChatView.svelte:1434](../apps/web/src/lib/components/tabs/views/ChatView.svelte#L1434)),
    `contain: layout paint` on `.messages-container`, `overflow-anchor: auto`
    on `.chat-layout`, and `content-visibility: auto` +
    `contain-intrinsic-size` for off-screen messages.
  - Stabilize part keys (use `toolCallId` / a content-stable id instead of
    `text-${index}`).

### A3. Defer code-block highlighting until fence close
- **Current:** Shiki re-tokenizes the entire (growing) code block on every
  delta — O(n²) + flicker while the fence is open and the language is unknown.
- **Change:** Render code as plain monospace while the fence is open; swap to
  full Shiki highlight on closing fence / stream-end. Implemented via the custom
  code-renderer snippet (shares mechanism with A1). This is what ChatGPT/Claude
  effectively do — the highlight "settles" at the end.

### A4. Default agent identity → "Ari"
- **Current:** assistant name falls back to the bare string `"Assistant"`;
  `set_assistant_name` only runs in the (disabled) onboarding path; no UI to
  change it. (Backend: `app_assistant_profile.assistant_name`,
  [assistant_profile.rs](../virtues-core/src/api/assistant_profile.rs).)
- **Change:** Default to **"Ari"** in the registry/profile fallback (not a bare
  string), and add a field in user/agent settings to rename it. Name flows into
  `BASE_SYSTEM_PROMPT` as today.

### A5. Cosmetic "stopped" marker (the only cancellation remnant)
- **Current:** Cancellation is correct server-side — partial persists, subagents
  abort. But a truncated message on reload looks like the AI just trailed off.
- **Change:** Render a subtle "Stopped" affordance on messages whose turn ended
  via cancellation (needs a `finish_reason`/flag surfaced to the part).

### ✅ Verification gate A
- Tables render and scroll in a streamed reply.
- Stream a long reply with a fenced code block: no flicker, no scroll jump, CPU
  visibly lower (DevTools performance) than before; off-screen messages don't
  reflow.
- New chat: assistant identifies as "Ari"; rename in settings persists and the
  next reply respects it.
- Stop mid-stream → reload → message shows "Stopped," content intact.

---

# Track B — Semantic embedding of chats

**Goal:** Every Virtues chat becomes vector-searchable (today only *imported*
external chats are embedded).

- **Current:** Indexer only walks ontologies with `embedding: Some(...)`.
  `app_chat` is `embedding: None`
  ([ontologies.rs:619](../virtues-registry/src/ontologies.rs#L619));
  `app_chat_messages` has no ontology. `data_content_conversation` (imported
  Claude/ChatGPT history) *is* embedded — ours is not.
  [append_message](../virtues-core/src/api/chats.rs#L593) does not trigger
  embedding (batch-only, which is fine).
- **Change:**
  - Add an `OntologyDescriptor` for `app_chat_message` (and/or `app_chat`
    summaries) in [ontologies.rs](../virtues-registry/src/ontologies.rs) with
    `embedding: Some(EmbeddingConfig { embed_text_sql: "t.content", author_sql:
    "t.role", timestamp_sql: "t.created_at", preview_sql: ... })`. Decide the
    `record_id` scheme (`chat_id` + `message_id`).
  - Decide granularity: per-message vs. per-chat-summary (we already compute
    `conversation_summary`). Likely **both** — messages for recall, summary for
    chat-level retrieval.
  - Backfill existing rows via the indexer; confirm incremental pickup on the
    next cycle after `append_message`.
  - Filter out `subject='onboarding_synthetic'` and `role='checkpoint'` rows.
- **Doctrine:** follow the deterministic-id + confirmed-cursor discipline from
  `project_ios_delivery_durability` (no silent stranding).

### ✅ Verification gate B
- Send a few chats, wait one indexer cycle, confirm rows in `search_embeddings`/
  `search_vectors` for the new ontology.
- `semantic_search` (and the eventual `@chat` picker) returns a past chat by
  meaning, not keyword.
- Backfill populated historical chats; checkpoints/synthetic rows excluded.

---

# Track C — Queue / interrupt while streaming

**Goal:** Let the user type and submit while the AI is still going (the missing
core interaction). Cursor-style queued-message chips above the composer.

- **Current:** Composer disables send while streaming; no queue. (Cancellation
  plumbing already exists, which covers the "interrupt" variant.)
- **Change:**
  - **Queue (default):** composer stays live during stream; Enter enqueues; show
    queued message(s) as chips **above** `ChatInput`; drained automatically when
    the current turn completes (and editable/removable while queued).
  - **Interrupt-and-steer (modifier, e.g. ⌘↵):** cancel current run (reuse
    `/api/chat/cancel`) and immediately start a fresh turn with the new input.
  - Persist queue in the chat instance store so a tab switch doesn't lose it.
- **Files:** [ChatInput.svelte](../apps/web/src/lib/components/ChatInput.svelte),
  [ChatView.svelte](../apps/web/src/lib/components/tabs/views/ChatView.svelte),
  [chatInstances.svelte.ts](../apps/web/src/lib/stores/chatInstances.svelte.ts).

### ✅ Verification gate C
- Type + Enter mid-stream → chip appears above composer → sends automatically
  after the turn ends.
- Queue 2+ messages; they drain in order; one can be removed while queued.
- ⌘↵ mid-stream cancels and restarts with the new message; partial prior
  message persists (Track A5 marker shows).

---

# Track D — Highlight-to-reference

**Goal:** Select text in any message → act on it. Perplexity-style.

- **Current:** None.
- **Change:**
  - Selection in a message surfaces a floating action bar.
  - Primary action opens a **floating note textarea above the composer** (not
    inside the main `ChatInput`) to "comment about" the selection — the quoted
    context rides along on submit.
  - **Multi-highlight:** allow several active highlights at once; each gets a
    distinct accent **derived from the active theme** (not arbitrary), so they
    remain visually distinct but on-theme.
  - Actions: *Quote in reply*, *Ask about this*, ***Add to page*** (reuse the
    `edit_page`/page infra), *Save*.
  - Highlights are anchored to message + offset range; survive re-render.
- **Ties into F:** "Add to page" and quoting share the reference plumbing.

### ✅ Verification gate D
- Select text → action bar → note textarea opens above composer → submit carries
  the quote.
- Two highlights show two theme-derived accents simultaneously.
- "Add to page" inserts the selection into a chosen page.
- Highlights persist across a streamed reply / scroll / re-render.

---

# Track E — Multimodal input & output

**Goal:** Attach files/images/audio/video to chat; generate images (later
video); never silently fail on a model that can't handle the input.

- **Current:** Real media backend exists
  ([media.rs](../virtues-core/src/api/media.rs): content-addressed, dedup,
  100MB, image/video/audio MIME allow-list) but it's wired to **pages, not chat
  messages**. `UIPart` has only Text/Reasoning/ToolInvocation/WebSearch/
  Checkpoint — no image/file part. Capability detection is implicit (model name
  → provider).
- **Change:**
  - **Input:** `+` button in `ChatInput` and **drag-drop onto the chat pane with
    visible drop feedback**. Preview thumbnails for attachments. Use AI SDK
    `experimental_attachments` (FileList → data-URL) or upload via `media.rs`
    and pass URLs.
  - **`UIPart`:** add `File`/`Image`/`Audio` variants (backend
    [chat.rs](../virtues-core/src/api/chat.rs) + frontend renderers) so a message
    can *carry* media, not just store it.
  - **Model capabilities (the gate):** add explicit
    `supports_vision/audio/pdf/image_gen` flags to `virtues_registry::models`.
    `ChatInput` reads the **active** model's caps to (a) allow/deny the
    attachment, (b) prompt "this model can't see images — switch to X?", or
    (c) auto-route to a capable model. No silent drops.
  - **Image generation:** new `generate_image` action (AI SDK 6 supports image
    gen + edit). Output stored via `media.rs`, rendered as a media `UIPart`.
  - **Video generation:** same shape but long-running — model as an action
    returning a job handle that streams progress via the existing `data-*`
    event pattern (reuse subagent-status mechanism).

### ✅ Verification gate E
- `+` and drag-drop attach an image; thumbnail preview; drop feedback visible.
- Attaching to a vision model works; attaching to a text-only model gives the
  capability prompt/route, not silence.
- `generate_image` returns an image rendered inline and persisted.
- (If included this pass) video gen shows progress and resolves to a playable
  asset.

---

# Track F — @ as a universal reference palette

**Goal:** `@` links to *anything addressable*, not just entities.

- **Current:** Solid entity pipeline (EntityPicker → chip → `[name](url)` →
  EntityChip). Gaps: chips silently degrade to text when edited; no in-typing
  indicator; entities only.
- **Change:**
  - One **@ palette** with type-grouped sections: **Entities**
    (people/places/orgs/things), **Pages**, **Chats**, **Spaces**, **Files/
    media**; stretch: a **specific message** or a **day**.
  - Each result → typed pill: distinguishing **icon** + **one semantic accent**
    (NOT per-type hardcoded colors — `feedback_no_hardcoded_badge_colors`).
  - Pills are **atomic / non-editable** inline tokens so they can't silently
    degrade (fixes today's bug).
  - Extend `parseEntityRoute(url)` to route page/chat/space/media URLs.
  - **Behavior:** `@chat` / `@space` inject that thing's context (summary +
    vectors) into the prompt — **depends on Track B**.
- **Files:** [EntityPicker.svelte](../apps/web/src/lib/components/EntityPicker.svelte),
  [EntityChip.svelte](../apps/web/src/lib/components/EntityChip.svelte),
  [ChatInput.svelte](../apps/web/src/lib/components/ChatInput.svelte),
  backend route resolution.

### ✅ Verification gate F
- `@` shows grouped Entities/Pages/Chats/Spaces/Files.
- Each pill has a type icon + on-theme accent; pills can't be edited into plain
  text.
- `@`-ing a past chat measurably injects its context (assistant references it).

---

# Track G — Co-creation choreography (AI + you edit a page together)

**Goal:** The differentiator — watch the AI edit a page in the adjacent pane in
real time, visibly, with undo. Not hidden changes.

- **Current (we own ~90%):** `edit_page` over **Yjs CRDT**, `EditDiffCard` and
  `PageEditResult` renderers, and **split panes** already exist
  ([SplitContainer.svelte](../apps/web/src/lib/components/tabs/SplitContainer.svelte)).
- **Step 1 — simplify first (per request):** Audit the overlap between the
  in-chat `EditDiffCard` and the in-doc rendering. Decide the single source of
  truth for an edit's lifecycle (proposed → streaming → applied/rejected) before
  adding choreography. Remove redundant representations.
- **Step 2 — choreography:**
  - On `edit_page`, auto-open the target page in the adjacent split pane.
  - **Stream the diff into the document in place** (slightly *delayed* vs. raw
    token stream so it reads as deliberate), with multiplayer-style cursor/caret
    presence showing where the AI is writing.
  - Per-edit **accept/reject at the edit site**, and **undo**.
  - Elegant motion — the "feels good" bar. This is an animation/orchestration
    project, not an architecture one.

### ✅ Verification gate G
- Ask the AI to edit a page → page opens in the other pane → edits animate in
  visibly with an AI caret → accept/reject per edit → undo restores.
- Only one edit representation remains (no duplicate diff card + doc state
  drift).

---

# Track H — Code execution

**Goal:** Real code execution that fits the self-hosted appliance.

- **Current:** `code_interpreter` tool + `CodeInterpreterCard` renderer exist;
  execution substrate unclear/unspecified.
- **Reference (what others do):** Claude Code runs on your machine via Bash, no
  sandbox (doesn't transfer to a hosted agent). ChatGPT Code Interpreter =
  network-isolated gVisor container, per-session FS. Claude analysis tool = JS
  in a browser Web Worker.
- **Change (recommended):** **Rootless Podman container as an action** — fits the
  Podman/Compose substrate and single-tenant appliance (blast radius = the
  user's own box, so a far simpler threat model than multi-tenant SaaS).
  - Ephemeral container per call; **network off by default**; CPU/mem/wall-time
    limits; scratch mount; capture stdout/stderr + artifacts → media parts.
  - Reuse the action stdin/stdout contract.
  - Later: persistent-kernel "session" mode (Jupyter-style) for iterative data
    analysis → maps onto `CodeInterpreterCard`.
  - Optional: expose the Jetson GPU to the sandbox for compute
    (see `project_jetson_gpu_group_access` for group plumbing).

### ✅ Verification gate H
- Agent runs code in a Podman sandbox; output + any artifact return and render.
- Network is off by default; resource limits enforced; container is ephemeral.

---

# Track I — System-prompt management & observability

**Goal:** Make the ~10-source assembled prompt inspectable instead of a black box.

- **Current:** Prompt assembled across
  [prompt.rs](../virtues-core/src/agent/prompt.rs) +
  `build_system_prompt()` in [chat.rs](../virtues-core/src/api/chat.rs) from
  identity, persona, narrative, memory, datetime, user context, space, page,
  etc. Personas have CRUD ([personas.rs](../virtues-core/src/api/personas.rs)).
  Usage is persisted per message. The fully-assembled prompt is **not** visible
  anywhere.
- **Change:**
  - Capture the fully-assembled system prompt per turn; expose a dev/
    observability panel ("show exactly what the model saw" — sections + final
    string + token counts).
  - Surface prompt-assembly metadata alongside the existing per-message usage.
  - Management UI for the persona/memory inputs that feed assembly.

### ✅ Verification gate I
- For any turn, open the panel and see the exact assembled prompt, by section,
  with token counts, matching what was sent.

---

# Track J — Tools quality

**Goal:** Every tool feels first-class: consistent rendering, approval where
needed, actionable errors.

- **Current:** Clean registry with mode-based filtering
  ([tools/mod.rs](../virtues-core/src/tools/mod.rs)), per-tool timeouts. But
  rendering is uneven — some tools have bespoke cards (`create_page`,
  `edit_page`, `code_interpreter`), generic ones get a thin row; errors are raw
  red text; only some tools gate via `PageBindingInline`.
- **Change:**
  - **Standardize a tool-result component contract** so every tool gets a
    coherent header/status/body/expand, with bespoke bodies as an opt-in.
  - **Human-in-the-loop approval** generalized from `PageBindingInline` (AI SDK 6
    has tool-execution approval) — any tool can request Allow/Deny before
    running.
  - **Actionable errors** — replace red-text dumps with cause + suggested action
    + retry.
  - Audit each registered tool for "does it actually work and return useful,
    citable output" (web_search, sql_query, semantic_search, edit_page,
    code_interpreter, dispatch_subagents, memory/naming tools).

### ✅ Verification gate J
- Every tool renders through the standard contract.
- A gated tool prompts Allow/Deny and respects the choice.
- A forced tool error shows cause + action + retry, not raw red text.

---

# Track K — Spaces & "add to space"

**Goal:** Fix the "weird/bad" top-left add-to-space UX. Spaces = folders/projects.

- **Current:** `ChatSpaceBreadcrumb` pill dropdown (search/create/checkmarks/
  remove) works, but filing is undiscoverable and the top-left affordance reads
  as odd. Sidebar `SpacesSection` lists spaces with counts.
- **Change:**
  - Rethink the chat→space affordance so filing is obvious and low-friction
    (clear "in [Space]" state; obvious move/file action).
  - **Suggested filing:** the agent already gets space context — have it propose
    "File this in [Space]?" when a chat clearly belongs somewhere.
  - **Drag a chat tab onto a space** in the sidebar.
  - **Multi-select / move-many** from the sidebar.
  - Clarify the Space mental model in UI (folder/project, not "room").
- **Files:** [ChatSpaceBreadcrumb.svelte](../apps/web/src/lib/components/chat/ChatSpaceBreadcrumb.svelte),
  [SpacesSection.svelte](../apps/web/src/lib/components/sidebar/SpacesSection.svelte),
  [space.svelte.ts](../apps/web/src/lib/stores/space.svelte.ts).

### ✅ Verification gate K
- Filing a chat into a space is obvious in < 2s for a new user (self-check).
- Drag a chat tab onto a space files it; multi-select moves several at once.
- Agent suggests a space when appropriate; accepting files the chat.

---

# Track L — Shell redesign (sidebar, window, tabs, overall feel)

**Goal:** Lean into the IDE/Arc model (not a chat app) and fix the remaining
"feel-bad" in motion and navigation. Taste-driven, partly continuous.

- **Current:** Ambitious shell — `UnifiedSidebar` (workspace/pinned/spaces/
  system/search/footer), `WindowTabBar` (DnD reorder, rename, icon picker),
  `SplitContainer` two-pane, multiple tab types (chat/page/wiki/space/system).
  More IDE/Arc than chat app already.
- **Change (to be specified with the user — needs taste input):**
  - Identify what *feels* wrong: density, navigation depth, the date-tab concept,
    split ergonomics, transitions.
  - Systematize animation/motion across the shell so it "feels good" (the
    recurring complaint). Likely a shared motion language reused by Tracks C/D/G.
  - Treated as a **separate, continuous track** layered after the conversation
    work lands, because much of the felt quality comes from A/C/D/G first.

### ✅ Verification gate L
- Per-iteration: the specific feel issue named at the start of that iteration is
  visibly resolved in the running app. (This track iterates rather than
  "completes.")

---

## Notes / open decisions to resolve in-track

- **B granularity:** per-message vs. per-summary embeddings (lean: both).
- **C interrupt keybind:** confirm ⌘↵ vs. another chord.
- **D anchoring:** message+offset range storage format for highlights.
- **E scope split:** ship image-gen this pass; video-gen may be its own follow.
- **G simplification:** decide single edit-lifecycle source of truth before
  choreography.
- **L:** requires a dedicated working session to enumerate concrete feel issues.
