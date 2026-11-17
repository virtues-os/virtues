<script lang="ts">
	/**
	 * The ∴ at the left of the window header: the app's mark, and the button
	 * that shows and hides the sidebar (⌘B).
	 *
	 * It sits on the sidebar's glass, which reaches up into the header
	 * (UnifiedSidebar). Closed, it is the top of the rail. Open, it widens
	 * into the sidebar's first row: the ∴ on the column the rows' icons
	 * stand on, and Virtues where their labels start. The window header
	 * places it and gives it its width. In the Mac app, where the window's
	 * buttons take the start of that row, it is a square with no name: at
	 * the row's end when open, on its own glass beside the buttons when
	 * closed.
	 *
	 * Pointing at it turns the mark into its own sidebar glyph: the three
	 * dots line up as a column, the list, and a hairline draws down beside
	 * them, the edge of the panel the list lives in. No dot crosses another
	 * on the way.
	 *
	 * The mark never lights up or moves on its own. The app's opening scales the dots
	 * (`.header-mark .mark-dot`, routes/(app)/+layout.svelte); the hover
	 * moves the groups around them, so the two never fight over a transform.
	 */
	import { sidebarState } from '$lib/stores/sidebarState.svelte';

	let {
		width,
		standalone = false,
		named = true,
	}: { width: number; standalone?: boolean; named?: boolean } = $props();

	const open = $derived(!sidebarState.collapsed);
	/** Laid out as a row (∴ then the name) only when it shows the name. */
	const asRow = $derived(open && named);
</script>

<button
	type="button"
	class="header-mark"
	class:standalone
	class:sidebar-shown={asRow}
	class:resizing={sidebarState.resizing}
	style:width="{width}px"
	onclick={() => sidebarState.toggle()}
	aria-label={open ? 'Hide the sidebar' : 'Show the sidebar'}
	aria-expanded={open}
	data-tip={asRow ? undefined : open ? 'Hide the sidebar' : 'Show the sidebar'}
	data-tip-key={asRow ? undefined : 'mod+b'}
	style:padding-left="{asRow ? 10 : (width - 16) / 2}px"
