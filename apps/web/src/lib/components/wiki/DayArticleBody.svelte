<!--
	DayArticleBody.svelte

	The day article's body with its margin. Each block of the page sits beside
	the notes that belong to it, in two kinds only:

	- context (a section's time span): always shown, plain text;
	- evidence (the record a sentence rests on): shown when the paragraph is
	  hovered or focused, always shown beside a table. `↗` means one thing:
	  open Record on that item.

	Captions and tables carry no links; the margin does. On a narrow screen the
	margin drops under its block. The markdown and its footnotes come from
	`parseDayArticle` (lib/wiki/dayArticle.ts).
-->
<script lang="ts">
	import Markdown from "$lib/components/Markdown.svelte";
	import type { ArticleBlock, MarginNote } from "$lib/wiki/dayArticle";

	interface Props {
		blocks: ArticleBlock[];
		/** Open Record on a cited item (`table:id`). */
		oncite?: (ref: string) => void;
	}

	let { blocks, oncite }: Props = $props();

	const context = (b: ArticleBlock) => b.notes.filter((n) => n.kind === "cx");
	const evidence = (b: ArticleBlock) => {
		// One entry per label: a paragraph can cite the same moment twice.
		const seen = new Set<string>();
		return b.notes.filter((n) => n.kind === "ev" && !seen.has(n.label) && seen.add(n.label));
	};
</script>

{#snippet note(n: MarginNote)}
	{#if n.kind === "ev" && n.ref && oncite}
		<button type="button" class="note note-evidence" onclick={() => oncite?.(n.ref as string)}>
			{n.label} <span aria-hidden="true">↗</span>
		</button>
	{:else}
		<span class="note">{n.label}</span>
	{/if}
{/snippet}

<div class="day-body">
	{#each blocks as block, i (i)}
		{@const cx = context(block)}
		{@const ev = evidence(block)}
		<div
			class="row"
			class:row-heading={block.kind === "heading"}
			class:row-table={block.kind === "table"}
		>
			<div class="text">
				<Markdown content={block.markdown} refVariant="quiet" variant="article" />
			</div>
			<aside class="margin" aria-label="Notes on this passage">
				{#each cx as n (n.label)}{@render note(n)}{/each}
				{#if ev.length}
					<div class="evidence" class:always={block.kind === "table"}>
						{#each ev as n (n.label)}{@render note(n)}{/each}
					</div>
				{/if}
			</aside>
		</div>
	{/each}
</div>

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
		margin-top: 2rem;
	}

	.row-heading:first-child .text :global(h2) {
		margin-top: 0;
	}

	.row-heading:first-child .margin {
		padding-top: 0.6rem;
	}

	.margin {
		display: flex;
		flex-direction: column;
		gap: 0.375rem;
		padding-top: 0.3rem;
		font-family: var(--font-sans);
		font-size: 0.6875rem;
		line-height: 1.35;
		color: var(--color-foreground-subtle);
	}

	.row-heading .margin {
		padding-top: 2.6rem;
	}

	.note {
		display: block;
		background: var(--color-surface-elevated);
		border-radius: 6px;
		padding: 0.4375rem 0.625rem;
		text-align: left;
	}

	.note-evidence {
		border: none;
		font: inherit;
		color: var(--color-primary);
		cursor: pointer;
		width: 100%;
	}

	.note-evidence:hover {
		color: var(--color-primary-hover, var(--color-primary));
	}

	/* A paragraph's evidence appears when you are reading it; a table's is
	   always there, because the table is a block the eye lands on whole. */
	.evidence {
		display: flex;
		flex-direction: column;
		gap: 0.375rem;
		opacity: 0;
		transition: opacity 0.15s ease;
	}

	.row:hover .evidence,
	.row:focus-within .evidence,
	.evidence.always {
		opacity: 1;
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
			margin: 0 0 0.5rem;
			order: -1;
		}

		.row-heading .note {
			background: none;
			padding: 0;
		}

		.evidence {
			display: none;
		}

		.evidence.always {
			display: flex;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.evidence {
			transition: none;
		}
	}
</style>
