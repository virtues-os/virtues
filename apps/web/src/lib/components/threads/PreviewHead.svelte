<script lang="ts">
	/**
	 * The top of the Threads door's preview: a tile in the item's color
	 * holding its glyph, the controls on the right, and the title under them.
	 * A thread and a task share it, so the column keeps one shape whichever
	 * row you point at.
	 */
	import type { Snippet } from 'svelte';

	let {
		color = null,
		glyph,
		actions,
		title,
		closed = false,
	}: {
		/** The tile's color, or none for a plain gray tile. */
		color?: string | null;
		glyph: Snippet;
		actions?: Snippet;
		title: string;
		/** A finished task: the title is struck through. */
		closed?: boolean;
	} = $props();
</script>

<div class="head">
	<span class="tile" class:tinted={!!color} style={color ? `--tile: ${color}` : undefined}>
		{@render glyph()}
	</span>
	{#if actions}<span class="actions">{@render actions()}</span>{/if}
</div>
<h2 class="title" class:closed>{title}</h2>

<style>
	.head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		min-height: 32px;
	}
	/* The item's color at a sixth behind its glyph, as a pin wears it. */
	.tile {
		display: grid;
		place-items: center;
		flex: none;
		width: 32px;
		height: 32px;
		border-radius: var(--radius-md);
		background: var(--fill-bg);
		color: var(--color-foreground-muted);
	}
	.tile.tinted {
		background: color-mix(in srgb, var(--tile) 16%, transparent);
		color: var(--tile);
	}
	.actions {
		display: inline-flex;
		align-items: center;
		gap: 4px;
	}
	.title {
		margin: 16px 0 0;
		font-family: var(--font-serif-ui);
		font-weight: 400;
		font-size: var(--text-heading);
		line-height: 28px;
		color: var(--color-foreground);
		overflow-wrap: anywhere;
	}
	.title.closed {
		color: var(--color-foreground-muted);
		text-decoration: line-through;
	}
</style>
