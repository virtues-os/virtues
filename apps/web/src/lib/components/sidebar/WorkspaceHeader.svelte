<script lang="ts">
	import AtlasIcon from "./AtlasIcon.svelte";
	import { isAppleKeyboard } from "$lib/utils/platform";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { HOME_ROUTE } from "$lib/sidebar/sections";
	import { pinsStore } from "$lib/stores/pins.svelte";

	interface Props {
		collapsed?: boolean;
		animationDelay?: number;
		/** Opens the search / command modal. */
		onSearch?: () => void;
	}

	let { collapsed = false, animationDelay = 0, onSearch }: Props = $props();

	// The masthead is a PATH, and the path is the way home.
	//
	// The earlier wordmark-as-home was reverted for a good reason: a bare mark
	// behaving like a button is a pattern read nowhere as navigation. This is
	// the answer to that objection rather than a repeat of the mistake — the
	// mast reads `∴ Virtues / galilee`, a breadcrumb, and breadcrumb roots are
	// clickable everywhere in software. The root gained a job the moment it
	// gained a tail.
	//
	// The tail appears only for things that carry a bookcloth color (today:
	// notebooks). Routes without a cloth get no tail: a path that narrated
	// every room would be a status bar.
	//
	// The tail carries no dot. The dot's job is to identify a thing in a LIST —
	// on its Desk spine, among other spines, and on its tab among other tabs.
	// A breadcrumb has no siblings to be distinguished from, so the dot there
	// was decoration, and it was stealing width from the one element that
	// actually needs it: the name, which was truncating to "Dog &…".
	const hint = $derived(isAppleKeyboard ? "⌘K" : "Ctrl K");

	const activeTab = $derived.by(() => {
		const pane = windowShellStore.activePane;
		if (!pane) return null;
		return pane.tabs.find((t) => t.id === pane.activeTabId) ?? null;
	});

	// The tail names the pinned thing you're inside. Keyed on the pin list
	// rather than on a route shape, so it follows the Desk wherever the Desk
	// goes — a pinned PDF or applet gets a path segment exactly like a
	// notebook does.
	const tailLabel = $derived.by(() => {
		const route = activeTab?.route;
		if (!route) return null;
		const pin = pinsStore.getByUrl(route);
		if (!pin) return null;
		return pin.label?.trim() || activeTab?.label || null;
	});

	function goHome() {
		windowShellStore.openTabFromRoute(HOME_ROUTE, {
			label: "Home",
			focusExisting: true,
		});
	}
</script>

