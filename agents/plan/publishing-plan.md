# Publishing — one frozen artifact, three surfaces

> **STATUS 2026-09-01: direction agreed, nothing built.** The primitive
> (publications), the medium (a face), and the networking decision (no ingress
> in v1) are settled. The renderer is the work.

## The ask that produced this

Someone plans a small building project with a person they love, and wants two
things out of the box at the end: a page that person can open at a URL on a
domain the owner controls (`example.com/the-project`), and a printed plan to
hand over.

That is the whole requirement, and it is worth keeping in front of the design,
because it is the ordinary case. Not a publishing platform — one artifact, one
recipient, one sheet of paper.

## What exists today (verified 2026-09-01)

Page sharing is **already built and already useless**:

- `app_page_shares (id, page_id, token, created_at)`, one row per page
  (`UNIQUE(page_id)`), token is a UUIDv4
- `POST/GET/DELETE /api/pages/:id/share`, and `GET /api/s/:token` +
  `GET /api/s/:token/files/:file_id`, both unauthenticated by design
  (`server/mod.rs`, in the pre-auth route table)
- `(public)/s/[token]/+page.svelte` renders it through
  `PublicPageViewer.svelte` → `createReadOnlyEditor()`, rewriting
  `/api/drive/files/:id/download` → `/api/s/:token/files/:id` so images work
  without a session

One line defeats all of it — `PageContent.svelte`:

```js
const url = `${window.location.origin}/s/${shareToken}`;
```

On the Mac that origin is `http://localhost:7117`. The box has no public
listener. **The mechanism is complete except for the part where anyone else
can load it.**

What is missing, and is not a small gap:

| | |
|---|---|
| Print | **Zero** `@media print`, `@page`, or `window.print()` anywhere in `apps/web`, `applets`, or `docs`. |
| Pagination | The page editor is a Yjs CRDT rendered by CodeMirror as a scrolling text buffer with decorations. CodeMirror does not paginate and will not. |
| Page layout state | `app_pages` has `kind` (`page`/`article`) and no size/layout column; `pageDisplay` (font/size/width) is a client-only store. |
| Export | One thing: "Copy as Markdown" in the toolbar overflow. |
| Face writes | The face runtime is exactly one verb — `virtues.query(sql)`, read-only, as `virtues_face_reader`, in a READ ONLY transaction, 5000-row cap, 1-hour token. |
| Face images | `FACE_CSP` is `img-src 'self' data: blob:`, and `/api/drive/files/:id/download` is authenticated — an `<img>` cannot send a header. A face's pictures are files inside `face/`, or `data:` URIs out of SQL. |
| Guests | None. `app_auth_user` plus pairing means *your devices*. There is no second person in the model. |

## The primitive: a publication, not a page share

`app_page_shares` is page-shaped, and the thing we need is not.

> A **publication** is a frozen, self-contained artifact with a token,
> produced by any surface: a page, an applet face, a wiki entity, a notebook,
> a query result.

Pages become one producer among many rather than the special case. One table,
one route, one renderer.

Freezing is the load-bearing word. A publication is rendered **once** and
stored as bytes. It does not query at view time, hold a token, or name an API.
That is what makes the same artifact printable, mailable, and (someday)
servable without three code paths — and what keeps a published thing correct
when it is stale, which a live view never is.

## The medium: a face is already the right container

The insight that makes this cheap. A face's constraints and print's
constraints and static publishing's constraints are the **same three
constraints**:

| Face constraint (today) | Why it is also print/publish |
|---|---|
| one `face/index.html`, self-contained | that *is* a publishable artifact; export is nearly `cp` |
| CSP forbids external hosts, ever | nothing breaks when it leaves the box |
| sandboxed iframe, opaque origin | no ambient authority baked in; works offline forever |
| arbitrary HTML/CSS you fully control | `@page { size: letter }` is simply allowed |

The page editor's constraints are the opposite of all four: a CRDT, live
decorations, box-authenticated media URLs, no pagination. So pages are a
*producer* of publications; the face is the *shape* a publication takes.

Nothing gets bent to make this work, which is the test of whether an
abstraction is the right one.

## Doctrine

Publishing is where a private box touches other people, so the rule it must
obey is stated once and applies to every later question:

> **The higher level may name, carry, and introduce. It may never hold or
> read.**

Hold → the box, always. Name → a shared namespace (DNS is irreducibly social;
you cannot be sovereign over a name). Carry → a blind wire. Read → the
recipient, and no one else.

And the test that decides any future addition:

> **If virtues.com vanished tomorrow, does the box still hold everything and
> still work for its owner?**

Yes → subsidiary. No → absorption.

This is why hosting publications on our infrastructure is refused: it inverts
the one claim every other feature makes, and creates a deletion promise we
could not honor. A user with no domain gets a *file*, not a link, and we say
so in the manual rather than apologizing for it.

## Networking: decided, and deferred

Two ways to give a stranger a URL, distinguished by where the bytes live when
they click:

- **Push** — the box renders an artifact and sends it somewhere already
  public. No inbound anything; works while the box sleeps.
