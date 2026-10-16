# Seeds

## Every box

`prod_seed.rs` ([../src/seeding/prod_seed.rs](../src/seeding/prod_seed.rs))
runs at every boot: the assistant's profile defaults and the getting-started
chat. Nothing else is seeded from the binary.

## The demo life

`demo/life.sql.gz` is the one demo dataset: a single fictional person, Jules
Arden, a product designer in Austin. It is what demo.virtues.com shows and what
`make dev-demo` loads.

- **Backbone.** Three thin years: raw streams, the days and events written on
  top of them, chats, pages, projects, memories, chapters, stories. The last
  eight weeks are as dense as the showcase week, so day numbers read against a
  believable "usual".
- **Showcase week.** Seven complete days with every stream full (location with
  a flight, messages, email, transcripts, Mac apps, browsing, money, health),
  plus the events, day pages and links the box derived from them.
- **Membership.** `demo.rows` lists every row the life brought and which part
  it belongs to; placement and the nightly reset work from that list, not from
  id prefixes.

The file is built by [tools/demo/build.py](../../tools/demo/build.py) and is
not edited by hand. Installing, placement and the reset are in
[deploy/demo/](../../deploy/demo/README.md). CI loads it on every migration
(`deploy/demo/check.sh`); when a migration renames or drops a column it uses,
rebuild it and commit both together.

All of it is fiction. Names come from the cast in
`tools/demo/gen-backbone.py`; phone numbers are `+1512555xxxx`, email is
`@example.com`. The showcase week's shape came from a real week; every word in
it was written fresh, and the build is leak-checked against the source record
before it is committed (`tools/demo/README.md`).
