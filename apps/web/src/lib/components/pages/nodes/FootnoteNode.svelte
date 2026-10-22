<!--
	FootnoteNode: an article's footnote in a block page, an inline atom.

	Drawn as a small raised number, the label's own (`ev-3` is 3). Its note
	shows on hover, on focus, on a click, and on a right-click or a long
	press: the kind and time of the record the sentence stands on, without
	the record's id, as the day page shows it, or a section's time for a
	`cx-` note. It is not editable: it moves, copies and goes whole, as a
	mention does.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import { onContextGesture } from "$lib/components/contextMenu/gesture";
	import type { ViewState } from "$lib/document/nodeviews.svelte";

	let { view }: { view: ViewState } = $props();

	const label = $derived(String(view.attrs.label ?? ""));
	const note = $derived(typeof view.attrs.note === "string" ? view.attrs.note : "");
	/** The number the label ends in, or the label when it ends in none. */
	const number = $derived(/(\d+)$/.exec(label)?.[1] ?? label);

	/** A record's address, `table:id`, which closes an evidence note. */
	const RECORD = /^[a-z_]+:[A-Za-z0-9_-]+$/;

	/** The note as the page shows it: its last part left off when that is a record's address. */
	const shown = $derived.by(() => {
		const parts = note.split(" · ");
		return parts.length > 1 && RECORD.test(parts[parts.length - 1].trim())
			? parts.slice(0, -1).join(" · ")
			: note;
	});

	let open = $state(false);
	let el: HTMLButtonElement;

	onMount(() => {
		onContextGesture(el, () => {
			open = !open;
		});
	});
</script>

<button
	bind:this={el}
	type="button"
	class="doc-footnote"
	class:selected={view.selected}
	class:open
	aria-label={shown ? `Source ${number}: ${shown}` : `Source ${number}`}
	onclick={() => (open = !open)}
	onblur={() => (open = false)}
	><sup>{number}</sup>{#if shown}<span class="note" role="tooltip">{shown}</span>{/if}</button
>

<style>
	.doc-footnote {
		position: relative;
		display: inline-block;
		margin: 0;
		padding: 0 0.15em;
		border: none;
		background: none;
		font: inherit;
		color: var(--color-foreground-muted);
		cursor: pointer;
		outline: none;
	}

	.doc-footnote sup {
		font-size: 0.75em;
		line-height: 0;
	}

	.doc-footnote:focus-visible,
	.doc-footnote.selected {
		outline: 2px solid var(--color-border-focus);
		outline-offset: 1px;
		border-radius: var(--radius-md);
	}

	.note {
		display: none;
		position: absolute;
		bottom: 100%;
		left: 50%;
		z-index: 10;
		width: max-content;
		max-width: 16rem;
		padding: 4px 8px;
		transform: translateX(-50%);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		background: var(--color-surface);
		color: var(--color-foreground);
		font-family: var(--md-body-family);
		font-size: 0.75rem;
		line-height: 1.4;
		white-space: normal;
		pointer-events: none;
	}

	.doc-footnote:hover .note,
	.doc-footnote:focus-visible .note,
	.doc-footnote.open .note {
		display: block;
	}

	@media print {
		.doc-footnote {
			color: inherit;
		}

		.note {
			display: none !important;
		}
	}
</style>
