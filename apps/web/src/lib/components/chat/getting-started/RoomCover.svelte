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
		width="1600"
		height="528"
		fetchpriority="high"
		onload={() => (loaded = true)}
	/>
</figure>

<style>
	/* No border, no radius, no card. A frontispiece is not a component
	   pasted onto a page — it IS the top of the page, so the paint runs to
	   the column's edges and dissolves into the paper where the words
	   begin. Masked rather than faded with an overlay, so it works on any
	   ground, light or dark. */
	.cover {
		margin: 0 0 2rem;
	}
	img {
		display: block;
		width: 100%;
		height: auto;
		max-height: 13rem;
		object-fit: cover;
		object-position: center 42%;
		opacity: 0;
		transition: opacity 0.5s ease;
		-webkit-mask-image:
			linear-gradient(to bottom, #000 62%, transparent 100%),
			linear-gradient(to right, transparent 0, #000 5%, #000 95%, transparent 100%);
		mask-image:
			linear-gradient(to bottom, #000 62%, transparent 100%),
			linear-gradient(to right, transparent 0, #000 5%, #000 95%, transparent 100%);
		-webkit-mask-composite: source-in;
		mask-composite: intersect;
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
