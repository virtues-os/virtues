# Getting started — one room

> After the founder's letter, getting started is a single chat. It opens
> already speaking, it carries four steps as cards, and the app around it
> stays shut until the box can call a model. Written 2026-09-08. Supersedes
> the seven-step Home page ([archived](../archive/getting-started-page-plan.md)).
>
> Setup (pairing, the airlock, claiming the box) is still
> [onboarding-plan.md](onboarding-plan.md) and stays untouched. This plan
> begins where the SPA opens.

## Why

**The page paradigm regrew the thing it replaced.** The Home first-run state
shipped 2026-08-31 as sections that retire, was struck the same day as "Home
with getting started slapped on", and became a numbered seven-step page
(`home/GettingStarted.svelte`, 534 lines, plus `IntroductionsCard`, plus the
focus/settled/loading phases HomeView reads). It is a wizard with a serif. The
list has already grown from four asks to seven rows.

**The product's first conversation already lives in a room that the id
governs.** The narrative interview is a seeded chat (`prod_seed.rs`), forced
into its mode server-side by chat id (`chat.rs` ~1098: the client cannot opt
it into tools), with a one-tool allowlist (`tools/mod.rs`
`get_tools_for_agent_mode`), a synthetic never-persisted opening
(`ChatView.svelte` `applyInterviewOpening`), and its own closed-state card
replacing the composer (`InterviewClosedCard`). Every mechanism a getting
started room needs exists once. This plan uses it a second time.

**The server already derives the state; the page stored a copy.** `GET
/api/setup/state` (`box_status.rs::compute_setup_state`) computes account,
first source, device collecting, first sync, remote access and
`narrative_identity_ready` from rows. The only stored progress is the
dismissed list (`app_user_profile.getting_started_dismissed`, migration 0014)
and `onboarding_status`. That is the right split and it stays.

## Decisions

- **One room, seeded at boot: `chat_getting_started`.** Title "Getting
  started". Undeletable and un-retitled by id, exactly as the interview is
  (`chats.rs::delete_chat`, `generate_title`). It is the first thing the app
  opens after the letter and the only thing reachable until step 1 is done.
