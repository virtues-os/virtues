# Collector delivery — permanent failures

**Status:** Open. The rest of the 2026-06-25 durability audit shipped with the
shared Rust outbox; see [`../record/data-durability.md`](../record/data-durability.md).

## The gap

The outbox (`crates/virtues-reach-client/src/outbox.rs`) and the drain
(`apps/web/plugins/reach/src/upload.rs`) treat every failure the same way: a
batch that does not come back `{"status":"success"}` is nacked, its rows back
off (capped at 5 min) and are retried forever. That is right for transient
failures and wrong for permanent ones:

- **A record the box will always reject** (a 400, a decode failure in
  `ios_ingest`) is retried every five minutes for the life of the install, on
  battery and radio.
- **It takes its batch-mates with it.** The box answers per batch, so every
  good row claimed alongside the poison row is nacked with it, every time. A
  stream whose oldest due row is poison stalls behind it.
- **Nobody sees it.** The device screen shows queue depth, not a record the box
  refuses.

## What to build

1. **Classify.** The drain distinguishes transient (no route, timeout, 5xx,
   409 from a busy run, 429) from permanent (4xx that is not 409/429, a box
   response naming a rejected record). Only transient failures back off.
2. **Isolate.** On a permanent batch failure, bisect: retry the batch halves
   until the rejected row is alone, so good rows deliver.
3. **Dead-letter, never delete.** A row rejected alone moves to a
   `dead_letter` state with the box's reason. It is never deleted
   automatically — un-acked data is the only copy.
4. **Surface it.** The device screen says "N records the server refused" per
   stream, with the reason.

Deterministic ids make every retry and every bisection idempotent, so none of
this can duplicate data.

## Open

- What the owner can do with a dead-lettered record: re-send after a box
  upgrade, export, or discard. Re-send after upgrade is the likely common case —
  a new box may accept what an old one rejected.
- Whether `ios_ingest` should return per-record outcomes rather than one status
  per batch, which would make bisection unnecessary.
