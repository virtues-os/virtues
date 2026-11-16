<script lang="ts">
	/**
	 * The ∴ at the left of the window header: the app's mark, and the button
	 * that shows and hides the sidebar (⌘B).
	 *
	 * Pointing at it turns the mark into its own sidebar glyph: the three
	 * dots line up as a column, the list, and a hairline draws down beside
	 * them, the edge of the panel the list lives in. No dot crosses another
	 * on the way. While the sidebar is shown, the space between the dots and
	 * the edge tints as well, so the glyph says which way the click goes.
	 *
	 * What your server is doing and what needs you live in the status
	 * capsule at the other end of the header (StatusCapsule), so the mark
	 * never lights up or moves on its own. The app's opening scales the dots
	 * (`.header-mark .mark-dot`, routes/(app)/+layout.svelte); the hover
	 * moves the groups around them, so the two never fight over a transform.
	 */
	import { sidebarState } from '$lib/stores/sidebarState.svelte';

	const open = $derived(!sidebarState.collapsed);
</script>

<button
	type="button"
	class="header-mark"
	class:sidebar-shown={open}
	onclick={() => sidebarState.toggle()}
	aria-label={open ? 'Hide the sidebar' : 'Show the sidebar'}
	aria-expanded={open}
	data-tip={`${open ? 'Hide' : 'Show'} the sidebar`}
	data-tip-key="mod+b"
>
	<!-- The dots are drawn, not typed: the app icon's geometry (equilateral,
	     side 15, r 3, on the 24-unit box `virtues:logo` uses in icons.ts), so
	     the header, the chat and the Dock show one mark. -->
	<svg class="mark" viewBox="0 0 24 24" width="17" height="17" aria-hidden="true" focusable="false">
		<rect class="panel" x="3.5" y="2" width="12.5" height="20" rx="3" />
		<path class="edge" d="M16 3.5V20.5" pathLength="1" />
		<g class="move apex"><circle class="mark-dot apex" cx="12" cy="5" r="3" /></g>
		<g class="move left"><circle class="mark-dot left" cx="4.5" cy="18" r="3" /></g>
		<g class="move right"><circle class="mark-dot right" cx="19.5" cy="18" r="3" /></g>
	</svg>
</button>

<style>
	.header-mark {
		display: grid;
		place-items: center;
		width: 28px;
		height: 28px;
		flex: none;
		padding: 0;
		border: none;
		border-radius: var(--radius-md);
		background: transparent;
		color: var(--color-foreground);
		cursor: pointer;
		/* The scale's motion: hover in fast and out slow; a press sinks to
		   94% and springs back (design.md, "The scale"). */
		transition:
			background-color var(--motion-hover-out) var(--ease-premium),
			transform var(--motion-release) var(--ease-premium);
	}

	.header-mark:hover {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in), var(--motion-release);
	}

	.header-mark:active {
		background: var(--press-bg);
		transform: scale(0.94);
		transition-duration: var(--motion-press);
	}

	.header-mark:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}

	.mark {
		display: block;
		/* The app opening scales the dots past the 24-box. */
		overflow: visible;
	}

	.mark-dot {
		fill: currentColor;
		transform-box: fill-box;
		transform-origin: center;
	}

	/* ── the morph ─────────────────────────────────────────────────────
	   The column stands at x 7.5 on the triangle's own rows (y 5, 11.5,
	   18), and the edge at x 16, so the glyph sits near the button's centre.
	   The apex slides straight left to the top row, the bottom-left dot
	   straight right to the bottom row, and the bottom-right dot rises into
	   the middle row. Each shrinks to r 2 so the column has air. */
	.move {
		transform-box: fill-box;
		transform-origin: center;
		transition: transform var(--motion-layout) var(--ease-premium);
	}

	.header-mark:is(:hover, :focus-visible) .move.apex {
		transform: translate(-4.5px, 0) scale(0.667);
	}

	.header-mark:is(:hover, :focus-visible) .move.left {
		transform: translate(3px, 0) scale(0.667);
		transition-delay: 20ms;
	}

	.header-mark:is(:hover, :focus-visible) .move.right {
		transform: translate(-12px, -6.5px) scale(0.667);
		transition-delay: 40ms;
	}

	/* The edge draws down once the moving dot has crossed its line, and on
	   the way back it lifts first, so the dot never passes through it. */
	.edge {
		fill: none;
		stroke: var(--color-foreground-muted);
		/* 1.25px of ink on a 17px glyph: 1.25 × 24 / 17. */
		stroke-width: 1.765;
		stroke-linecap: round;
		stroke-dasharray: 1;
		stroke-dashoffset: 1;
		opacity: 0;
		transition:
			stroke-dashoffset var(--motion-state) var(--ease-premium),
			opacity var(--motion-state) var(--ease-premium);
	}

	.header-mark:is(:hover, :focus-visible) .edge {
		stroke-dashoffset: 0;
		opacity: 1;
		transition-duration: 220ms, 80ms;
		transition-delay: 110ms;
	}

	/* The panel the list lives in, tinted while the sidebar is shown. */
	.panel {
		fill: currentColor;
		opacity: 0;
		transition: opacity var(--motion-state) var(--ease-premium);
	}

	.header-mark.sidebar-shown:is(:hover, :focus-visible) .panel {
		opacity: 0.1;
		transition: opacity var(--motion-sidebar) var(--ease-sidebar) 120ms;
	}

	@media (prefers-reduced-motion: reduce) {
		.header-mark,
		.move,
		.edge,
		.panel {
			transition: none !important;
		}

		.header-mark:active {
			transform: none;
		}
	}
</style>
