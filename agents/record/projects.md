# Projects

**Written 2026-09-29.** A Project is a room you enter and work in: a saved,
named lens over the life-graph that grounds every chat filed inside it.
Supersedes `agents/plan/projects-plan.md` (planned 2026-07-08, deleted). The
corpus half, files that extract, embed and cite, was specified and built
under [researcher-plan.md](../plan/researcher-plan.md), which also holds the
steps still open. North star: NotebookLM and Claude Projects, over things you
already own rather than an upload bin.

## The model

- **A Project is a lens, not a container.** Its members are rows in
  `app_project_items`, each a ref URL: a page, a day, a person, a chat, a
  drive file, a link. Anything retrievable can be a member, not only files,
  except another project: the server refuses one. That is the federation claim: the PDF, the advisor's email thread, the
  person and last Tuesday sit in one retrieval scope as peers.
- **A project has a brief, and no memo.** `instructions` are standing
  directions for how the model behaves in the room, written by the owner and
  inlined into every chat the project holds, with the member list and each
  member's role. A catch-up memo (`current_status`) was dropped on 2026-09-29
  to keep the project to what it is for and what is in it; its column stays
  in the table, unread, so memos already written are kept.
- **Chat is many threads**, Claude-Projects style: characters in one chat,
  drafting in another, research in a third, all grounded in the same members.
  Not NotebookLM's single continuous thread.
- **No local notes.** A project references global Pages; pasted text becomes a
  file in Drive. One ingestion path for everything.
- **Internal metaphor: room** (you enter it; it has a state and an accent).
  User-facing name: Project. The feature was never to be called "Rooms".

### The Library question

The first design gave a project a **Library** of materials. Within two weeks
(2026-07-20) the noun was retired: a project *containing* a Library is a
container inside a container, which contradicts the lens. Things are simply
**in the project**; the verb is "Add to project". Membership has a role:
`library` (grounds chat, the default, since that is what membership means),
`manuscript` (the owner's own draft, excluded from retrieval so it is never
cited back at them as a source), and `pin` (navigation only).

The same review moved ingestion off membership. Extraction was first "lazy,
on add-to-Library"; it became **universal on upload**. Every text-bearing
drive file is extracted, chunked and embedded as the `uploaded_document`
ontology, and membership is only scope and weight, never the trigger.

### Thing, and the three-way overlap

Before projects, "a folder you re-enter" existed three times: Spaces
(collection, chat binding, memo, color), Things-as-container, and pins. The
consolidation made **Projects** the curated workspace, kept **pins** as flat
cross-cutting shortcuts, and kept **tags** as labels that might one day feed
smart membership. Thing was demoted from container to a plain entity for
reference. That was later taken further: `wiki_things` was dropped on
2026-07-22, and the entity kinds a record can point at are person, place and
organization.

## Doctrines

- **Materials are snapshots you own, not live pointers.** An external source
  is fetched once and stored with provenance; change detection ("watch") is a
  separate feature.
- **Extraction is native text only.** Born-digital files carry their text.
  OCR was designed, spiked on the NPU and cut; the measured reasons are in
  researcher-plan.
- **Scoped retrieval has two modes.** Open (the default) adds a z-space boost
  to members; Scoped makes membership a hard filter and adds a grounded prompt
  line, answering only from the project and saying so when it is empty. The
  boost is additive because z-scores go negative, and a multiplier would invert
  rankings. Members resolve per search call rather than from a snapshot, so a
  file added mid-chat counts at once.
- **The concept map is explicit only.** There is no NER over free text, so an
  auto-map over uploaded material would be empty. The graph is built from what
  the owner wrote or filed: entity members and `[@ref]` links inside member
  pages, with co-occurrence edges. It shipped as a "Mentions" filter on the
  member grid rather than a separate Map tab.
- **Project views are built in.** Plain components, not customizable `view`
  applets.

## Citations are refs

**Decided 2026-07-10.** A load-bearing claim carries a named source that opens
the real source, not a generic "5 matches" blob or an opaque `[1]`. The
realization that collapsed the design: the app already rendered
`[text](/person/<id>)` links as refs with a hover preview and click-to-source,
so **the model cites by emitting an ordinary markdown link to the source's ref
route**, and the ref system supplies the chip, the preview and the navigation.
No citation syntax, no new schema, no stream event.

- The system prompt's `<citations>` block tells every mode to cite
  load-bearing claims as links to the `ref` a tool returned, never to
  fabricate one, and never to cite the same source twice in a row. A computed
  figure cites the query that produced it.
- **Every hit got a viewer**, which removed the one hard problem. A raw
  `data_*` record (an email, a transaction) had no route, and the first cut
  cited the entity it was *about* instead. That was reversed: every record opens
  at `/record/<ontology>/<id>`, so a citation points at the source itself.
  Document chunks cite `/drive/file_<id>?page=N&q=<quote>`, and highlights
  cite `?hl=<id>`.
- The audit that preceded this found the old stack **dormant and fragile**:
  normal chat never told the model to cite, and a citation id was a frontend
  positional counter the model never saw. The numeric citation components
  survive for `web_search` only, where an external URL has no ref.

## The rename

On 2026-09-21 Notebook became **Project** (migration 0029): `app_notebooks` →
`app_projects`, `app_notebook_items` → `app_project_items`, `notebook_id` →
`project_id`, `/notebook/…` → `/project/…`. Spaces had become Notebooks on
2026-07-09. **Ids were not rewritten**: a project id is still `nb_…`, because
the prefix lives in ref URLs, pins, member rows and stored JSON on live boxes,
and rewriting those across foreign keys is not worth a cosmetic letter. Records
written before the rename keep the names they were written with.

## Deliberately not built

Export and sharing links; templates and suggest-a-project; char-precise
citation highlights (named chips per claim were enough); and Timeline or
Outline views. Each stays out until someone needs it. None is a gap.
