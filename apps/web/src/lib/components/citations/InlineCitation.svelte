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
		border-radius: 9999px;
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

	/* Color classes from Tailwind - applied via the color prop */
	:global(.text-red-500) {
		color: #ef4444;
	}
	:global(.text-red-600) {
		color: #dc2626;
	}
	:global(.text-blue-500) {
		color: #3b82f6;
	}
	:global(.text-blue-600) {
		color: #2563eb;
	}
	:global(.text-green-500) {
		color: #22c55e;
	}
	:global(.text-purple-500) {
		color: #a855f7;
	}
	:global(.text-purple-400) {
		color: #c084fc;
	}
	:global(.text-indigo-500) {
		color: #6366f1;
	}
	:global(.text-amber-500) {
		color: #f59e0b;
	}
	:global(.text-orange-500) {
		color: #f97316;
	}
	:global(.text-pink-500) {
		color: #ec4899;
	}
	:global(.text-cyan-500) {
		color: #06b6d4;
	}
	:global(.text-cyan-400) {
		color: #22d3ee;
	}
	:global(.text-emerald-500) {
		color: #10b981;
	}
	:global(.text-emerald-400) {
		color: #34d399;
	}
	:global(.text-violet-500) {
		color: #8b5cf6;
	}
	:global(.text-rose-500) {
		color: #f43f5e;
	}
	:global(.text-gray-500) {
		color: #6b7280;
	}
	:global(.text-gray-400) {
		color: #9ca3af;
	}
</style>
