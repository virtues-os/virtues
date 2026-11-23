<script lang="ts">
	/**
	 * A thing's mark as something you can choose: on the radar, on the graph,
	 * and on the map and its clock. Given a point it sits centered on it;
	 * given none it sits in the flow of its row. It names
	 * itself in a tip on hover, unless the view names it some other way.
	 */
	import { MARK_SIZE, type InboxItem } from '$lib/inbox/items';
	import InboxMark from './InboxMark.svelte';

	let {
		item,
		x = null,
		y = null,
		picked = false,
		faded = false,
		tip = true,
		onPick,
		onHover,
	}: {
		item: InboxItem;
		x?: number | null;
		y?: number | null;
		picked?: boolean;
		/** Drawn faint: the Show filter leaves it out, but its place stays. */
		faded?: boolean;
		/** False where the view shows the name itself when the mark is pointed at. */
		tip?: boolean;
		onPick: (key: string) => void;
		/** The pointer or the keyboard is on this mark (its key), or has left it (null). */
		onHover?: (key: string | null) => void;
	} = $props();

	const size = $derived(MARK_SIZE[item.kind]);
	const placed = $derived(x !== null && y !== null);
</script>

<button
	type="button"
	class="dot"
	class:placed
	class:on={picked}
	class:faded
	style:left={placed ? `${Math.round((x ?? 0) - size / 2)}px` : undefined}
	style:top={placed ? `${Math.round((y ?? 0) - size / 2)}px` : undefined}
	style:width="{size}px"
	style:height="{size}px"
	aria-label={item.title}
	aria-pressed={picked}
	data-key={item.key}
	data-tip={tip ? item.title : undefined}
	onclick={() => onPick(item.key)}
	onmouseenter={() => onHover?.(item.key)}
	onmouseleave={() => onHover?.(null)}
	onfocus={() => onHover?.(item.key)}
	onblur={() => onHover?.(null)}
>
	<InboxMark kind={item.kind} radar />
</button>

<style>
	.dot {
		display: inline-grid;
		flex: none;
		place-items: center;
		padding: 0;
		border: 0;
		border-radius: 50%;
		background: none;
		cursor: pointer;
		transition: opacity var(--motion-state) var(--ease-premium);
	}
	.dot.placed {
		position: absolute;
		z-index: 3;
		transition:
			left var(--motion-layout) var(--ease-premium),
			top var(--motion-layout) var(--ease-premium),
			opacity var(--motion-state) var(--ease-premium);
	}
	/* A halo in the accent: faint under the pointer, stronger on the thing you picked. */
	.dot:hover {
		outline: 4px solid color-mix(in srgb, var(--color-primary) 10%, transparent);
	}
	.dot.on {
		outline: 4px solid color-mix(in srgb, var(--color-primary) 18%, transparent);
	}
	.dot.faded {
		opacity: 0.15;
	}

	@media (prefers-reduced-motion: reduce) {
		.dot,
		.dot.placed {
			transition: none;
		}
	}
</style>