- **Four steps. That is the ceiling.** Connect AI · Introductions · Connect
  your world · In your own words. One promise card after them ("your first
  day, written up") that is not a step. No "go further" row; applets and the
  manual are the app's business once the door opens.
- **Steps are derived, never stored.** Done means a row exists. Skips are the
  only stored state, in the existing dismissed list. There is no progress
  pointer, no `current_step`, nothing the model can advance.
- **The model is a guest who arrives after step 1.** Before the box can call
  a model the room is written entirely by the box: synthetic turns and cards,
  no model call, exactly as the interview's opening is delivered today. After
  step 1 the model speaks in a `getting_started` mode with a short allowlist.
- **Secrets never enter the transcript.** Sign-in, the BYO key, OAuth, pairing
  are cards that talk to their existing endpoints. No tool takes a credential
  as an argument. The composer refuses to send a message while the room is
  showing a card that wants a secret (the card says so).
- **The interview stays its own room.** Step 4 is a hand-off card into
  `chat_narrative_interview`. The interviewer is a witness with zero tools and
  the drafter treats the whole transcript as material; an operator chat with
  tools cannot share that transcript. Done = `narrative_identity_ready`.
- **The lock keys on "can this box call a model", not "has an account".**
  DIY boxes are exempt from the account requirement (`compute_setup_state`,
  `requires_account`). Connect AI is satisfied by a linked subscription OR an
  active BYO route (`settings_byo::byo_is_active`). The verdict is computed
  server-side so the paired phone, which loads the box's SPA, gets the same
  answer as the desktop.
- **One door, two labels.** An icon top right of the room. Before step 1 it
  is hidden-ish and reads "dangerously skip onboarding"; it calls the
  existing `POST /api/setup/skip-onboarding`, and `/dangerously-skip-onboarding`
  typed in the composer does the same. After step 1 the app is open anyway,
  so the same door reads "come back to this later" and opens Home. The pause
  feature and the exit are one control. "Onboarding" left the visible
  vocabulary on 2026-08-31; the one place the word survives is on a door
  nobody is meant to want.
- **Everything is in the chat, the phone included.** Pairing a phone is a
  card in step 3 that takes the same shape pairing has elsewhere (the pair
  modal's QR hand-off). On the phone itself, the native permissions flow
  (`MobileOnboarding.svelte`, Tauri plugin invokes) becomes a card in the
  same room rather than a screen mounted before it.
- **Introductions is one prompt, not a form.** A single textarea. The model,
  connected by then, extracts the four facts and plays them back in a
  confirmation card; the card writes the profile, never the model's word.
- **No new flag for the lock.** The skip sets `onboarding_status` to
  `active`, which also bypasses the letter on later visits. Accepted (Adam
  2026-09-08).
- **The first model turn is authored.** A fixed line the room shows when
  step 1 flips, no model call; the model speaks on the person's first reply.
  A server-initiated turn (a kick with no user message) is not built and not
  needed to ship.
- **Synthetic turns are never persisted.** The room re-renders the present
  state at the top on every load. Only the person's messages and the model's
  replies are stored. A person returning after a week sees today's state,
  not a replay of cards that no longer apply.
- **No trait inference, no machine-written identity.** Unchanged doctrine
  ([narrative-identity.md](../build/narrative-identity.md)). This room asks
  for facts the record cannot supply and connects things. It never describes
  the person.

## The room, top to bottom

What the person sees on opening, in order. Everything above the first stored
message is synthetic.

**The mast.** "Getting started", one standfirst sentence in the letter's
voice: the server keeps the record; these four things are what it cannot do
for itself. No count, no progress bar. The four steps as a short list with a
mark against each that is done, rendered from the endpoint below. The door
sits at the mast's right (see Decisions: one door, two labels).

**Step 1 — Connect AI.** Card with two doors: the Virtues subscription
(`AccountGate.svelte` ported: `setupSubscribeStart`, `setupLoginStart`,
`setupLinkPoll`) and bring-your-own (the BYO form from Settings, posting to
`settings_byo::save_handler`). Done when `subscription.linked` or
`byo_is_active`. Not skippable in the room; the slash command is the skip.
While this card is open the composer is inert and says why in one line: the
room can talk once it has something to talk with.

**Step 2 — Introductions.** One textarea under one ask: what to call you,
what you will call it, where home is, and when you were born, in whatever
words; with a clause that the story of your life comes later, in its own
conversation, so this stays to the facts. The text is sent as an ordinary
turn; the model answers with a `record_introductions` call whose output the
client renders as a confirmation card (name, assistant name, time zone
resolved from the place, birth date), each field editable, one button. The
card writes through `updateProfile` and `updateAssistantProfile` exactly as
the old `IntroductionsCard` did; the tool itself writes nothing. Done when
`preferred_name` is set. Skippable. If the model cannot resolve a field the
card shows it empty rather than guessed.

**Step 3 — Connect your world.** `ConnectWorld.svelte` ported as a card:
sources, this Mac, and the phone. The phone is its own card in the same
shape pairing has elsewhere (QR hand-off from the pair modal); on the phone,
the card is the native permissions flow that `MobileOnboarding` runs today,
invoked from the room instead of mounted before it. Done when `first_source`
or `device_collecting` (the existing `worldEnough` rule; a collector with a
denied permission outranks the check, as today). Skippable: sources stay in
Settings.

**Step 4 — In your own words.** Hand-off card: what the interview is, about
twenty minutes, one button into the interview room. "Underway" once
`interview_started`, done on `narrative_identity_ready`. Skippable.

**The promise.** One line, not a step: your first day is written overnight
from what your sources hold. Appears once step 3 is done, becomes "Read
<date>" when `first_day` lands (census), and is not dismissible because it
retires itself.

**Below the line: the conversation.** Stored messages, the model's turns, in
the real ChatView. The model can be asked anything about the four steps; it
can open a card, skip a step, record introductions, and answer why. It cannot
mark anything done.

**The sidebar card.** Not a line item. A colored progress card at the
bottom of the sidebar, above Sources: title, the four steps as marks, the
open count, and it opens the room. It exists from step 1 to graduation and
is the only place outside the room that mentions getting started. Home
appears in the sidebar the moment step 1 is done; it is Home with no
getting-started furniture at all.

**Graduation.** When every step is done or skipped, the sidebar card goes,
the mast collapses to one line, and the room becomes an ordinary chat listed
under Chats by its title.

## Server

**`GET /api/getting-started`** — the one derived truth. Authenticated.

```json
{
  "ai_connected": true,
  "locked": false,
  "steps": [
    { "id": "connect_ai", "status": "done", "via": "subscription" },
    { "id": "introductions", "status": "open" },
    { "id": "connect_world", "status": "skipped", "detail": "…" },
    { "id": "interview", "status": "open", "underway": true }
  ],
  "first_day": null,
  "graduated": false
}
```

Computed from `compute_setup_state` plus `byo_is_active`, `preferred_name`,
the dismissed list, `interview_started`, and the census's `first_day`. `status`
is `done` | `open` | `skipped`; done wins over skipped (a skipped step whose
row later appears reads done). `locked` = `!ai_connected && onboarding_status
!= 'active'`, so the slash command unlocks by the path that already exists.
Query errors surface with `?`, never as a plausible false (CLAUDE.md: a
swallowed `first_source` query once read zero forever).

**`POST /api/getting-started/skip`** `{ "step": "introductions" }` — appends
to the dismissed list; `{ "step": …, "skipped": false }` removes. The `further`
and `first_day` ids die with the page.

**Seed.** `prod_seed.rs` inserts `chat_getting_started` beside the interview
chat. `GETTING_STARTED_CHAT_ID` lives next to `INTERVIEW_CHAT_ID`.

**Mode.** `chat.rs` chat_handler forces `agent_mode = "getting_started"` by
id. `build_system_prompt` branches as it does for the interview: a standalone
`GETTING_STARTED_PROMPT` in `prompt.rs` (character, the four steps and what
each is for, the secrets rule stated to the model, the conduct: short turns,
never nag, never claim a step is done) plus one injected block rendering the
endpoint's state as text, the way the interview prompt injects the reply
count. If `!ai_connected` the handler refuses the turn with a plain sentence;
the client never sends one, but no client can be trusted to.

**Tools** (`tools/mod.rs` allowlist `"getting_started"`, definitions in
`virtues-registry/src/tools.rs`, execution in `tools/executor.rs`):

| Tool | Does | Never |
|---|---|---|
| `show_step` `{step}` | returns a card marker the client renders as that step's card, opened | opens a card for a done step |
| `skip_step` `{step}` | the same write as the endpoint; refused for `connect_ai` | skips silently: the reply says it |
| `record_introductions` `{preferred_name?, assistant_name?, home_timezone?, birth_date?}` | returns the fields as a marker the client renders as the confirmation card; the card writes | writes anything itself; fills a field it could not resolve |

No search, no data, no pages, no applets. The room is about the box, not the
record. `show_step` follows the `permission_needed` card pattern
(`PageBindingInline`): the tool's output is a marker, the UI is the client's.

**Lock, server side.** Nothing new. Chat turns already fail without a route;
the endpoint's `locked` is what the client reads. No API is walled: a locked
app is a client posture, and the CLI and a paired phone keep working.

## Client

**Route guard** (`(app)/+layout.ts`): the existing `/founders-letter`
redirect stays. After it, one fetch of `/api/getting-started`; if `locked`,
every route but `/chat/chat_getting_started` redirects there. The
`onboarding_status === 'active'` arm already exempts a skipped box.

**Sidebar while locked:** the workspace header and the one room. No Desk, no
Library, no Settings, no search. Once `ai_connected`, the full sidebar plus
the progress card (`sidebar/GettingStartedCard.svelte`) above Sources until
graduation. `UnifiedSidebar` gets a `locked` prop and the card; nothing else
changes shape. The phone's drawer (`MobileDrawer.svelte`) gets the same two
states.

**The door** is one component in the room's mast, reading `ai_connected`
for its label and target. Before step 1: the skip. After: Home.

**The room** is `ChatView` with `GETTING_STARTED_CHAT_ID` treated as the
interview is: `applyGettingStartedOpening` prepends the mast and the cards as
synthetic parts from the endpoint's state (re-fetched on every load and after
every card action), and the four cards are one component each:
`gettingstarted/ConnectAiCard`, `IntroductionsCard` (moved from `home/`),
`ConnectWorldCard` (wrapping `ConnectWorld.svelte`), `InterviewCard`. A
`tool-show_step` part renders the same card, opened.

**Composer:** inert with one line while `locked`. A slash registry in
`ChatInput.svelte`'s submit path, one entry to start: `/dangerously-skip-onboarding`
does what the door does. The registry is client-side
and deterministic; it is not the removed "write it up" phrase intercept (a
phrase the model was meant to act on), it is a command the model never sees.
Unknown slash commands send as text.

**Deleted:** `home/GettingStarted.svelte`, `home/IntroductionsCard.svelte`
(moved), the `phase` prop and its three states in `HomeView.svelte`,
`setupState.svelte.ts`'s getting-started readers (`accountSatisfied`,
`worldEnough`, `interviewStarted` move behind the new endpoint; the
remote-access flip toast keeps its poll), and the `further` / `first_day`
dismiss ids.

