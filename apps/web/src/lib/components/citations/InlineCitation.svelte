<script lang="ts">
	import type { Citation } from "$lib/types/Citation";
	import CitationTooltip from "./CitationTooltip.svelte";
	import Icon from "$lib/components/Icon.svelte";

	let { citationId, citation, onPanelOpen } = $props<{
		citationId: string;
		citation?: Citation;
		onPanelOpen?: (citation: Citation) => void;
	}>();

	let showTooltip = $state(false);

	function handleClick() {
		if (citation && onPanelOpen) {
			onPanelOpen(citation);
		}
	}

	function handleMouseEnter() {
		if (citation) {
			showTooltip = true;
		}
	}

	function handleMouseLeave() {
		showTooltip = false;
	}

	function handleFocus() {
		if (citation) {
			showTooltip = true;
		}
	}

	function handleBlur() {
		showTooltip = false;
	}

	function handleKeyDown(e: KeyboardEvent) {
		if ((e.key === "Enter" || e.key === " ") && citation) {
			e.preventDefault();
			handleClick();
		}
	}

	// Determine if this is an active (has data) or pending (streaming) citation
	const isActive = $derived(!!citation);
</script>

<span class="inline-citation-wrapper">
	{#if isActive && citation}
		<!-- Active citation with data - show icon -->
		<button
			class="citation-badge active"
			onmouseenter={handleMouseEnter}
			onmouseleave={handleMouseLeave}
			onfocus={handleFocus}
			onblur={handleBlur}
			onclick={handleClick}
			onkeydown={handleKeyDown}
			aria-label="View source: {citation.label}"
			aria-describedby={showTooltip
				? `tooltip-${citation.id}`
				: undefined}
		>
			<Icon icon={citation.icon} width="12" height="12"
			/>
		</button>

		{#if showTooltip}
			<CitationTooltip {citation} />
		{/if}
	{:else}
		<!-- Pending citation - show number only -->
		<span class="citation-badge pending">{citationId}</span>
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

	/* Active badge (has citation data) - interactive */
	.citation-badge.active {
		cursor: pointer;
		transition: all 0.15s ease;
	}

	.citation-badge.active:hover {
		background: var(--color-primary-subtle);
		border-color: var(--color-primary);
	}

	.citation-badge.active:focus {
		outline: none;
		box-shadow: 0 0 0 2px var(--color-primary);
	}

	/* Pending badge (no data yet) - non-interactive */
	.citation-badge.pending {
		color: var(--color-foreground-muted);
		cursor: default;
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