>
	<!-- The dots are drawn, not typed: the app icon's geometry (equilateral,
	     side 15, r 3, on the 24-unit box `virtues:logo` uses in icons.ts), so
	     the header, the chat and the Dock show one mark. -->
	<!-- Its own glass, beside the Mac app's window buttons. It is a layer
	     rather than a class on the button so it can fade while the
	     sidebar's glass rises around the ∴ or draws away from it. -->
	<span class="mark-glass lq" aria-hidden="true"></span>
	<span class="mark-slot">
	<svg class="mark" viewBox="0 0 24 24" width="16" height="16" aria-hidden="true" focusable="false">
		<path class="edge" d="M16 3.5V20.5" pathLength="1" />
		<g class="move apex"><circle class="mark-dot apex" cx="12" cy="5" r="3" /></g>
		<g class="move left"><circle class="mark-dot left" cx="4.5" cy="18" r="3" /></g>
		<g class="move right"><circle class="mark-dot right" cx="19.5" cy="18" r="3" /></g>
	</svg>
	</span>
	{#if named}
		<span class="mark-label" aria-hidden="true">Virtues</span>
	{/if}
</button>

<style>
	/* As tall as a tab. Laid out like a sidebar row: the ∴, as large as a
	   row's icon, then the name where a row's label starts. Closed it is as
	   wide as a rail row, with the ∴ in the middle (the inset is set inline
	   from the width). Its width and the inset ease on the sidebar's clock,
	   so the ∴ moves with the icons below it. */
	.header-mark {
		display: flex;
		align-items: center;
		gap: var(--sidebar-interactive-gap);
		height: var(--chrome-tab-h);
		flex: none;
		padding: 0;
		overflow: hidden;
		border: none;
		border-radius: var(--radius-lg);
		color: var(--color-foreground);
		white-space: nowrap;
		cursor: pointer;
		transition:
			width var(--motion-sidebar) var(--ease-sidebar),
			padding-left var(--motion-sidebar) var(--ease-sidebar),
			border-radius var(--motion-sidebar) var(--ease-sidebar),
			background-color var(--motion-hover-out) var(--ease-premium),
			box-shadow var(--motion-hover-out) var(--ease-premium),
			transform var(--motion-release) var(--ease-glass);
	}

	.header-mark {
		position: relative;
		background: transparent;
	}

	/* On the sidebar's glass it has no surface of its own. */
	.header-mark:not(.standalone) {
		border-radius: var(--sidebar-interactive-radius);
	}

	/* Standing alone it has its own glass (`.lq`). Opening, the glass goes
	   quickly as the sidebar's glass comes up around the ∴. Closing, it
	   comes back once the sidebar's glass has drawn away. */
	.mark-glass {
		position: absolute;
		inset: 0;
		border-radius: inherit;
		pointer-events: none;
		opacity: 0;
		transition: opacity 120ms var(--ease-premium);
	}

	.standalone .mark-glass {
		opacity: 1;
		transition: opacity 200ms var(--ease-premium) 160ms;
	}

	.standalone:hover .mark-glass {
		box-shadow: var(--lq-rim), var(--lq-hover-wash), var(--lq-lift); /* design-ok: glass under the pointer */
	}

	/* The glint where the pointer is, as on glass buttons (app.css). The
	   root layout keeps --lq-x and --lq-y on this button. */
	.mark-glass::after {
		content: "";
		position: absolute;
		inset: 0;
		border-radius: inherit; /* design-ok: takes its element's radius */
		background: radial-gradient(circle at var(--lq-x, 50%) var(--lq-y, 0%), var(--lq-glint), transparent 65%);
		opacity: 0;
		transition: opacity var(--motion-hover-out) var(--ease-premium);
	}

	.standalone:hover .mark-glass::after {
		opacity: 1;
		transition-duration: var(--motion-hover-in);
	}

	.mark-slot,
	.mark-label {
		position: relative;
	}

	.header-mark.resizing {
		transition: none;
	}

	/* On the sidebar's glass it lights as a row does, by the glass's own
	   light under the pointer (UnifiedSidebar). Standing alone, its own
	   glass brightens instead (`.mark-glass`). */

	/* The square sinks under a press and springs back. The open row does
	   not: a full-width row that shrank would read as a wobble. */
	.header-mark:not(.sidebar-shown):active {
		transform: scale(0.92);
		transition-duration: var(--motion-sidebar), var(--motion-sidebar), var(--motion-sidebar), var(--motion-hover-in), var(--motion-hover-in), var(--motion-press);
	}

	.mark-slot {
		flex: none;
		width: var(--sidebar-interactive-icon-size);
		height: var(--sidebar-interactive-icon-size);
		display: grid;
		place-items: center;
	}

	/* The name, in the serif the app writes its titles in. It comes and
	   goes with the sidebar's words (UnifiedSidebar): in after the sidebar
	   is partly open, out before its edge reaches it. */
	.mark-label {
		font-family: var(--font-serif);
		font-size: 16px;
		line-height: 1;
		letter-spacing: -0.005em;
		opacity: 0;
		transform: translateX(-4px);
		transition:
			opacity 100ms ease-in,
			transform 100ms ease-in;
	}

	.sidebar-shown .mark-label {
		opacity: 1;
		transform: none;
		transition:
			opacity 240ms var(--ease-premium) 130ms,
			transform 280ms var(--ease-premium) 130ms;
	}

	/* Inside the button's own shape, as on a sidebar row. */
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
		/* 1.25px of ink on a 16px glyph: 1.25 × 24 / 16. */
		stroke-width: 1.875;
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

	@media (prefers-reduced-motion: reduce) {
		.header-mark,
		.mark-glass,
		.mark-label,
		.move,
		.edge {
			transition: none !important;
		}

		.header-mark:active {
			transform: none;
		}
	}
</style>
