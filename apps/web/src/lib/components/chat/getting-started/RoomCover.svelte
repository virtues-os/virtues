<!--
	The room's frontispiece: a watercolor of a scholar's window — ferns and a
	monstera on the sill, botanical prints, a row of books and an inkwell on
	the desk, morning light through old glass.

	It is paint, not a mask. The graphite study that stood here was one ink
	on alpha, so the page could tint it with the foreground token and it
	flipped to chalk on a dark theme for free. A watercolor has its own
	colors, so the trick is different: the white of the paper is knocked out
	to alpha (unmultiplied against white, `tools`-side, once), which is what
	a wash on a page IS — pigment over whatever the paper is. On a light
	theme the picture sits straight on the page and trails off into it at
	the edges, because the painting does. On a dark theme pigment over ink
	would read as a smudge, so the figure lays a paper card under it: a
	tipped-in plate, the way a book carries a color plate on its own stock.
	The card's paper and inset are driven by `--identity-dark`, the flag each
	dark theme already sets beside its palette, so no theme is named here.

	It sits inside the column, at the measure of the words. The oil painting
	that stood here first ran the full width of the pane and made the room
	read as two products stacked.
-->
<script lang="ts">
	let visible = $state(false);
</script>

<figure class="cover" class:visible>
	<img
		src="/covers/getting-started.png"
		alt="A watercolor of a scholar's window: ferns and a monstera on the sill beside botanical prints, a row of books and an inkwell on the desk below, morning light through old glass"
		width="1600"
		height="529"
		decoding="async"
		onload={() => (visible = true)}
	/>
</figure>

<style>
	.cover {
		/* The paper card exists only where `--identity-dark` is 1: on a light
		   theme both the tint and the inset compute to nothing, and the
		   painting sits directly on the page. */
		--plate-dark: var(--identity-dark, 0);
		--plate-paper: #f3efe6;
		margin: 0 0 1.5rem;
		padding: calc(var(--plate-dark) * 14px);
		border-radius: calc(var(--plate-dark) * 6px);
		background: color-mix(in srgb, var(--plate-paper) calc(var(--plate-dark) * 100%), transparent);
		opacity: 0;
		transition: opacity 0.6s ease;
	}
	.cover.visible {
		/* A plate in the margin of the page, not the page's subject. */
		opacity: 0.92;
	}
	.cover img {
		display: block;
		width: 100%;
		height: auto;
		aspect-ratio: 1600 / 529;
	}
	@media (prefers-reduced-motion: reduce) {
		.cover {
			transition: none;
		}
	}
</style>
