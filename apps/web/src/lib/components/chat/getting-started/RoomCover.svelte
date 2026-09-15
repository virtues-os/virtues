<!--
	The room's frontispiece: a graphite study of the library's garden doors,
	the centre pair standing open.

	It is a MASK, not a picture. The drawing is one ink, so the PNG carries
	only its alpha and the page paints it with the foreground token —
	graphite on paper in the light theme, chalk on ink in the dark one. No
	rectangle, no paper colour of its own to clash with the page, and it
	needs no border or fade to stop looking like a component: the drawing
	simply trails off into the page at its edges, because the study does.

	The oil painting that stood here first ran the full width of the pane
	and made the room read as two products stacked — a cinematic band over a
	column of chat. This sits inside the column, at the measure of the words.
-->
<script lang="ts">
	let visible = $state(false);
	// The mask has no intrinsic paint, so there is no load event to wait on
	// the way an <img> gives one; fade the block in once it is in the DOM.
	$effect(() => {
		const id = requestAnimationFrame(() => (visible = true));
		return () => cancelAnimationFrame(id);
	});
</script>

<figure
	class="cover"
	class:visible
	role="img"
	aria-label="A pencil study of a library's garden doors, the centre pair standing open onto a lawn and a stone urn"
></figure>

<style>
	.cover {
		margin: 0 0 1.5rem;
		width: 100%;
		aspect-ratio: 1600 / 529;
		background-color: var(--color-foreground);
		/* Graphite reads heavy at full strength against body copy; this is a
		   drawing in the margin of the page, not the page's subject. */
		opacity: 0;
		transition: opacity 0.6s ease;
		-webkit-mask-image: url("/covers/getting-started.png");
		mask-image: url("/covers/getting-started.png");
		-webkit-mask-size: contain;
		mask-size: contain;
		-webkit-mask-repeat: no-repeat;
		mask-repeat: no-repeat;
		-webkit-mask-position: center;
		mask-position: center;
	}
	.cover.visible {
		opacity: 0.78;
	}
	@media (prefers-reduced-motion: reduce) {
		.cover {
			transition: none;
		}
	}
</style>
