# The manual

This directory is the **public documentation** for Virtues — the source that
`virtues.com/docs` renders and that the server will eventually serve as its own
in-app help. It is not the workshop: design records, plans, and audits stay in
[`../agents/`](../agents/) and are written for the people and agents building
Virtues. Pages here are written for the people *running* it.

## The contract

- **`manifest.json` is the law.** Every page is listed there, in reading order,
  under its section. The website builds its nav, its prerender list, its
  sitemap entries, and `llms.txt` from the manifest — an unlisted page is
  invisible, and a listed-but-missing page must carry `"planned": true` or the
  site build fails.
- **Pages are plain markdown** with frontmatter: `title`, `description`
  (becomes the meta description — write it for search), and optionally
  `updated` (YYYY-MM-DD). No Svelte components — these files must also render
  on the server, and must stay readable as raw markdown (every page is served
  as `<slug>.md` for agents and curl).
- **Slugs are paths.** `operate/upgrading.md` publishes at
  `/docs/operate/upgrading`. `index.md` is the `/docs` landing page.
- **Write in the practical register** (see `../agents/build/voice.md` and the website's
  `DESIGN.md`): tight, specific, technical. The philosophical voice lives in
  the Library, not here.
- **Claim only what ships.** A manual page describes the released system. If a
  feature is half-built, its page stays `"planned"` in the manifest until the
  feature lands. The workshop is where in-progress design lives.

## How it publishes

The website repo's `scripts/sync-docs.mjs` copies this directory into its
build — from a local sibling checkout in dev, from a GitHub tarball pinned to
`VIRTUES_DOCS_REF` on Vercel. Once stable releases exist, that ref pins to the
latest stable tag so published docs describe what servers actually run.

When work lands that changes user-visible behavior, sweep the relevant manual
page in the same wave of commits — same discipline as the workshop README's
status column.
