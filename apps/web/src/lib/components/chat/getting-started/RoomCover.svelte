<!--
	The cover at the head of the room: a library opening onto a garden, in
	autumn light. A book's frontispiece, and the only picture in the setup.

	It is the whole product's motif — traditional oil, majestic, slightly
	impressionist, thick paint — settled 2026-09-15 after the earlier
	portrait plates were deleted (composed for a column that no longer
	exists, and never a family). Painted through our own gateway on the
	Image slot, so anything drawn later matches this one.

	Column width, not bled: the plate under the interview's opening is the
	room's one wide thing, and two different widths at the top of one page
	read as an accident. Capped short so the welcome's first sentence is
	still above the fold on a laptop.
-->
<script lang="ts">
	let loaded = $state(false);
</script>

<figure class="cover" class:loaded>
	<img
		src="/covers/getting-started.jpg"
		alt="An oil painting: a college library in autumn light, its double doors open onto a walled garden"
		width="2400"
		height="480"
		fetchpriority="high"
		onload={() => (loaded = true)}
	/>
</figure>

<style>
	/* No border, no radius, no card, no fade, and no gutter: the painting
	   spans the FULL width of the scroller and touches its top edge, so the
	   room opens with a picture rather than with a picture on a page. A soft
	   mask was tried first and read as a cheap vignette; a column-width
	   version read as a component.

	   It is a direct child of the scroller, above the messages column, so
	   "full width" is plain `width: 100%` — no container-query escape, no
	   negative margins, nothing to drift by a scrollbar. The column's own
	   top padding then supplies the air beneath it.

	   The source is composed AT 5:1 rather than cropped into a band — a 3:1
	   painting in this slot lost its ceiling and floor to object-fit and
	   looked stretched. At the pane's usual width the band comes out just
	   under the 18rem cap, so the whole painting is drawn and object-fit
	   never bites. */
	.cover {
		margin: 0;
		width: 100%;
	}
	img {
		display: block;
		width: 100%;
		height: auto;
		max-height: 18rem;
		object-fit: cover;
		object-position: center;
		opacity: 0;
		transition: opacity 0.5s ease;
	}
	.cover.loaded img {
		opacity: 1;
	}
	@media (prefers-reduced-motion: reduce) {
		img {
			transition: none;
		}
	}
</style>
