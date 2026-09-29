# References

**Written 2026-09-29.** Everything that can be pointed at (a person, a place,
a file, a page, a day, a record, an external URL) is pointed at by one
primitive, the **ref**, and rendered by one component. Supersedes the plan of
the same name, `agents/plan/references.md` (designed 2026-07-07, deleted).

## The rename was the fix

In July 2026 the same three references rendered as three unrelated widgets: a
person as a gray `@` pill, a domain as an icon and text, a photo as a bare blue
link. Citations used a fourth renderer and Drive files a fifth. There was no
reference primitive, only five things that happened to be references.

The word `@` produces is a **Reference**, not an "entity" and not a
"backlink". Those words leaked implementation (wiki entity, page backlink)
into one idea: *a pointer from here to a target*. A ref is a URL route; a
target is anything a route can name. Backlinks are inbound references, the
same edge read the other way. Once "reference" was the noun, a person and a
PDF stopped being different widgets. The code followed: `EntityChip` became
`Ref`, `EntityPicker` became `RefPicker`, `entityRoutes` became `refRoutes`,
`search_entities` became `search_refs`, and drive files became ref targets at
`/drive/file_<id>`, found by `@`-search beside people and places.

## Three densities, and open differs by type

A ref renders at one of three densities:

- **Inline**, in a line of text: a name, target-agnostic.
- **Preview**, on hover or focus: a floating card with target-specific facts
  (a person's relationship, a place's map, a file's thumbnail). One shared
  body, `RefCard`, fetched through a cached summary.
- **Open**: the target at full fidelity. A route, not a component.

**The load-bearing distinction:** inline and preview are genuinely universal,
one component that behaves the same everywhere. Open is not. A person's full
view and a PDF's full view share nothing, so open is a route that dispatches
by target type, which the tab registry already did. The seam falls exactly on
the existing component/route boundary, and forcing one component across it was
the mistake the design stopped making.

**Identity is not representation.** A person may have a record, a page and
files, but open lands on one canonical surface per type (person to profile,
PDF to reader), and that surface aggregates the rest. Profiles were not
converged into editable pages.

## Inline is a link, not a pill

The first design made the inline density a filled pill. The citations work
(see [projects.md](projects.md)) split it by surface: a pill where a ref is a
token you inserted (the composer, the page editor), a Wikipedia-style link in
rendered output, because load-bearing citations recur in prose and must
disappear into the text.

That split did not last either. Once the page editor revealed markdown only
where the caret touches, its surface became a reading surface too, and a pill
beside a link would say the same thing two ways. **Every inline ref now
renders as a link**: in chat answers, in the editor and in the composer. Type
is surfaced on hover, never in inline chrome. Plain click on a rendered ref
opens the target beside you; hover peeks.

## Embeds, reversed

Field feedback (a lone `@name` on its own line reads as "…is that it?") led
to a fourth density: an **embed**, a persistent card promoted automatically
when a ref sits alone on a line, with a per-type body.

**That was reversed for entities.** `ref-links.ts` now renders a
`![label](/person/…)` block embed as a plain inline link and swallows the
`!`: **entities have no card; they are always inline links, with hover.** Old
documents written with block embeds render as links rather than breaking.
The `!` prefix still means media (an image, audio, video or file) and still
renders the media widget. The preview card is the only card an entity has.

## Citations are refs

A citation is not a parallel system. The model cites a load-bearing claim by
emitting an ordinary markdown link to the `ref` a tool returned, and the ref
system renders it. The reasoning, and the decision that every retrieved record
gets its own viewer so no citation dead-ends, is in
[projects.md](projects.md#citations-are-refs).

## Left alone on purpose

Entity resolution writes derived pointers (`wiki_refs`) that power backlinks
and search. That was never refactored into this work. It produces refs; it
does not render them. The plan's idea of warming a per-region tile cache at
resolution time was overtaken by the box serving its own map files
([map-tiles.md](map-tiles.md)); a place's preview never has a client talk to a
third party.
