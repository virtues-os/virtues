# @tiptap/markdown 3.31.4 against the page converter

**Measured 2026-10-21.** The question was whether the official
`@tiptap/markdown` extension could become the pages editor's markdown path
in place of the crate's converter (`convert_for_editor` in the page, and
`markdown.ts` held byte for byte to `render.rs`). It cannot without
rewriting most of it. It loses content on about two in five corpus cases.
This repeats the 2026-09-14 measurement
([editor-kernel-2026-09-14.md](editor-kernel-2026-09-14.md)) against the
version the app now ships, on the document contract's schema instead of
StarterKit.

## Method

The probe was a temporary vitest file in `apps/web/src/lib/document/`. It
loaded `@tiptap/markdown` 3.31.4 (with `marked` 17.0.6) into
`node_modules` without touching `package.json`, and both were removed
afterwards. It built a `MarkdownManager` from `contractExtensions()` with
GFM on, so the core node types used Tiptap's own `renderMarkdown` and
`parseMarkdown`. Two corpora, both invented and both in the repo:

- **Export:** the 190 cases in `crates/virtues-document/tests/corpus/cases.json`.
  Each case's HTML was parsed into the contract schema, then serialized by
  `toMarkdown` (ours, equal to the server's by CI) and by
  `MarkdownManager.serialize`. Tiptap's output was read back through our
  converter (`written` dialect) to see which node types and text survived.
  It was also put through Tiptap's own parse and serialize again to check
  that it was stable.
- **Import:** the 55 inputs in `markdown.json`, read by our converter
  (`paste` dialect) and by `MarkdownManager.parse`, then compared as
  documents.

## Results

| Measure | Count |
|---|---|
| Export equal to ours, trailing newline aside | 87 of 190 |
| Export differs and loses node types or text on read-back | 74 of 190 |
| Export differs in format only | 29 of 190 |
| Export is empty where ours is not | 18 of 190 |
| Tiptap's own export changes on a second round trip | 12 of 190 |
| Import gives the same document as ours | 26 of 55 |
| Import gives the same text as ours | 33 of 55 |
| Import throws | 1 of 55 |

Node types and marks lost on export, by number of cases: footnote 9,
mention 8, proposed insertion 7, proposed deletion 5, audio 4, file 4,
figure 4, veil 4, callout 3, applet 3, hard break 3, video 3. Also an
image's width and underline, which is written as `++text++` and does not
read back.

## Why it loses content

These come from reading `MarkdownManager.ts` in the 3.31.4 package and
match what the probe saw.

- **A node with no markdown handler serializes to an empty string**
  (`renderNodeToMarkdown`). The contract's own nodes (callout, mention,
  footnote, figure, applet, audio, video, file) and the proposal and veil
  marks have none. A page of callouts exports as nothing.
- **Text is entity-encoded.** `encodeTextForMarkdown` replaces `&`, `<` and
  `>` with `&amp;`, `&lt;` and `&gt;` in all text outside code. The model
  would read "Fish &amp; chips". Ours writes `Fish & chips \<3`.
- **Ordered lists are renumbered.** A list numbered 2019, 2021, 2024 comes
  back 2019, 2020, 2021. A list counting down 3, 2, 1 comes back 3, 4, 5.
  A list starting at 0 starts at 1. The numbers are content here.
- **Import misses the house dialect.** A callout reads as a quote holding
  the literal `[!tip]`. CriticMarkup keeps both sides of a suggestion.
  Mentions become plain links. A raw HTML block loses its tags. An image
  inside a sentence throws an invalid-content error.

One finding from 2026-09-14 no longer holds. An unknown tag such as
`<time>` is now kept as literal text instead of being deleted
(`htmlAsLiteralText`).

## Conclusion

Format-only differences (table padding, trailing-space hard breaks, tight
lists) would already break the byte-for-byte parity CI checks between the
page's copy and the server's `content` column. That parity is what keeps
the model's edits anchored. The losses rule it out on their own. Adopting
it would mean writing `renderMarkdown` and a `marked` tokenizer for about
twelve node and mark types and overriding text escaping and list
numbering. That work would replace most of the extension and still need
the conformance harness against `render.rs`, which stays because the server
needs the same output.

What would reopen this: the server's markdown paths moving off Rust, or the
extension gaining a pluggable text escaper and list numbering that keeps
each item's number. The probe is easy to rerun with the method above.
