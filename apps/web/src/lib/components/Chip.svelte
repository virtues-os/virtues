<!--
	Chip.svelte

	A small pill: a filter you pick, a link to somewhere, or a plain label.

	One element per job, chosen by the props given:
	- `href`: a link, in the accent because it is pressable. An `onclick` with
	  it runs first, so a view can open an in-app route in a tab.
	- `onclick` alone: a button. With `selected` it is a filter in a row of
	  filters, and says so through `aria-pressed`.
	- neither: a label on the fill, read and never pressed.

	Selected is the selected wash (`--active-bg`), never weight or the accent:
	design.md, "Grays" and "Type".
-->
<script lang="ts">
	import type { Snippet } from 'svelte';

	let {
		selected = false,
		href,
		onclick,
		children,
	}: {
		selected?: boolean;
		href?: string;
		onclick?: (e: MouseEvent) => void;
		children: Snippet;
	} = $props();
</script>

{#if href}
	<a class="chip link" {href} {onclick}>{@render children()}</a>
{:else if onclick}
	<button type="button" class="chip filter" class:selected aria-pressed={selected} {onclick}>
		{@render children()}
	</button>
{:else}
	<span class="chip label">{@render children()}</span>
{/if}

<style>
	.chip {
		display: inline-flex;
		align-items: center;
		height: 28px;
		box-sizing: border-box;
		padding: 0 12px;
		border: 1px solid transparent;
		border-radius: var(--radius-full);
		font: inherit;
		font-size: var(--text-small);
		line-height: 1;
		white-space: nowrap;
		text-decoration: none;
	}

	.filter {
		border-color: var(--color-border);
		background: none;
		color: var(--color-foreground-muted);
		cursor: pointer;
		transition: background var(--motion-hover-out) var(--ease-premium);
	}

	.filter:hover:not(.selected) {
		background: var(--hover-bg);
		color: var(--color-foreground);
	}

	.filter.selected {
		border-color: transparent;
		background: var(--active-bg);
		color: var(--color-foreground);
	}

	.filter:active {
		background: var(--press-bg);
	}

	.link {
		background: var(--fill-bg);
		color: var(--color-primary);
	}

	.link:hover {
		color: var(--color-primary-hover);
	}

	.label {
		background: var(--fill-bg);
		color: var(--color-foreground-muted);
	}

	@media (prefers-reduced-motion: reduce) {
		.filter {
			transition: none;
		}
	}
</style>
