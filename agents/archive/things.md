# Things

**Status: removed (2026-07-21).** Deprecated by `agents/archive/stories-plan.md` §8
(2026-07-13) and finished: the `wiki_things` table is dropped
(migration `0060_drop_wiki_things.sql`), the `/api/things` write path and the
`api::things` module are gone. Projects/hobbies now become stories, concepts
become topics, and a rare mattering particular accumulates as a floating
mention instead of its own entity. Kept below for history only.

---

A **thing** is a folder you can re-enter. A project, a pet, a goal, a topic — anything you want to keep loosely organized without forcing it into a rigid ontology.

## What a thing is

- A **name + icon + description** (all editable, all optional except name).
- A list of **pinned references** — URLs to pages, chats, people, places, files, or external links (articles, videos, docs).
- A **"Where you left off" memo** (`current_status`) at the top of the detail view — a short summary that re-orients you after a gap. AI-generated nightly + on-open when stale.

That's it. Things are deliberately light.

## What a thing is *not*

- Not a tag or category — there are no required types in v1.
- Not an ownership or scoping construct — pinning a page to a thing doesn't move it or restrict it.
- Not an entity in the wiki sense — things are folders, not nodes the NER pipeline tries to resolve.

## Schema (v1)

- **Table:** `wiki_things` (id, name, category, icon, description, cover_image, current_status, current_status_at, current_status_edited_by, created_at, updated_at)
- **Pins:** `wiki_thing_pins` (id, thing_id, url, name, description, sort_order, added_at) — pins are URL pointers; no FK into target tables.
- **ID prefix:** `thg_`
- **Routes:** `/things` (list), `/thing/thg_{id}` (detail)

The `category` column exists but is **not surfaced in v1 UX**. Reserved for future AI-tagging and filtering.

## Sidebar

The sidebar "Things" link (`sys_things`) routes to `/things`. The legacy `sys_projects` ID maps to `sys_things` via `LEGACY_ID_MAP` for stored views.

## Chat integration

The ChatInput toolbar has a **Thing picker** that attaches one or more things as message-level context. There is no `thing_id` on chat sessions — context is per-message.

## Known gaps / next up

- **User-editable `current_status`.** The backend `UpdateThingRequest` does not yet include `current_status`; the detail view shows it read-only. Adding edit requires extending the Rust handler and flipping `current_status_edited_by` to `'human'` on user write.
- **AI memo generation.** The nightly/on-open writer is the killer feature — verify it's actually running and writing into `current_status`.
- **Cover image.** Column exists, no UX.
