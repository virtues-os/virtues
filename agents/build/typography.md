# Typography — the faces we actually have

What is on disk, what each face may be asked to do, and the one constraint that
surprises every newcomer. The registrations live in
[`apps/web/src/app.css`](../../apps/web/src/app.css); the tokens that name them
live in [`themes.css`](../../apps/web/src/themes.css). This page explains them —
the CSS wins on any disagreement.

## The shelf

Everything is self-hosted in `apps/web/static/fonts/`, not loaded from a CDN.
That is deliberate and not a performance preference: a box runs on a LAN with no
guaranteed egress, and the kiosk panel is Linux, where "it's a system font" is
false for every face below.

| Family | Files | Cuts | Token |
|---|---|---|---|
| JJannon | `JJannon-Display-Regular.woff2` | **one** | `--font-serif`, `--font-serif-ui` |
| Avenir | Regular / Medium / Bold `.woff2` | 400 / 500 / 700 | `--font-sans` |
| IBM Plex Mono | Regular / Medium / SemiBold `.woff2` | 400 / 500 / 600 | `--font-mono` |

Every file is `woff2`. A face that arrives in any other format is converted
before it lands here — on 2026-09-16 the serif fallback was still an
uncompressed 212 KB `.ttf`, larger than the entire rest of the shelf combined,
for a face no user ever saw for more than a swap window.

`/fonts/` is served with a one-week `cache-control` by the box
(`static_cache_control` in `virtues-core/src/server/mod.rs`) and gzipped at
build time by `apps/web/scripts/precompress.mjs` — which walks the directory, so
neither one carries a filename list to update. Nothing else in the repo names a
font file. Adding or removing a face is `app.css` plus the file.

## JJannon ships exactly one cut

**There is no bold serif and no italic serif, and there cannot be one.** This is
the thing to know before designing anything here.

`app.css` registers that single file three times — `JJannon` at 300, `JJannon`
at 400, and a separate family `JJannon UI` with corrected vertical metrics — so
the family reads as richer in the stylesheet than it is on disk. All three are
the same ink.

What follows mechanically, not as preference:

- Ask for `font-weight: 700` and CSS font matching stays **inside** the family
  and returns the one cut. The heading renders regular and nothing warns you.
  Family fallback is reached only for a glyph the face lacks, or while the file
  is still loading — never for weight or style.
- Ask for italic and the browser **synthesizes** an oblique: a mechanical slant
  of the roman, not a drawn italic. It looks like what it is.
- So a serif hierarchy cannot be set with weight or slope. It is set with size,
  case, color and space. The design grammar already asks for exactly that; this
  is why it can.

This is not hypothetical. `AppletCard.svelte` asks for `font-style: italic` on
`var(--font-serif)` in two rules, and several components set the serif at
`font-weight: 500` — the first gets a synthesized slant, the second gets plain
400 with no indication that the request was dropped. Neither is loud enough to
notice in review, which is exactly why the constraint needs writing down rather
than discovering.

The house rules "the serif is never bold" and "no italic serif" are therefore
enforced by the file, not only by taste. Anyone proposing a serif system with
two weights — an outside agency included — is proposing a license purchase and a
new file, not a CSS change.

## Two serif tokens, one face

`--font-serif` is for prose. `--font-serif-ui` is for a serif that shares a row
with something that is not text — an icon, a dot, a control.

The difference is vertical metrics. JJannon declares ascent/descent =
0.740/0.260, exactly one em with no leading in it, which leaves centered letters
0.098em high beside anything drawn to a roomier box. `JJannon UI` overrides the
metrics to fix that; `JJannon` is left alone because the same correction would
move first-baseline position across the wiki, pages and home. The full
derivation is the comment above the `JJannon UI` registration in `app.css` — read
it there rather than restating it here.

Reaching for `--font-serif` in chrome is the common mistake, and it does not look
broken so much as slightly *off*. The eye reads the gap between the two zones,
not the absolute error.

## The fallback stack is a system stack

`--font-serif: 'JJannon', ui-serif, Georgia, 'Times New Roman', serif`

Nothing after the first entry is a webfont, on purpose. The tail is only ever
drawn during `font-display: swap` or for a glyph JJannon lacks, and a system
serif does that instantly for zero bytes. A downloadable fallback is strictly
worse: it is a second file fetched to cover a moment that ends when the first
file arrives.

If a serif fallback ever has to come back — a real glyph-coverage gap, measured —
it comes back as `woff2` and with the gap written down.

## Open: the Avenir license

**Unresolved, and only a human can resolve it.** Avenir is a commercial
Linotype/Monotype typeface. IBM Plex Mono is OFL and safe to redistribute;
JJannon was licensed deliberately. For the three Avenir `.woff2` files there is
nothing on record here either way.

Self-hosting is a heavier use than "installed on this Mac": the webfont is
redistributed from a public repo and from every box we ship. Until someone with
the answer says otherwise, treat the files' presence as evidence of nothing —
don't add weights, don't copy them into another repo or product, and don't
remove them either, because swapping the sans is a design decision rather than a
cleanup. The same warning sits at the `@font-face` block so it is found by
whoever is actually editing.
