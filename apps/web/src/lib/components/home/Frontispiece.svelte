<!--
	Frontispiece.svelte — the painting on the right, with words on it.

	The one framed object the app's pages share (2026-09-04): a painting for
	the moment, one line from the bank or from the record itself, and a few
	numbers in white over the painting's dusk. Getting Started shows the
	painting and line for the step at hand and the record's census; Home shows
	the hour's painting, yesterday's own sentence, and today's count. Same
	object, so a person who finishes getting started keeps meeting it.

	It is a card set in the page's margin — 12px, no shadow — never a ground
	behind the text column. Below 900px it becomes a header and the work
	follows. The paintings ship in static/plates until the plate job draws
	them from the record.
-->
<script lang="ts">
	interface Figure { v: string; k: string }
	interface Link { label: string; run: () => void }
	let {
		src,
		line,
		figures = [],
		since = "",
		links = [],
	}: {
		/** The painting. */
		src: string;
		/** The line on it: banked, or the record's own. */
		line: string;
		/** Up to three numbers, each with a label. */
		figures?: Figure[];
		/** One quiet line under the numbers. */
		since?: string;
		/** Up to two ways onward, as white text. */
		links?: Link[];
	} = $props();
</script>

<aside class="front" aria-hidden="true">
	{#key src}
		<img {src} alt="" />
	{/key}
	<div class="text">
		<p class="epigraph">{line}</p>
		{#if figures.length > 0}
			<div class="ledger">
				{#each figures as f (f.k)}
					<div><div class="v">{f.v}</div><div class="k">{f.k}</div></div>
				{/each}
			</div>
		{/if}
		{#if since}<div class="since">{since}</div>{/if}
		{#if links.length > 0}
			<div class="links">
				{#each links as l (l.label)}
					<button type="button" onclick={l.run}>{l.label}</button>
				{/each}
			</div>
		{/if}
	</div>
</aside>

<style>
	/* Sticky in the pane: the card keeps its place while the work scrolls,
	   and is as tall as the pane less its margins — so its words are always
	   in view, on a short page and a long one alike. */
	.front {
		position: sticky; top: 20px; align-self: start;
		height: calc(100dvh - var(--chrome-row-h, 40px) - 2 * var(--pane-inset, 12px) - 2px - 40px);
		min-height: 480px; overflow: hidden;
		margin: 20px 20px 20px 0; border-radius: 12px;
		background: color-mix(in srgb, var(--color-foreground) 40%, var(--color-background));
	}
	@media (max-width: 900px) {
		.front { position: relative; top: 0; order: -1; height: auto; min-height: 320px; margin: 12px; }
	}
	img {
		position: absolute; inset: 0; width: 100%; height: 100%;
		object-fit: cover; object-position: 50% 30%;
		animation: front-in 0.8s ease both;
	}
	.front::after {
		content: ""; position: absolute; inset: 0; pointer-events: none;
		background: linear-gradient(180deg, rgba(20,26,38,0) 38%, rgba(20,26,38,0.55) 68%, rgba(20,26,38,0.86) 100%);
	}
	.text {
		position: absolute; left: 0; right: 0; bottom: 0; z-index: 1;
		padding: 0 48px 48px; color: #fff;
		animation: arrive 0.8s ease both; animation-delay: 200ms;
	}
	@media (max-width: 640px) { .text { padding: 0 24px 28px; } }
	.epigraph {
		font-family: var(--font-serif); font-weight: 400;
		font-size: 26px; line-height: 1.3; letter-spacing: -0.005em;
		max-width: 15em; margin: 0;
	}
	.ledger { display: flex; gap: 32px; margin-top: 32px; padding-top: 24px; border-top: 1px solid rgba(255,255,255,0.28); }
	.v { font-family: var(--font-serif); font-size: 28px; line-height: 1; font-variant-numeric: lining-nums tabular-nums; }
	.k { font-family: var(--font-sans); font-size: 12px; color: rgba(255,255,255,0.72); margin-top: 8px; }
	.since { margin-top: 20px; font-family: var(--font-sans); font-size: 12px; color: rgba(255,255,255,0.6); }
	.links { display: flex; gap: 20px; margin-top: 24px; }
	.links button {
		font-family: var(--font-sans); font-size: 14px; font-weight: 500; color: #fff;
		background: none; border: 0; padding: 0; cursor: pointer; opacity: 0.9;
	}
	.links button:hover { opacity: 1; text-decoration: underline; text-underline-offset: 3px; }

	/* from-only keyframes, per the house rule */
	@keyframes arrive { from { opacity: 0; transform: translateY(6px); } }
	@keyframes front-in { from { opacity: 0; } }
	@media (prefers-reduced-motion: reduce) { .text, img { animation: none; } }
</style>