## Leaving and coming back

Progress is derived, so leaving costs nothing and there is nothing to
"pause". The button's job is permission and a promise, not state.

- **Before step 1 there is no door but the ugly one.** With no model the app
  has nothing to show; the only exit is `/dangerously-skip-onboarding`.
- **After step 1 the app is open**, because the lock keys on AI alone. The
  door's label turns to "Come back to this later"; it opens Home and says in
  one sentence what happens meanwhile: the progress card stays in the
  sidebar, and nothing here expires. That door is the whole pause feature.
- **Nothing nags from Home.** A "3 things left" chip on Home is the exact
  furniture struck on 2026-08-31. The sidebar card is the reminder; the
  first-day promise is the reason to return.
- **Re-entry re-renders the present.** Done cards do not re-render; the mast
  lists them as done and the first open card is the one that opens. A person
  back after a week sees today, not a replay.

## Modules

The cards are modules with one contract, so adding one is a registry entry,
not a page rewrite. Each module declares:

| Field | Where it lives |
|---|---|
| `id`, title, one-line what | server registry (the endpoint's rows) |
| `done` predicate | server, computed from rows |
| skippable | server |
| card component | client, one file under `gettingstarted/` |
| prompt blurb | server, what the model may say about it |
| **payoff** | client, the plate shown once `done` flips |

The payoff is the rule that keeps this from being a checklist: every module
ends with something drawn from the record, never a checkmark. Connect AI
ends with the model's first real turn, which does something (names what the
box already holds, or if it holds nothing, says what tomorrow brings).
Introductions ends with the assistant answering to its name. Connect your
world ends with the lifeline plate lighting its first lane (`first_seen` from
`GET /api/wiki/lifeline`, the "nothing watching" vs "nothing happened" ink).
The interview ends with the same plate carrying their chapters instead of the
example's. Graphics are plates drawn from the record, per
[design-grammar.md](../build/design-grammar.md); no illustration that is not
about them.

