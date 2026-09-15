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
	/* No border, no radius, no card, and no fade — a frontispiece is not a
	   component pasted onto a page, it IS the top of the page. It runs wider
	   than the column, to the same measure as the plate under the
	   interview's opening, so the room has one wide thing and the paint
	   reaches past the words on both sides. Edges are clean: a soft mask was
	   tried and read as a cheap vignette.

	   The scroller is the size container (ChatView sets container-type), so
	   this measures against it, and `.room` keeps layout containment only so
	   the overhang is drawn rather than clipped. */
	.cover {
		margin: 0 0 2.25rem;
		width: min(72rem, calc(100cqw - 3rem));
		max-width: none;
		position: relative;
		left: 50%;
		transform: translateX(-50%);
	}
	img {
		display: block;
		width: 100%;
		height: auto;
		max-height: 15rem;
		object-fit: cover;
		object-position: center 42%;
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
