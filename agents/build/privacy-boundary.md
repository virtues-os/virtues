# Privacy boundary — where data leaves the box

**This page sets requirements.** It covers what a change that sends data off
the box must keep true, and what we may and may not claim about it. Written against
`services/virtues-api/src/catalog.rs` (`enforce_zdr`) and the prompt builders
in `virtues-core/src/api/`. The transport side — what the relay can and cannot
see — is the manual's [reach page](../../docs/operate/reach.md); the
superseded pre-iroh description is
[`../archive/privacy-model.md`](../archive/privacy-model.md).

## Summary

Storage and transport keep the owner's data on the box: the relay forwards
end-to-end-encrypted bytes it has no key for. **Inference is the exception, because
the assistant needs the data to answer.** Unless a slot points at a local model,
asking the assistant anything sends the relevant part of the owner's data to a
model provider.

## What leaves

| When | What goes out |
|---|---|
| A chat turn | the question, the retrieved records (message text, transcripts, calendar entries, transactions, anything `sql_query` returns), the narrative identity core and the standing rules |
| The nightly day write-up | the day's evidence — message and transcript text, transactions with merchant and amount, calendar entries with attendee names, app and browser titles |
| Photo favorites, after the owner turns them on in the iPhone app | one Omni-slot call per favorite photo, with **the picture itself** (at most 2048 pixels on its longest side; one frame of a video), the date and time it was taken, and the name of a place the owner has named. Coordinates are not sent. A changed picture is sent again. At most 100 a day (`virtues-core/src/photo_enrichment.rs`). The screen that turns photos on says a model provider describes each one |
| Transcription | **the audio itself**, not a transcript |
| Image generation | the prompt, and any image supplied to edit |
| A web search | the query the assistant wrote and, when it gives one, the objective behind it; the gateway's search vendor receives the query |
| An applet with an `agent` prompt | whatever that applet was written to read |
| An outside AI agent connected to `/api/mcp` (Claude Code, Claude Desktop or Cursor, through the Mac app's bridge or an agent key) | whatever its tool calls return: `sql_query` results from any table the SQL catalog lists (the `data_` and `wiki_` tables, pages, tasks, threads, and the titles and message text of chats with Virtues), `semantic_search` results and page text. It reaches that agent's lab under the owner's own account terms, without the zero-retention terms the chat's gateway enforces (`virtues-core/src/server/api/mcp.rs`, `MCP_TOOLS` in `virtues-core/src/tools/mod.rs`, `crates/virtues-registry/src/sql_catalog.rs`) |
| Settings: a drafted change (Return in the search field) | what the owner typed, and the current values of the settings it may change: names, work, employer, time zone, the assistant's name and style notes, the two spending limits |
| Settings: a sample reply ("How {name} talks to you") | the owner's display name, the assistant's name, and the style notes in the field, saved or not; the week it answers about is invented, so no record goes out |
| A Google account signed in through `api.virtues.com` (a credential with `api_connection_id`) | every Gmail and Calendar request and response for that account, in transit: box → `api.virtues.com` → Truto → Google. Neither `api.virtues.com` nor Truto stores a body, and `api.virtues.com` logs only the route pattern. Truto keeps the Google grant, and logs each call's method, status, path and query (which hold calendar ids, that is addresses, and message ids), caller address and timings. This is a temporary step until Virtues has its own verified Google app; an account connected the older way still goes from Google straight to the box (`services/virtues-api/src/routes/vendor.rs`, `virtues-core/src/api/vendor_api.rs`) |
| An Outlook account (every Outlook account is signed in through `api.virtues.com`) | every Outlook mail and calendar request and response for that account, in transit: box → `api.virtues.com` → Truto → Microsoft Graph. The same rules as Google's row: no body is stored, `api.virtues.com` logs only the route pattern, and Truto keeps the Microsoft grant and logs each call's method, status, path and query (which hold calendar ids and sync tokens), caller address and timings. The sign-in asks Microsoft to read mail and calendars only. This is temporary, until Virtues has its own approved Microsoft app (`services/virtues-api/src/routes/vendor.rs`) |
| A bank is linked or removed through Plaid | Bank data itself goes box → `api.virtues.com` → Plaid in transit and is never stored. To cap linked banks per subscription, `api.virtues.com` keeps one row per linked bank in `linked_banks`: the account's opaque id and a SHA-256 of Plaid's item id. That tells us how many banks an account has linked, never which bank, and holds nothing Plaid's API accepts. The row is deleted when the bank is removed, and nothing records when or how it is used (`services/virtues-api/src/banks.rs`) |
| A watched Google calendar changes | Google calls `api.virtues.com` with no body: a random channel id and the box's random push id that the box chose, Google's opaque resource id, and a state word. `api.virtues.com` keeps the channel id until the box collects it. No event, address or account name passes through (`services/virtues-api/src/routes/push.rs`) |
| Home backgrounds: the list of styles, and a style when the owner picks it | box → `api.virtues.com` with the box's key: a request for the catalog of styles when an app loads the backgrounds and the box's copy is a day old (at most once an hour while it fails), which names nothing, and a request for a style's archive when the owner picks a style the box does not have or the catalog names a newer archive of one it has, which names that style. Both routes log nothing (`services/virtues-api/src/routes/backgrounds.rs`). `api.virtues.com` reads the files from object storage with its own key, so storage sees that server's address and the file name, never the box's. With `VIRTUES_BACKGROUNDS_SOURCE` set, the box asks that address instead and sends no key. The apps load the pictures from the box only (`virtues-core/src/api/backgrounds.rs`) |
| The owner says what excites them most about Virtues (Setup's Names step, or Settings · Profile) | On a box linked to an account: box → the account service (`services/virtues-atlas`, `PUT /account/start-with`) with the box's key, carrying one of five words (`days`, `patterns`, `memory`, `habit`, `ownership`) or `null` when the owner clears it. Atlas keeps it on the account row and nothing reads it back through the API. It is sent again when the box links to an account (`virtues_api::start_with`). Nothing the owner wrote and nothing from their data goes with it, and a box with no account key sends nothing. It is not part of any prompt, so no model provider receives it. |

Hosted path: box → `api.virtues.com` → the AI gateway → the model's provider.
Two parties besides the provider handle the request in the clear. The provider
per slot is whatever `crates/virtues-registry/src/models.rs` names; never hard-
code a provider list in copy.

## What must stay true

- **Zero data retention is attached per request, by the model.**
  `enforce_zdr` asks the gateway to route only through zero-retention
  endpoints for every model except one the gateway marks `zdr: "none"`. It is
  not a user setting: a global switch would silently downgrade every background
  job for as long as it stayed flipped.
- **A retained model is an explicit per-slot choice.** The picker labels it
  *Retained* (`ModelCatalog.svelte`); choosing it affects only that slot.
- **An unknown model id enforces.** A typo or a model newer than the hourly
  catalog fails with an error instead of reaching an endpoint whose retention we
  cannot verify.
- **Embeddings run on the box, and search doesn't rerank.** Search never sends
  data out to be indexed.
- **The usage ledger records metadata only** — model, tokens, cost. No prompt
  or completion log exists on our side.
- **BYO AI moves the boundary to the owner's vendor.** Calls go to their
  endpoint under their terms; ZDR enforcement is ours only on the hosted path.
  Web search is the exception: their endpoint cannot run it, so with a key
  set it still takes the hosted path, as the settings page says.
  Never claim BYO is more private (see [`byo-ai-plan.md`](../plan/byo-ai-plan.md)).
- **Every kind of record can reach a model.** The assistant reads the whole
  record, cycle and symptoms included, through the SQL tool, search, the day
  write-up, suggestions, the chat's circumstances and entity articles. There
  are no per-kind switches.
- **Export writes on the box.** `virtues export` writes plain files to a
  folder on the server; nothing is uploaded.
- **A local model keeps only its own slot's calls on the box.** A slot pointed at a local
  endpoint keeps that slot's inference on the box; titles, summaries and
  transcription still go wherever their own slots point.

## What we must not claim

- **"Private by construction" for inference.** The relay cannot read traffic —
  cryptography enforces that. Zero retention is enforced on our side and
  contractual on the provider's side: it is a real guarantee we can check, but it
  still depends on the provider keeping its promise.
- **"Coordinates never leave."** The day write-up reduces GPS to distance and
  pace on the box, but chat's `sql_query` can read `data_location_point`, so a
  question about where you were sends coordinates, unless the owner has
  turned off Places you've been.
- **"Blind relay" or "no records"** as unqualified properties. The relay sees
  which device keys talk, from which addresses, and how much passes when;
  "keeps no records" is an operational description, not an attested property.

A new path that sends record content off the box — a new applet capability, a
new cloud feature — adds a row to "What leaves" in the same change.
