<script lang="ts">
	/**
	 * One property in the preview column: an icon, then its value. With
	 * `onclick` the whole row is the control; otherwise the value may hold
	 * its own (a home picker, a date picker). An empty value is `quiet`.
	 */
	import type { Snippet } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';

	let {
		icon,
		iconColor = null,
		quiet = false,
		title,
		trailing,
		onclick,
		children,
	}: {
		icon: string;
		/** The icon's own color, such as a thread's. */
		iconColor?: string | null;
		quiet?: boolean;
		/** The row's full words, shown on hover when the row cuts them off. */
		title?: string;
		/** A short word at the row's far end, such as when it happens. */
		trailing?: string;
		onclick?: () => void;
		children: Snippet;
	} = $props();
</script>

{#if onclick}
	<button type="button" class="row" class:quiet data-tip={title} {onclick}>
		<span class="icon" style={iconColor ? `color: ${iconColor}` : undefined}><Icon {icon} width="16" /></span>
		<span class="value">{@render children()}</span>
		{#if trailing}<span class="trailing">{trailing}</span>{/if}
	</button>
{:else}
	<div class="row" class:quiet data-tip={title}>
		<span class="icon" style={iconColor ? `color: ${iconColor}` : undefined}><Icon {icon} width="16" /></span>
		<span class="value">{@render children()}</span>
		{#if trailing}<span class="trailing">{trailing}</span>{/if}
	</div>
{/if}

<style>
	.row {
		display: flex;
		align-items: center;
		gap: 12px;
		width: calc(100% + 16px);
		min-height: 32px;
		margin: 0 -8px;
		padding: 0 8px;
		box-sizing: border-box;
		border: 0;
		border-radius: var(--radius-md);
		background: transparent;
		font: inherit;
		font-size: var(--text-label);
		line-height: 20px;
		text-align: left;
		color: var(--color-foreground);
	}
	button.row {
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	button.row:hover {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}
	.icon {
		display: grid;
		place-items: center;
		flex: none;
		width: 16px;
		color: var(--color-foreground-muted);
	}
	/* The 4px inset leaves room for a control's hover fill (the home
	   picker's), which the clip would otherwise cut at the edges. */
	.value {
		min-width: 0;
		flex: 1 1 auto;
		margin: 0 -4px;
		padding: 0 4px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.trailing {
		flex: none;
		font-size: var(--text-small);
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
	}
	.quiet .value {
		color: var(--color-foreground-subtle);
	}
	@media (prefers-reduced-motion: reduce) {
		button.row {
			transition: none;
		}
	}
</style>