The module list is still capped at four. A registry makes a fifth cheap,
which is the reason to write the cap down here.

## Compaction

The room is long-lived and the model's context is not the transcript.

- **The model reads the state block plus recent turns.** Never the whole
  room. The state block is regenerated per turn from the endpoint, so a
  compacted history loses nothing the model needs.
- **The synthetic mast replaces history.** Done cards leave the room; the
  mast is the record of them. `CompactionCheckpoint` applies to the
  conversation below the line exactly as in any chat.
- **After graduation the room is an ordinary chat** and compacts on the
  ordinary schedule. Its title stays.

## What 2026 practice says, and where we differ

The current literature on AI-led onboarding converges on a few things, most
of which this plan already does, and one it refuses.

- **A real conversation within a minute, and a result before understanding.**
  Get the person doing something useful before they know the product. Ours:
  the model's first turn after step 1 is a real turn, not a tour.
- **Conversation replaces forms only where a form is worse.** Open-ended
  intake is where dialogue wins; facts (name, time zone, keys) are cards. The
  interview is the open-ended part and it already has its own room.
- **The agent completes setup on the person's behalf where it can.** Ours can
  open cards, skip, and record introductions. It cannot do OAuth or pairing,
  and must never appear to.
- **Progressive, not front-loaded.** Sources can be added any day; the room
  never demands completeness. Continuous onboarding is what Home and the
  daily write-up are for.
- **Re-engage on stated intent within a day or three.** Ours is the first-day
  promise: the record itself comes back with something, at the maintenance
  hour, without a notification campaign.
- **Close the loop: the conversation becomes structured data.** Ours does by
  construction: introductions to the profile, the interview to the document
  and `wiki_chapters`. The room's own chatter is not mined.
- **Where we differ: enrichment.** The industry's favourite lever is
  personalising the first message from the signup email (company, industry,
  stack). That is inference about the person from data, which this product
  forbids ([narrative-identity.md](../build/narrative-identity.md)). The room
  knows what the box holds and nothing about who they are until they say.

## Where this stands (2026-09-13)

Steps 1–6 of the build order are BUILT on `wave` (see the commit log for
`getting_started`). Three deviations from the text above, all deliberate:

- **The lock's stored bit is the `connect_ai` skip in the dismissed list**,
  not `onboarding_status`. The letter's exit already sets `onboarding_status`
  to `active` for everyone, so it cannot distinguish "read the letter" from
  "used the door". `locked = !ai_connected && !dismissed('connect_ai')`; the
  door and the slash command write that one skip. No new column.
- **BYO stays in Billing for now.** Its form is sudo-gated and another agent
  is mid-change on it (`58d320bd`), so the Connect AI card links to
  `/virtues/billing`, which the lock allows. Extracting it into the card is
  the one open item in step 3.
