# bloub (vendored)

`bot/` is the animation engine from [jeremy-prt/bloub](https://github.com/jeremy-prt/bloub)
(MIT, see LICENSE here), an SVG recreation of the x.ai bot avatar: one filled
shape morphing through 14 states, eyes as mask holes, no animation library.
`engine.sample(t)` is a pure function of time — no clock, no DOM.

Vendored as-is at bloub commit of 2026-09-01, minus the vitest test files
(this app doesn't run vitest; the tests live upstream). Comments are in
French — they're the author's measurement notes, kept verbatim so future
diffs against upstream stay clean. Don't edit `bot/` casually; take fixes
from upstream.

`Bloub.svelte` is ours: a Svelte 5 port of upstream's `BloubBot.vue` rendering,
holding one engine state driven by props (`idle`, `thinking`, …). Pointer
gaze-follow and the timeline player were not ported.
