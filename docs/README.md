# The manual

This directory is the **public documentation** for Virtues. `virtues.com/docs`
renders it, and the server will later serve it as in-app help. Design records,
plans and audits go in [`../agents/`](../agents/), which is written for the
people and agents building Virtues. Pages here are written for the people
*running* it.

## Rules for pages

- **Every page must be listed in `manifest.json`**, in reading order, under its
  section. The website builds its nav, its prerender list, its sitemap entries
  and `llms.txt` from the manifest. A page missing from the manifest does not
  appear on the site, and a listed page with no file must carry
  `"planned": true` or the site build fails.
- **Pages are plain markdown** with frontmatter: `title`, `description`
  (used as the meta description, so write it for search), and optionally
  `updated` (YYYY-MM-DD). Don't use Svelte components. These files also have
  to render on the server and stay readable as raw markdown, because every
  page is served as `<slug>.md` for agents and curl.
- **Slugs are paths.** `operate/upgrading.md` publishes at
  `/docs/operate/upgrading`. `index.md` is the `/docs` landing page.
- **Release notes live in `whats-new/`**, one page per stable tag, listed
  newest first in the manifest. virtues.com/changelog redirects to the first
  one, so a new note goes at the top of its section. Write each from the
  commits between tags, and leave out fixes to bugs no stable release had.
- **Follow the website's `DESIGN.md`.** Keep pages short,
  specific and technical, and get the facts right. Wording follows the
  Virtues Copy Guide, and page structure follows "How pages are shaped" below.
- **Claim only what ships.** A manual page describes the released system. If a
  feature is half-built, its page stays `"planned"` in the manifest until the
  feature lands. In-progress design goes in `../agents/`.

## How pages are shaped

Every page follows the same shape, so readers learn it once.

- **A lean main path first.** The page opens with the steps most readers
  follow, in order, and nothing else. Number the steps when order matters.
- **A "Next steps" heading ends the main path,** with one line pointing to the
  next page.
- **Everything else goes below a horizontal divider (`---`):** troubleshooting,
  alternate paths (such as other ways to install), then explanation of how
  things work.
- **Explanatory sections describe; they don't instruct.** Don't use "To do X,
  do Y", "you need" or "run" there. If something is an action the reader
  should take, it belongs in the steps.
- **Headings:** imperatives for steps ("Step 1: Choose a model"), noun phrases
  for topics ("Embedding server requirements"). Mark an optional step
  "(optional)".
- **Information goes where the reader needs it:** before the step that uses
  it, not after.
- **End a step with its result only when the reader can't see it coming,**
  such as how long a wait takes, or what to keep for the next step ("ends by
  printing a six-digit pairing code"). Skip lines that announce what the
  screen already shows, and don't link to a section that starts right below.
- **Each fact lives on one page.** Other pages link to it instead of repeating
  it.
- **Describe an option as who it's for, what happens, and what to do first.**
- **Every command gets its own code block,** and every check says what a pass
  looks like.
- **Quote the UI exactly:** labels and questions as the app or installer shows
  them.
- **Mark untested paths** with "Not yet tested on real hardware." until someone
  has walked them on a real device.
- **Installation stays strictly in order** (Prerequisites, System
  requirements, Install script, Connecting your devices). Optional setups that
  work before or after installing go in Guides.

## How it publishes

The website repo's `scripts/sync-docs.mjs` copies this directory into its
build: from a local sibling checkout in dev, and from a GitHub tarball pinned
to `VIRTUES_DOCS_REF` on Vercel. That ref is `main`, which moves only when a
release ships, so the published docs describe what servers actually run.
`.github/workflows/docs.yml` triggers the site rebuild, and its trigger and
`VIRTUES_DOCS_REF` have to change together.

When a change alters user-visible behavior, update the relevant manual page in
the same set of commits.