- **The phone's drawer is not yet locked.** The route guard locks the phone
  too (it loads this SPA), but `MobileDrawer` still lists every chat while
  locked. Step 2's last item.

Step 7 (payoffs) is partly ahead of schedule: `ChapterLifelineLive` already
draws the person's chapters (`7146459b`); the lane-lighting plate for
step 3 is not built. Step 8 (docs) waits for a release.

## Gaps found against the code (2026-09-08)

Each is a build item below; none needs a migration.

- No "no model" refusal exists: a turn with neither subscription nor BYO
  fails inside the gateway call with a generic error. The handler checks
  `ai_connected` first and answers in one sentence.
- The letter exits to `/home` (`founders-letter/+page.svelte`); it must exit
  to the room, and the app's default route must too while locked.
- ChatView renders only text, file and `tool-*` parts; cards need one
  template branch keyed on a synthetic message-id prefix (`gs-card-`), the
  way the interview slots its plate by message id.
- The chat list has no pin and no subtitle, which is why the progress card
  is a sidebar card and not a list row.
- BYO has no component; its form is inline in `SettingsView.svelte` and is
  extracted first.
- Nothing runs a model turn without a user message, which is why the first
  turn is authored.
- The dev skip (`VIRTUES_DEV_SKIP_SETUP`) marks setup done but nothing marks
  AI connected; the endpoint's dev arm must, or every checkout opens locked.
- Context assembly builds from stored messages; the mode needs its own
  branch beside the interview's (`chat.rs::build_system_prompt`).
- `ChapterLifeline` is fictional constants; the payoffs need it generalised
  to `GET /api/wiki/lifeline` and `GET /api/wiki/chapters`. The one piece of
  the plan that is not plumbing.
- Subscription sign-in needs the cloud; a LAN-only box passes step 1 via a
  local BYO endpoint or the door. Accepted.

## Build order

1. **Endpoint + seed + skip.** `GET /api/getting-started`, the chat row, the
   step skip. Tests: derived statuses per row state; done beats skipped;
   `locked` false on a DIY box with BYO active; false after the slash skip.
2. **Route guard + sidebar lock + composer inert.** With step 1 alone the
   app is already a room and a door. Verify on the dev box with
   `VIRTUES_DEV_SKIP_SETUP` unset and the BYO row deleted.
3. **The cards and the door.** Connect AI (BYO extracted from Settings),
   Connect your world with the phone card, the interview hand-off, the
   authored first line, the door with its two labels, the sidebar progress
   card. All working with no model in the loop except Introductions. This is
   the whole product for a box with no AI, so it is complete before the
   model is invited.
4. **Mode + prompt + tools + the no-model refusal.** `getting_started` mode,
   the prompt, the three tools, the state block, the context branch.
   Introductions lands here (it needs the model). Test: a tool call for a
   done step is refused; the prompt never claims a step done that the
   endpoint says is open; a turn with no route answers in one sentence.
5. **Slash registry.** One command. Test the unknown-command passthrough.
6. **Delete the page.** Home loses its phases, the letter exits to the room,
   `MobileOnboarding` becomes the phone card. Remove the dead ids.
7. **Payoffs.** The lifeline plate generalised to real lanes and chapters.
   Last, because everything above ships without it.
8. **Docs.** `docs/setup` gets the room described as it ships;
   [onboarding.md](../build/onboarding.md) "Setup vs onboarding" points here.

Each step lands green on `wave`; one PR at the end.

## Traps

- **The chat-as-wizard failure.** A form pretending to be a conversation.
  Cards for every action, prose only where the model has something to say,
  and the model forbidden from asking for anything a card collects better.
- **A key in the transcript.** If someone pastes one anyway the message
  still reaches the provider. The composer warns when a secret-shaped string
  (`sk-`, a 40+ char token) is typed in this room and refuses to send it. Not
  a filter for the model; a door for the person.
- **The phone.** It loads the box's SPA (`spa-delivery-plan.md`), so the lock
  and the room arrive there too. `MobileOnboarding` is the phone's native
  permission flow and stays; it is not this.
- **App outruns the server.** A new phone build against an old box must not
  strand on a missing endpoint: the guard treats a 404 from
  `/api/getting-started` as unlocked (memory: `project_app_outruns_server`).
- **Interview state lives in two rooms.** The card reads `interview_started`
  and `narrative_identity_ready`; the interview room is the only writer.
  Never mirror.
- **The old page's dismissed ids** stay in existing rows. The endpoint
  ignores unknown ids; no migration.
