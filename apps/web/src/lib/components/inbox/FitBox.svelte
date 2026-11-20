<script lang="ts">
	/**
	 * A drawing of a fixed size, scaled to fit the space it is given. The
	 * radar, the graph and the map are each laid out in their own fixed
	 * pixels, so their geometry is plain arithmetic.
	 *
	 * A round drawing is centered and keeps its own air inside its box. A
	 * `sheet` has straight edges (the map), so it sits where
	 * the other views' content does: 24px in from each side, which lines its
	 * edges up with the text on the line above it, and 16px under that line.
	 */
	import type { Snippet } from 'svelte';

	let {
		w,
		h,
		max = 1.35,
		sheet = false,
		children,
	}: {
		w: number;
		h: number;
		/** The most it grows, so a drawing in a very large pane does not go soft. */
		max?: number;
		/** The drawing has straight edges, so it starts at the top and keeps the page's side margin. */
		sheet?: boolean;
		children: Snippet;
	} = $props();

	const TOP = 16;
	const side = $derived(sheet ? 24 : 8);
	let width = $state(0);
	let height = $state(0);
	const scale = $derived(width && height ? Math.min((width - side * 2) / w, (height - (sheet ? TOP * 2 : 8)) / h, max) : 1);
	const left = $derived(Math.max(0, (width - w * scale) / 2));
	const top = $derived(sheet ? TOP : Math.max(0, (height - h * scale) / 2));
</script>

<div class="stage" bind:clientWidth={width} bind:clientHeight={height}>
	<div class="box" style:width="{w}px" style:height="{h}px" style:transform="scale({scale})" style:left="{left}px" style:top="{top}px">
		{@render children()}
	</div>
</div>

<style>
	.stage {
		position: relative;
		flex: 1 1 auto;
		min-height: 0;
		overflow: hidden;
	}
	.box {
		position: absolute;
		transform-origin: 0 0;
	}
</style>