<div class="masthead" class:collapsed>
	<div
		class="masthead-row animate-row"
		style="animation-delay: {animationDelay}ms"
	>
		<div class="path">
			<!-- The mark, drawn: the JJannon ∴ glyph is text-weight; the mast
			     needs logo weight. Same optical grid as the Atlas icons —
			     16px box, ~12px of ink. -->
			<span class="mark-glyph" aria-hidden="true">
				<svg viewBox="0 0 12 10.5" width="12" height="10.5" fill="currentColor">
					<circle cx="6" cy="2.4" r="1.5" />
					<circle cx="2.6" cy="8.1" r="1.5" />
					<circle cx="9.4" cy="8.1" r="1.5" />
				</svg>
			</span>
			<button type="button" class="mark-word" onclick={goHome} title="Home">
				Virtues
			</button>
			{#if tailLabel}
				<span class="path-sep" aria-hidden="true">/</span>
				<span class="path-tail">{tailLabel}</span>
			{/if}
		</div>
	</div>

	<!-- Search gets its own full-width row rather than an icon crowding the
	     path. It is the most-used control in the panel and it was rendered as
	     the smallest target in it, squeezed against a breadcrumb that needs
	     every pixel of width it can get. A row can also say what it is and
	     what key opens it, which an unlabelled magnifier cannot. -->
	<button
		type="button"
		class="search-row animate-row"
		style="animation-delay: {animationDelay + 20}ms"
		onclick={() => onSearch?.()}
		title="Search ({hint})"
	>
		<AtlasIcon name="search" />
		<span class="search-label">Search</span>
		<span class="search-hint" aria-hidden="true">{hint}</span>
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

	/* One chrome row, shared height with the pane toolbar so the two sides of
	   the seam sit on one centreline. */
	.masthead {
		padding: 0 8px;
		margin-top: var(--pane-inset);
		box-sizing: border-box;
	}

	.masthead-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		height: var(--chrome-row-h);
		padding-left: var(--sidebar-padding-left-base);
	}

	/* Gap and gutter are the row tokens, not hand-picked numbers, so the
	   wordmark lands on exactly the same text edge as every label below it. */
	.path {
		display: flex;
		align-items: center;
		gap: var(--sidebar-interactive-gap);
		min-width: 0;
		user-select: none;
		color: var(--color-foreground);
	}

	.mark-glyph {
		width: 16px;
		flex-shrink: 0;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	/* The root: one size up from the labels below — the mast ranks, quietly. */
	.mark-word {
		font-family: var(--font-serif, serif);
		font-size: 15px;
		font-weight: 400;
		letter-spacing: 0.025em;
		-webkit-text-stroke: 0.2px currentColor;
		line-height: 1;
		color: var(--color-foreground);
		background: none;
		border: none;
		padding: 0;
		cursor: pointer;
		flex-shrink: 0;
	}

	.mark-word:hover {
		opacity: 0.65;
	}

	.mark-word:focus-visible {
		outline: 2px solid var(--color-border-focus);
		outline-offset: 2px;
		border-radius: 2px;
	}

	.path-sep {
		font-family: var(--font-serif, serif);
		font-size: 15px;
		color: var(--color-foreground-subtle);
		flex-shrink: 0;
	}

	/* Identical to the root in every respect, ink included: it is one path,
	   set once. The root is distinguished by being hoverable, not by being a
	   different colour — a breadcrumb whose halves are styled differently
	   reads as a title with a caption. */
	.path-tail {
		font-family: var(--font-serif, serif);
		font-size: 15px;
		font-weight: 400;
		letter-spacing: 0.025em;
		-webkit-text-stroke: 0.2px currentColor;
		color: var(--color-foreground);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		min-width: 0;
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

	/* A destination-shaped row: same height, radius, gutter and text edge as
	   every Library row, so it belongs to the column rather than hovering
	   above it. Quieter than a destination, because it is a utility. */
	.search-row {
		display: flex;
		align-items: center;
		gap: var(--sidebar-interactive-gap);
		width: 100%;
		height: var(--sidebar-interactive-height);
		padding: 0 10px 0 var(--sidebar-padding-left-base);
		margin-top: 2px;
		border: none;
		border-radius: var(--sidebar-interactive-radius);
		background: transparent;
		color: var(--color-foreground-muted);
		font-size: var(--sidebar-interactive-font-size);
		text-align: left;
		cursor: pointer;
		transition: background 150ms var(--ease-premium), color 150ms var(--ease-premium);
	}

	.search-row :global(.atlas-icon) {
		opacity: var(--sidebar-icon-opacity);
	}

	.search-row:hover {
		background: var(--sidebar-hover-bg);
		color: var(--color-foreground);
	}

	.search-row:hover :global(.atlas-icon) {
		opacity: 1;
	}

	.search-row:focus-visible {
		outline: 2px solid var(--color-border-focus);
		outline-offset: -2px;
	}

	.search-label {
		flex: 1;
	}

	/* The chord, in the console's voice — mono, tabular, the quietest ink in
	   the panel. It teaches the shortcut without competing with the label. */
	.search-hint {
		font-family: var(--font-mono);
		font-size: 10px;
		letter-spacing: 0.04em;
		color: var(--color-foreground-disabled);
		flex-shrink: 0;
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
