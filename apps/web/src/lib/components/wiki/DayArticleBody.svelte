<!--
	DayArticleBody.svelte

	The day article's body with its margin.

	Every sentence the writer tagged with evidence opens that evidence when
	clicked: the record's own words in a card beside it (DayEvidence), with a
	way on to the record in Data. A table or a photo opens its evidence the same
	way, as a whole. Nothing in the margin repeats it; the margin holds only the
	context notes (a section's time span), as quiet text.

	Paragraphs are drawn sentence by sentence (DayInline) so each can be its
	own element; headings, tables and figures go through Markdown. On a narrow
	screen the margin drops under its block. The markdown and its footnotes come
	from `parseDayArticle` (lib/wiki/dayArticle.ts).
-->
<script lang="ts">
	import type { Snippet } from "svelte";
	import Markdown from "$lib/components/Markdown.svelte";
	import { veilMarks, type ArticleBlock, type MarginNote } from "$lib/wiki/dayArticle";
	import { veiled } from "$lib/actions/veil";
	import { veil } from "$lib/stores/veil.svelte";
	import DayInline from "./DayInline.svelte";
	import DayEvidence from "./DayEvidence.svelte";

	interface Props {
		blocks: ArticleBlock[];
		/** Open Data on a cited item (`table:id`). */
		oncite?: (ref: string) => void;
		/** Draws a person link inside a sentence (the day's gloss card). */
		person?: Snippet<[{ name: string; url: string }]>;
	}

	let { blocks, oncite, person }: Props = $props();

	const context = (b: ArticleBlock) => b.notes.filter((n) => n.kind === "cx");
	const blockEvidence = (b: ArticleBlock) => b.notes.filter((n) => n.kind === "ev" && n.ref);

	/** The open card: the element it belongs to, and what it shows. */
	let open = $state<{ anchor: HTMLElement; evidence: MarginNote[]; sentence: string } | null>(null);

	function show(anchor: HTMLElement, evidence: MarginNote[], sentence: string) {
		const again = open?.anchor === anchor;
		close();
		if (again) return;
		anchor.classList.add("active");
		open = { anchor, evidence, sentence };
	}

	function close() {
		open?.anchor.classList.remove("active");
		open = null;
	}

	const refsOf = (evidence: MarginNote[]) => evidence.map((n) => n.ref).filter(Boolean).join(" ");

	function onClick(e: MouseEvent, evidence: MarginNote[], sentence: string) {
		// A link inside the sentence is its own target, and a reader selecting
		// words to copy them isn't asking for the source.
		if ((e.target as HTMLElement).closest("button, a")) return;
		if (String(window.getSelection() ?? "").trim()) return;
		show(e.currentTarget as HTMLElement, evidence, sentence);
	}

	function onKey(e: KeyboardEvent, evidence: MarginNote[], sentence: string) {
		if (e.target !== e.currentTarget) return;
		if (e.key !== "Enter" && e.key !== " ") return;
		e.preventDefault();
		show(e.currentTarget as HTMLElement, evidence, sentence);
	}

	function cite(ref: string) {
		close();
		oncite?.(ref);
	}
</script>

<div class="day-body">
	{#each blocks as block, i (i)}
		{@const cx = context(block)}
		{@const ev = blockEvidence(block)}
		{@const marked = veilMarks(block.markdown)}
		<div
			class="row"
			class:row-heading={block.kind === "heading"}
			class:row-table={block.kind === "table"}
			class:row-figure={block.markdown.startsWith("![")}
		>
			<div class="text" use:veiled={{ hiding: veil.hiding, phrases: marked.phrases }}>
				{#if block.kind === "paragraph"}
					<div class="markdown markdown--article">
						<p>
							{#each block.sentences as s, j (j)}{#if j > 0}{" "}{/if}{#if s.evidence.some((n) => n.ref)}<span
										class="s"
										data-refs={refsOf(s.evidence)}
										role="button"
										tabindex="0"
										aria-haspopup="dialog"
										onclick={(e) => onClick(e, s.evidence, s.markdown)}
										onkeydown={(e) => onKey(e, s.evidence, s.markdown)}
									><DayInline markdown={veilMarks(s.markdown).markdown} {person} /></span
								>{:else}<DayInline markdown={veilMarks(s.markdown).markdown} {person} />{/if}{/each}
						</p>
					</div>
				{:else if ev.length}
					<div
						class="s s-block"
						data-refs={refsOf(ev)}
						role="button"
						tabindex="0"
						aria-haspopup="dialog"
						onclick={(e) => onClick(e, ev, block.markdown)}
						onkeydown={(e) => onKey(e, ev, block.markdown)}
					>
						<Markdown content={marked.markdown} refVariant="quiet" variant="article" />
					</div>
				{:else}
					<Markdown content={marked.markdown} refVariant="quiet" variant="article" />
				{/if}
			</div>
			<aside class="margin" aria-label="When">
				{#each cx as n (n.label)}<span class="note">{n.label}</span>{/each}
			</aside>
		</div>
	{/each}
</div>

{#if open}
	<DayEvidence anchor={open.anchor} evidence={open.evidence} sentence={open.sentence} oncite={cite} onclose={close} />
{/if}

<style>
	.day-body {
		display: flex;
		flex-direction: column;
	}

	.row {
		display: grid;
		grid-template-columns: minmax(0, 40rem) 11rem;
		column-gap: 2.25rem;
		align-items: start;
	}

	/* The article's own spacing lives in Markdown's article register; a block
	   here is one paragraph, so its first/last margins are the rhythm. */
	.text :global(.markdown > :first-child) {
		margin-top: 0;
	}

	.row-heading .text :global(h2) {
		margin: 2.25rem 0 0.625rem;
	}

	.row-heading:first-child .text :global(h2) {
		margin-top: 0;
	}

	.row-heading:first-child .margin {
		padding-top: 0.6rem;
	}

	/* Where a citation brings you back to: the passage it came from, marked
	   for a moment and then let go. */
	.text {
		border-radius: 6px;
		transition: background-color 1s ease;
	}

	.row:global(.flash) .text {
		background-color: var(--color-surface-elevated);
		transition: none;
	}

	/* A sentence with evidence carries no mark until it is touched: a faint
	   wash on hover (after a beat, so reading past it doesn't flicker), the
	   highlight while its card is open. */
	.s {
		cursor: pointer;
		border-radius: 3px;
		box-decoration-break: clone;
		-webkit-box-decoration-break: clone;
		transition: background-color 0.12s ease;
	}

	.s:hover {
		background-color: color-mix(in srgb, var(--color-primary) 6%, transparent);
		transition-delay: 0.2s;
	}

	.s:focus-visible {
		outline: 2px solid var(--color-border-focus);
		outline-offset: 1px;
	}

	.s:global(.active) {
		background-color: var(--color-highlight);
		transition-delay: 0s;
	}

	/* Back from Data: the sentence the citation came from, lit and let go. */
	.s:global(.flash) {
		animation: sentence-flash 1.6s ease;
	}

	@keyframes sentence-flash {
		0%,
		40% {
			background-color: var(--color-highlight);
		}
		100% {
			background-color: transparent;
		}
	}

	.s-block {
		display: block;
		border-radius: 8px;
	}

	.s-block:hover {
		background-color: transparent;
	}

	.row-table .s-block :global(tr:hover td) {
		background-color: color-mix(in srgb, var(--color-primary) 5%, transparent);
	}

	.margin {
		display: flex;
		flex-direction: column;
		gap: 0.375rem;
		padding-top: 0.3rem;
		font-family: var(--font-sans);
		font-size: 0.75rem;
		line-height: 1.35;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}

	/* A heading's notes hang beside it without making its row taller, so a
	   second note never pushes the paragraph below away from its heading. */
	.row-heading .margin {
		padding-top: 2.6rem;
		height: 0;
		overflow: visible;
	}

	/* A photo with its caption on the line under it: the caption reads as one,
	   smaller and quieter than the prose around it. */
	.row-figure .text :global(img) {
		display: block;
		width: 100%;
		margin: 0.375rem 0 0.625rem;
		border-radius: 6px;
	}

	.row-figure .text :global(p) {
		font-size: 0.9375rem;
		line-height: 1.45;
		color: var(--color-foreground-muted);
		margin-bottom: 1.75rem;
	}

	.note {
		display: block;
	}

	@media (max-width: 56rem) {
		.row {
			grid-template-columns: minmax(0, 1fr);
		}

		.margin {
			padding-top: 0;
			margin: -0.5rem 0 1rem;
		}

		.row-heading .margin {
			padding-top: 0;
			height: auto;
			margin: -0.375rem 0 0.75rem;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.s,
		.text {
			transition: none;
		}
	}
</style>
