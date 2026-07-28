<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { isAppleKeyboard } from "$lib/utils/platform";

	interface Props {
		collapsed?: boolean;
		animationDelay?: number;
		/** Opens the search / command modal. */
		onSearch?: () => void;
	}

	let { collapsed = false, animationDelay = 0, onSearch }: Props = $props();

	// The mark, then the search. Identity above utility.
	//
	// Two wrong versions preceded this one, and they were wrong the same way.
	// First the ∴ was a button that went Home — a wordmark behaving like a
	// control, which is a pattern used nowhere else in software, so nobody read
	// it as the way home. Then it was replaced with a bordered, input-shaped
	// search field, which was worse: a border is a hard edge and nothing else
	// in the sidebar has one, so a utility you touch twenty times a day became
	// the highest-contrast object in the panel, outranking the user's entire
	// life beneath it.
	//
	// Both mistakes were the same mistake: the top of the sidebar is the most
	// valuable space in the app, and putting a *control* there spends it.
	//
	// So: the mark returns and is inert — no hover, no cursor, no tab stop. It
	// is the one place the serif appears in the chrome, which concentrates the
	// typographic identity in a single deliberate spot instead of spreading it
	// thin. Search sits below it as an ordinary row, borderless, with its hint
	// revealed on hover rather than pinned open.
	const hint = $derived(isAppleKeyboard ? "⌘K" : "Ctrl K");
</script>

<div class="masthead" class:collapsed>
	<!-- Inert. aria-hidden because "∴ virtues" read aloud between the window
	     title and the first destination is noise, not information. -->
	<div class="mark animate-row" style="animation-delay: {animationDelay}ms" aria-hidden="true">
		<span class="mark-glyph">∴</span><span class="mark-word">virtues</span>
	</div>

	<button
		type="button"
		class="search-row animate-row"
		style="animation-delay: {animationDelay + 30}ms"
		onclick={() => onSearch?.()}
		aria-label="Search"
	>
		<Icon icon="ri:search-line" width="15" />
		<span class="search-label">Search</span>
		<kbd>{hint}</kbd>
	</button>
</div>

<style>
	@keyframes fadeSlideIn {
		from {
			opacity: 0;
			transform: translateX(-8px);
		}
		to {
			opacity: 1;
			transform: translateX(0);
		}
	}

	/* Two things have to agree for the seam to read straight, and only fixing
	   one of them fixes nothing:
	     · the row HEIGHT (--chrome-row-h), previously 30 here and 40 there;
	     · the row's OFFSET from the top of the window. The pane is a card inset
	       by --pane-inset; the sidebar is full-bleed. Matching heights while the
	       sidebar started 13px higher left the centrelines exactly as far apart
	       as before. */
	/* The mark occupies the chrome row, so it and the pane toolbar share a
	   centreline across the seam; search sits below in the panel proper. */
	.masthead {
		padding: 0 8px;
		margin-top: var(--pane-inset);
		display: flex;
		flex-direction: column;
		box-sizing: border-box;
	}

	.mark {
		display: flex;
		align-items: center;
		gap: 6px;
		height: var(--chrome-row-h);
		padding: 0 calc(var(--sidebar-padding-left-base) - 8px + 2px);
		user-select: none;
		color: var(--color-foreground);
	}

	.mark-glyph {
		font-family: var(--font-serif, serif);
		font-size: 17px;
		line-height: 1;
	}

	.mark-word {
		font-family: var(--font-serif, serif);
		font-size: 15px;
		line-height: 1;
		letter-spacing: 0.01em;
	}

	.masthead.collapsed {
		opacity: 0;
		transform: translateX(-8px);
		pointer-events: none;
		transition:
			opacity 150ms var(--ease-premium),
			transform 150ms var(--ease-premium);
	}

	.animate-row {
		animation: fadeSlideIn 200ms var(--ease-premium) backwards;
	}

	/* A row, not a field. No border — a border would be the only hard edge in
	   the panel and would make search the loudest thing on the desk. It matches
	   the nav rows below it in height, padding and radius, so it reads as the
	   first row of the list rather than as a control bolted above it. */
	.search-row {
		display: flex;
		align-items: center;
		gap: var(--sidebar-interactive-gap);
		width: 100%;
		height: var(--sidebar-interactive-height);
		padding: 0 var(--sidebar-padding-left-base);
		margin-left: -2px;
		border: none;
		border-radius: var(--sidebar-interactive-radius);
		background: transparent;
		color: var(--color-foreground-subtle);
		font: inherit;
		font-size: var(--sidebar-interactive-font-size);
		text-align: left;
		cursor: pointer;
		transition:
			background 150ms ease,
			color 150ms ease;
	}

	.search-row :global(svg) {
		opacity: var(--sidebar-icon-opacity);
	}

	.search-row:hover {
		background: var(--sidebar-hover-bg);
		color: var(--color-foreground);
	}

	.search-row:hover :global(svg) {
		opacity: 1;
	}

	.search-row:focus-visible {
		outline: 2px solid var(--color-border-focus);
		outline-offset: 1px;
	}

	.search-label {
		flex: 1;
		min-width: 0;
	}

	/* Revealed on hover, not pinned open. A shortcut hint is a reminder for the
	   moment you're reaching for the thing — permanently visible, it is one more
	   object competing for attention every time you look at the panel. Bare
	   type, no filled chip, for the same reason. */
	kbd {
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		line-height: 1;
		letter-spacing: 0.02em;
		color: var(--color-foreground-disabled);
		opacity: 0;
		transition: opacity 150ms ease;
	}

	.search-row:hover kbd,
	.search-row:focus-visible kbd {
		opacity: 1;
	}

	@media (prefers-reduced-motion: reduce) {
		.animate-row {
			animation: none;
		}
		.search-row {
			transition: none;
		}
	}
</style>
