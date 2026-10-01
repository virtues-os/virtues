# Names have one owner

Shipped on `wave` 2026-09-30 → 10-01 in four slices (`006666c4`, `50fc8707`,
`690661ea`, and the slice 4 commit). The rule: **a chat, page or project's
name, icon and color belong to it. Anything that points at it holds the URL
and asks.**

## What was wrong (swept 2026-09-30)

A sidebar pin read "Ideas for …" while its page was titled something else
weeks later. That was the visible case of a general one: pins stored the
label and icon they saw at pin time, tabs persisted their labels to
localStorage and restored them as-is, wiki article pages copied their
subject's name once, ref pills froze the text they were written with, and no
rename path updated any of the copies. Underneath sat eight route→kind maps,
six fallback strings for "untitled", and a shell entity registry that was
never filled, so every update to it did nothing.

Two worse bugs came out of the same sweep:

- **Chat titles were overwritten.** The client reset its "title generated"
  flag on every open, so the next send asked for a new title and the server
  wrote it unconditionally, over hand renames too.
- **A page rename was undone.** An open page kept its old title locally, and
  its next save (an icon, a cover) sent the old one back.

## What it is now

| Where | How it gets the name |
|---|---|
| Sidebar rows, ⌘K, drawer, Home | The chat, page and project stores, with one fallback per kind (`untitled()`: "New chat", "Untitled page", "Untitled project") |
| Pins | `api/pins.rs` lists `PinView`s resolved at list time; the client overlays the stores (`pinIdentity`) |
| Tabs, window title | `identityOf(route, {title: tab.label})`; the stored label is only the first frame after a restore |
| Ref pills to a chat, page or project | `identityOf` with the written text as fallback |
| Anything the stores don't hold | `POST /api/refs/resolve` (`refs::resolve_identities`), batched and cached per session in `refs/identity.svelte.ts` |
| Wiki article pages | `wiki_articles::retitle_article`, called from every person, place, org and story rename |

Writes go through `renameRef(url, title)` and `setRefLook(url, {icon, color})`.
Renaming or re-iconing from a pin, a tab or a sidebar row changes the thing
itself. A pin has no name of its own unless nothing stands behind it (an
external URL, an app screen), and a tab never has one: a tab with no record
behind it offers no Rename.

A pin never shows something in the trash: the box leaves it out of the list
while its chat, page or project is in Recently deleted, restoring brings it
back, and purging deletes it.

Chat titles carry `title_source` (`seed`, `generated`, `settled`, `user`;
migration 0037). The server titles a chat after its first exchange and once
more at six user turns, never over a name the person gave it, with a
compare-and-set write so a rename made mid-generation wins.

## Decisions, and what they rule out

- **No aliases.** A pin cannot carry a name different from its thing. If that
  is ever wanted, it is a deliberate second name shown beside the real one,
  never a silent fork.
- **A person or place in prose keeps the words it was written with.** Only
  chat, page and project pills follow renames. "Nick" in a sentence is the
  writer's phrasing; swapping in the record's full name would rewrite it.
- **Days and years are named by their views.** The resolver's "name" for
  `/day/…` is the id, so the client never asks the box for one.

## Not covered

- Renaming an article page directly does not rename its subject; the next
  subject rename re-titles the page.
- Ref pills inside the page editor (CodeMirror) show the markdown's text,
  because that text is what is being edited.
- Several views still write `updateTab({label})` (chat auto-title, project
  and applet detail). They now only keep the restored-tab fallback current.
