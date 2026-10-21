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
| Transcription | **the audio itself**, not a transcript |
| Image generation | the prompt, and any image supplied to edit |
| An applet with an `agent` prompt | whatever that applet was written to read |

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
  Never claim BYO is more private (see [`byo-ai-plan.md`](../plan/byo-ai-plan.md)).
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
  question about where you were sends coordinates.
- **"Blind relay" or "no records"** as unqualified properties. The relay sees
  which device keys talk, from which addresses, and how much passes when;
  "keeps no records" is an operational description, not an attested property.

A new path that sends record content off the box — a new applet capability, a
new cloud feature — adds a row to "What leaves" in the same change.
