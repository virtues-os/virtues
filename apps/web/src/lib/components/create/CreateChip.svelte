<script lang="ts">
	/**
	 * One property in a create dialog (New task, New thread): a pill with an
	 * icon and its value that opens a picker anchored under it. An unset value
	 * reads quieter than a set one. With no `menu` it is a plain button,
	 * for a property that opens something else (a search).
	 */
	import type { Snippet } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import Popover from '$lib/floating/primitives/Popover.svelte';

	let {
		icon,
		label,
		quiet = false,
		color = null,
		open = $bindable(false),
		onclick,
		menu,
		surface = true,
	}: {
		icon: string;
		label: string;
		quiet?: boolean;
		/** The icon's own color, such as a thread's. */
		color?: string | null;
		open?: boolean;
		onclick?: (e: MouseEvent) => void;
		/** The picker it opens. */
		menu?: Snippet<[{ close: () => void }]>;
		/** False for a picker that draws its own surface (the date picker). */
		surface?: boolean;
	} = $props();
</script>

{#snippet pill(toggle: (e: MouseEvent) => void)}
	<button type="button" class="chip" class:quiet class:open aria-haspopup={menu ? 'menu' : undefined} onclick={toggle}>
		<span class="icon" style={color ? `color: ${color}` : undefined}><Icon {icon} width="14" /></span>
		<span class="label">{label}</span>
	</button>
{/snippet}

{#if menu}
	<Popover bind:open placement="bottom-start" offset={4}>
		{#snippet trigger({ toggle })}{@render pill(() => toggle())}{/snippet}
		{#snippet children({ close })}
			{#if surface}
				<div class="menu">{@render menu?.({ close })}</div>
			{:else}
				{@render menu?.({ close })}
			{/if}
		{/snippet}
	</Popover>
{:else}
	{@render pill((e) => onclick?.(e))}
{/if}

<style>
	.chip {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		max-width: 240px;
		height: 28px;
		padding: 0 12px 0 8px;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-full);
		background: transparent;
		font: inherit;
		font-size: var(--text-small);
		color: var(--color-foreground);
		cursor: pointer;
		transition:
			background-color var(--motion-hover-out) var(--ease-premium),
			border-color var(--motion-hover-out) var(--ease-premium);
	}
	.chip:hover,
	.chip.open {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}
	.chip.quiet {
		color: var(--color-foreground-muted);
	}
	.icon {
		display: grid;
		place-items: center;
		flex: none;
		color: var(--color-foreground-muted);
	}
	.label {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	/* The picker's surface: the floating shadow, no border. */
	.menu {
		display: flex;
		flex-direction: column;
		min-width: 224px;
		max-height: 320px;
		overflow-y: auto;
		padding: 4px;
		border-radius: var(--radius-lg);
		background: var(--color-background);
		box-shadow: var(--shadow-float);
	}
	@media (prefers-reduced-motion: reduce) {
		.chip {
			transition: none;
		}
	}
</style>
