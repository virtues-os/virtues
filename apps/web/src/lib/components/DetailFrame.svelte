<script lang="ts">
	/**
	 * The frame of a page with a side column: a thread's, a task's or an
	 * insight's page. It holds a `<main>` and an `<aside>`, at the same wide
	 * measure and gutters as the list they open from (Page's `full` width), so
	 * the left edge does not move when you open a row. The side column is one
	 * width everywhere, and where the page is narrow (a phone, or one side of
	 * a split window) it goes under the main column.
	 *
	 * You can change the width from the page bar (stores/pageWidth.svelte.ts).
	 * At Reading the side column goes under the main column too, since a
	 * 280px column beside a reading measure leaves the text too narrow.
	 */
	import type { Snippet } from 'svelte';
	import { usePageWidth } from '$lib/stores/pageWidth.svelte';

	let { children }: { children: Snippet } = $props();

	const width = usePageWidth(() => 'full');
</script>

<div class="detail-box">
	<div class="detail-frame" class:stacked={width.width === 'reading'} style:--frame-width={width.css}>
		{@render children()}
	</div>
</div>

<style>
	/* The frame stacks by the width it is given, which in a split window is
	   half the screen's. */
	.detail-box {
		container-type: inline-size;
	}
	.detail-frame {
		position: relative;
		display: grid;
		grid-template-columns: minmax(0, 1fr) 280px;
		gap: 64px;
		/* The width is the text's, as on every other page; the gutters go on top. */
		--frame-gutter: 48px;
		max-width: calc(var(--frame-width) + 2 * var(--frame-gutter));
		margin: 0 auto;
		padding: 48px var(--frame-gutter) 96px;
		transition: max-width var(--motion-hover-out, 0.2s) var(--ease-premium, ease-out);
	}
	.detail-frame > :global(main) {
		min-width: 0;
	}
	.detail-frame.stacked {
		grid-template-columns: minmax(0, 1fr);
		gap: 40px;
	}
	@container (max-width: 900px) {
		.detail-frame {
			grid-template-columns: minmax(0, 1fr);
			gap: 40px;
			--frame-gutter: 20px;
			padding: 40px var(--frame-gutter) 64px;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.detail-frame {
			transition: none;
		}
	}
</style>
