<script lang="ts">
	import type { Citation } from '$lib/types/Citation';
	import 'iconify-icon';

	let { citation } = $props<{
		citation: Citation;
	}>();

	// Extract domain from URL for display
	function extractDomain(url: string): string {
		try {
			return new URL(url).hostname.replace('www.', '');
		} catch {
			return url;
		}
	}
</script>

<div
	class="citation-tooltip"
	id="tooltip-{citation.id}"
	role="tooltip"
>
	<div class="tooltip-header">
		<iconify-icon icon={citation.icon} class={citation.color} width="16" height="16"
		></iconify-icon>
		<span class="tooltip-title">{citation.label}</span>
	</div>

	{#if citation.url}
		<div class="tooltip-url">
			<iconify-icon icon="ri:link" width="12" height="12"></iconify-icon>
			<span>{extractDomain(citation.url)}</span>
		</div>
	{/if}

	<div class="tooltip-hint">
		<iconify-icon icon="ri:cursor-line" width="12" height="12"></iconify-icon>
		<span>Click for details</span>
	</div>
</div>

<style>
	.citation-tooltip {
		position: absolute;
		bottom: calc(100% + 8px);
		left: 50%;
		transform: translateX(-50%);
		z-index: 50;
		min-width: 220px;
		max-width: 320px;
		padding: 12px;
		background: var(--color-surface, #ffffff);
		border: 1px solid var(--color-border, #e5e5e5);
		border-radius: 8px;
		box-shadow:
			0 4px 6px -1px rgba(0, 0, 0, 0.1),
			0 2px 4px -2px rgba(0, 0, 0, 0.1);
		pointer-events: none;
		animation: tooltip-fade-in 0.15s ease-out;
	}

	@keyframes tooltip-fade-in {
		from {
			opacity: 0;
			transform: translateX(-50%) translateY(4px);
		}
		to {
			opacity: 1;
			transform: translateX(-50%) translateY(0);
		}
	}

	/* Arrow pointing down */
	.citation-tooltip::after {
		content: '';
		position: absolute;
		top: 100%;
		left: 50%;
		transform: translateX(-50%);
		border: 6px solid transparent;
		border-top-color: var(--color-surface, #ffffff);
	}

	.citation-tooltip::before {
		content: '';
		position: absolute;
		top: 100%;
		left: 50%;
		transform: translateX(-50%);
		border: 7px solid transparent;
		border-top-color: var(--color-border, #e5e5e5);
	}

	.tooltip-header {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-bottom: 4px;
	}

	.tooltip-title {
		font-weight: 500;
		font-size: 0.8125rem;
		color: var(--color-foreground, #171717);
	}

	.tooltip-url {
		display: flex;
		align-items: center;
		gap: 4px;
		font-size: 0.75rem;
		color: var(--color-primary, #3b82f6);
		margin-bottom: 8px;
	}

	.tooltip-hint {
		display: flex;
		align-items: center;
		gap: 4px;
		font-size: 0.6875rem;
		color: var(--color-foreground-muted, #a3a3a3);
		border-top: 1px solid var(--color-border, #e5e5e5);
		padding-top: 8px;
		margin-top: 4px;
	}
</style>
