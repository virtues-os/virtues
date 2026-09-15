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
	.cover {
		margin: 0 0 1.75rem;
		border-radius: 6px;
		overflow: hidden;
		background: var(--color-surface-elevated, rgba(0, 0, 0, 0.03));
		/* The painting is warm and opaque in both themes — paint does not
		   invert — so a hairline keeps it from floating on a dark ground. */
		border: 1px solid var(--color-border-subtle, var(--color-border));
	}
	img {
		display: block;
		width: 100%;
		height: auto;
		max-height: 13rem;
		object-fit: cover;
		object-position: center 42%;
		opacity: 0;
		transition: opacity 0.4s ease;
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