- **Pull** — their browser reaches the box. Needs a **name**, a **cert**, and
  an **ingress**; box uptime becomes a dependency of every link ever sent.

Pull is what "it stays on my server" means, and it is coherent. It is also the
only option with a permanent cost: **the box currently has zero
internet-facing attack surface.**

Three facts that constrain any future pull design:

1. **iroh cannot carry a browser.** `virtues-iroh/src/server.rs` closes the
   connection before a byte of HTTP if the peer is not allowlisted, and a
   browser has no EndpointId. This is not a policy to relax — it is the same
   argument `review-access-plan.md` makes about pairing. No amount of relay
   work changes it. (The relay opened on 2026-08-31, which removed the
   billing entanglement but not this.)
2. **BYO domain is not an alternative to SNI passthrough — it is a parameter
   of it.** A domain you own still needs an A record pointing at something
   that receives packets. `CNAME → the relay` *is* passthrough wearing your
   name; `A → your house` is a port-forward, and the box has no TLS surface
   (`server/mod.rs`: "Plain HTTP on :8000 is the only listener"). Ship BYO
   domain with the ingress, not after it.
3. **A public ingress must never serve `app.clone()`.** `relay::maybe_spawn`
   hands the *entire* API to the iroh transport (`server/mod.rs:1233`), which
   is correct there because the transport is allowlisted. A public door must
   serve a **separate, minimal router** — the publication route, its asset
   door, and nothing else. Then a total auth bypass on that path yields
   exactly the things someone chose to publish. Without this split, the
   ingress should not be built at all.

**v1 builds no ingress.** LAN plus a file covers the actual ask completely,
keeps the zero-surface property, and wastes nothing: a frozen self-contained
artifact is precisely what an ingress would later serve.

## The work

1. **The publication primitive.** `app_publications (id, producer_kind,
   producer_id, token, title, content_hash, bytes, rendered_at, expires_at,
   revoked_at)` plus a hit log. Replaces `app_page_shares`, which has no
   expiry, no revocation record, and no way to answer "was this ever opened."
   Token stays 122-bit; add the three columns it should always have had.
2. **The renderer — the actual work, and shared by all three surfaces.**
   Producer → one self-contained HTML document: assets inlined or `data:`,
   query results frozen to literals, no `virtues.js`, no `/api/` reference, no
   token, no box URL. Screen, print, and publish are the same bytes.
3. **Paged mode.** `@page { size: letter | a4 }`, break control, running
   heads. Not a CSS pass on the editor — a second render path, for which
   `createReadOnlyEditor()` is the precedent that a second path is acceptable.
   Page size belongs on the publication, not on `app_pages`.
4. **PDF is the browser's job.** `window.print()` on the frozen document; the
   OS makes the file. No headless browser on the box — Chromium on a Q6A is
   not a thing we are doing. Typst/weasyprint as a subprocess applet is the
   later answer *if* unattended PDF is ever needed.
5. **A publication lint, in the pipeline not the review.** Refuse to publish
   an artifact containing `virtues.query`, `/api/`, a bearer-shaped string, a
   localhost URL, or an EndpointId. A publish that ships a dead API call has
   leaked what the API is named.
6. **Show what leaves.** Dereference entity links, list every image, resolve
   the SQL to the actual rows, and put it on screen before anything is
   written. **This is the most important item on the list.** The failure mode
   here was never an attacker — it is a page that mentioned `[@Nick]` and
   quietly carried their phone number into a public artifact.
7. **Fix the origin bug** (`PageContent.svelte`) — advertise the box's LAN
   address, which `BOX_DIRECT_ADDRS` already tracks for iroh dialing. Ten
   lines, and it makes sharing work today for anyone in the room.
8. **A face asset door** — `GET /api/face/asset/:id?vt=`, gated by the
   existing face token, so a face can show drive images. ~40 lines beside
   `face_query_handler`. Without it every visual face is `data:` URIs.
9. **Deferred, deliberately: face writes.** Read-only is defensible. A board
   gets edited by talking to virtues, not by clicking the board. Revisit only
   with a real second producer.

## What v1 does not do

No ingress, no hosted publishing, no guest identity, no multiplayer, no live
feeds. A publication is a snapshot handed to one person. Everything above is
reachable from that; none of it is required by it.

## Verification

- A publication renders identically on screen, in the print dialog, and as a
  saved standalone file opened with the network off.
- The lint refuses an artifact carrying a token, an `/api/` path, or a
  localhost URL — proven with a deliberately poisoned fixture, not by
  inspection.
- The "what leaves" screen names every image and every dereferenced entity in
  a page that links a person, before anything is written.
- A shared page opens from a second device on the same network (item 7).
- Revoking a publication makes the token 404, and the hit log shows the
  opens that happened before it.
- A face with images renders in the app, in print, and in the exported file
  with no change to its source (item 8).

## Death condition

Delete when a publication can be produced, printed, and handed over. What
survives: a manual page on publishing and printing, and a record of the
primitive (frozen artifact, three surfaces) plus the two decisions worth
keeping — *the face's constraints are print's constraints*, and *the higher
level may name, carry, introduce; never hold or read*.
