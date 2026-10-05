# Identity: who is on the other end of a record

**Status: proposed, not built.** For the owner's review. Supersedes the "entity
merge" line in [wiki-notes-plan.md](wiki-notes-plan.md). Settled elsewhere and
assumed here: identity is deterministic or owner-made, never a model's guess
(the ER research that killed semantic entity resolution).

## What is wrong, measured on one box (2026-10-05)

- **Companies stored as people:** 45 of 730 people. 80 of the 239 email
  addresses on people look automated (no-reply, info, billing, alerts).
- **Never written to:** 214 people with an email address have never had an
  email sent to them by the owner.
- **Duplicates:** 20 exact-name groups cover 48 rows, usually one row per email
  address or phone number, with the person's records split between them.
- **No refs at all:** 225 people.
- **Organizations are mostly merchants:** 243 of 255.

## How it happens

1. **Every unseen sender becomes a person.** `resolve_or_create_person_with_name`
   (`entity_resolution/people.rs:686`) and its calendar twin mint a
   `wiki_people` row for any email address they have not seen. Nothing checks
   whether a human is on the other end. `entities.rs:363` already names the
   bug; the only remedy is reclassifying one row at a time by hand.
2. **Identity is one address.** The id is a hash of the email
   (`people.rs:749`), so one human with three addresses is three people.
3. **Relays become people.** A notification sent "via" a service from a
   no-reply address is one row, named after whoever it relayed first.
4. **Contacts fold in, but the duplicates stay.** A phone contact matches by
   email first and phone second (`ios_ingest/contacts.rs:164`). Its data is
   merged into the first match, and the other rows that already hold its other
   addresses survive. Later lookups by address then pick an arbitrary
   `LIMIT 1` match. The phone match is a substring `LIKE`, which can join
   people who only share digits.
5. **Nothing can merge.** `reclassify_person_as_organization` exists; merge
   does not.

## The model: what was observed, apart from who it is

Three layers, in ELT terms:

1. **Records** (`data_*`). Untouched.
2. **Identifiers.** A new table: one row per `(kind, value)` — an email
   address, a phone number in E.164, a messaging handle. Derived entirely from
   records: display names seen, first and last seen, how often, whether the
   owner has ever written to it, and automated signals. Rebuildable at any
   time.
3. **Subjects** (`wiki_people`, `wiki_orgs`). A subject is a set of
   identifiers. The assignment is its own table, with who made it: `contact`
   (a card the owner keeps), `owner` (a merge, split or reclassify they made),
   or `rule` (the creation rule below).

`wiki_refs` stays as the materialized join of record → identifier →
subject. A merge, split or reclassify changes assignments, then rebuilds the
refs for the identifiers it touched. Day articles, search and the wiki follow
without knowing anything happened.

This also replaces the slowest query on the box: the `handle_owner` CTE in
`resolve_message_senders` (83 slow-query warnings in three days) unnests every
person's handles on every batch. An indexed identifier table is a lookup.

## Rules

**Creating a person needs two-way contact.** An identifier earns a person only
when the owner has written to it, it is on a contact card, or it shares a
meeting with the owner. A one-way sender gets no person. If it carries
automated signals, it is filed under an organization keyed by its domain.

Automated signals, any one of which is enough:

- a local part like `no-reply`, `notifications`, `billing` or `info`;
- Gmail's Promotions or Updates category (the `labels` column, already stored);
- a known relay (a "via" display name from a shared no-reply address).

**Merging.**

- **Automatic** only when a contact card lists both identifiers. The card is
  the owner's own statement that they are one person.
- **Suggested** when two subjects share a normalized name and nothing
  conflicts. Conflicting means both are on separate contact cards, or both were
  active in the same thread. The owner confirms. These are the identity
  questions in [narrative-resolution-plan.md](narrative-resolution-plan.md).
- **Never** from a model's judgment, and never from a shared first name.

**What a merge does,** in one transaction:

1. The loser's identifiers move to the survivor; its refs are rebuilt. A record
   both referenced collapses into one ref, so the unique-index collision that
   `entities.rs` cites as merge's blocker cannot happen.
2. The loser's name becomes an alias. Notes, rules, pins and project items
   follow.
3. One article survives, and it is marked for revision. The other is archived,
   not deleted.
4. A redirect row keeps `/person/<loser>` opening the survivor, including links
   already written into articles.

Undo is reassigning the identifiers back, which the assignment table makes
mechanical.

## Order

1. **Stop the bleeding.** The two-way creation rule and the automated signals,
   at the resolver. No schema change. New junk stops.
2. **One-time cleanup.** Reclassify existing people whose every identifier
   is automated and never written to, with a dry-run count first.
3. **Identifiers and assignments.** The two tables, backfilled from existing
   rows. The resolvers write identifiers; refs are built through assignments.
   Contacts match on exact normalized values only.
4. **Merge,** with redirects and undo, and the suggestion queue in the UI.

Steps 1–2 are small and make the People list honest. Steps 3–4 are the real
fix and need a migration each.

## Open

- **One-way senders that matter.** A real person who writes and is never
  answered. Proposed: they keep refs to their identifier, and the owner can
  promote them from a "seen, not a contact" list.
- **Group threads.** Do co-participants count as two-way contact? Proposed: no.
  A reply to the thread is contact with the sender only.
- **Merchants.** Are 243 merchant organizations useful as subjects, or should
  transactions point at a merchant identifier with no subject until the owner
  cares? Proposed: the latter, for the same reason as people.
