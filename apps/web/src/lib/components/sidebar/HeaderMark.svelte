<!--
	The ∴ mark at the head of the window header: the app's identity, above
	the rail. It is a mark only. The sidebar has its own button beside it.
-->

<span class="header-mark" aria-hidden="true">
	<!-- Drawn, not typed. A typed ∴ glyph is text-weight — a 21px glyph
	     put a 12px figure with 2px dots over a column of 20px line icons. The
	     geometry is the app icon's and ThinkingMark's, exactly: equilateral,
	     side 15, r 3, on the 24-unit box `virtues:logo` uses in icons.ts, so
	     the header, the chat, and the Dock all show one mark. -->
	<svg viewBox="0 0 24 24" width="17" height="17" focusable="false">
		<circle class="mark-dot apex" cx="12" cy="5" r="3" />
		<circle class="mark-dot left" cx="4.5" cy="18" r="3" />
		<circle class="mark-dot right" cx="19.5" cy="18" r="3" />
	</svg>
</span>

<style>
	.header-mark {
		display: grid;
		place-items: center;
		width: 32px;
		height: 28px;
		flex: none;
		color: var(--color-foreground);
	}

	.header-mark svg {
		display: block;
		/* The app opening scales the dots past the 24-box. */
		overflow: visible;
	}

	.mark-dot {
		fill: currentColor;
		transform-box: fill-box;
		transform-origin: center;
	}
</style>
