<script lang="ts">
	import type { Citation } from "$lib/types/Citation";
	import CitationTooltip from "./CitationTooltip.svelte";
	import Icon from "$lib/components/Icon.svelte";

	// A source badge in a reply: its tool's icon, the source named on hover or
	// focus, the citation panel on click. Markdown.svelte draws one for each
	// `[key]` that names a source and leaves any other brackets as text.
	let { citation, onPanelOpen } = $props<{
		citation: Citation;
		onPanelOpen?: (citation: Citation) => void;
	}>();

	let showTooltip = $state(false);
</script>

<span class="inline-citation-wrapper">
	<button
		type="button"
		class="citation-badge"
		onmouseenter={() => (showTooltip = true)}
		onmouseleave={() => (showTooltip = false)}
		onfocus={() => (showTooltip = true)}
		onblur={() => (showTooltip = false)}
		onclick={() => onPanelOpen?.(citation)}
		aria-label="View source: {citation.label}"
		aria-describedby={showTooltip ? `tooltip-${citation.id}` : undefined}
	>
		<Icon icon={citation.icon} width="12" height="12" />
	</button>

	{#if showTooltip}
		<CitationTooltip {citation} />
	{/if}
</span>

<style>
	.inline-citation-wrapper {
		position: relative;
		display: inline;
	}

	.citation-badge {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 17px;
		height: 17px;
		padding: 0;
		margin: 0 1px;
		font-size: 0.65rem;
		font-weight: 400;
		color: var(--color-foreground-subtle);
		background: var(--color-surface-elevated);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-full);
		vertical-align: text-bottom;
		position: relative;
		top: -4px;
		line-height: 1;
		font-family: inherit;
	}

	.citation-badge :global(svg) {
		display: flex;
		margin: auto;
	}

	.citation-badge {
		cursor: pointer;
		transition: all 0.15s ease;
	}

	.citation-badge:hover {
		background: var(--color-primary-subtle);
		border-color: var(--color-primary);
	}

	.citation-badge:focus {
		outline: none;
		box-shadow: 0 0 0 2px var(--color-primary);
	}

	/* Color classes applied via the color prop. Semantic hues route through
	   the theme so they read correctly on all 16 themes. */
	:global(.text-red-500) {
		color: var(--color-error);
	}
	:global(.text-red-600) {
		color: var(--color-error);
	}
	:global(.text-blue-500) {
		color: var(--color-info);
	}
	:global(.text-blue-600) {
		color: var(--color-info);
	}
	:global(.text-green-500) {
		color: var(--color-success);
	}
	:global(.text-purple-500) {
		color: var(--cat-purple);
	}
	:global(.text-purple-400) {
		color: var(--cat-purple-light);
	}
	:global(.text-indigo-500) {
		color: var(--cat-indigo);
	}
	:global(.text-amber-500) {
		color: var(--color-warning);
	}
	:global(.text-orange-500) {
		color: var(--cat-orange);
	}
	:global(.text-pink-500) {
		color: var(--cat-pink);
	}
	:global(.text-cyan-500) {
		color: var(--cat-cyan);
	}
	:global(.text-cyan-400) {
		color: var(--cat-cyan-light);
	}
	:global(.text-emerald-500) {
		color: var(--cat-emerald);
	}
	:global(.text-emerald-400) {
		color: var(--cat-emerald-light);
	}
	:global(.text-violet-500) {
		color: var(--cat-violet);
	}
	:global(.text-rose-500) {
		color: var(--cat-rose);
	}
	:global(.text-gray-500) {
		color: var(--color-foreground-muted);
	}
	:global(.text-gray-400) {
		color: var(--color-foreground-subtle);
	}
</style>
