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

	// One control, not three.
	//
	// This row used to be a ∴ mark that went Home, plus two unlabelled 15px
	// glyphs. Three problems in one row: a wordmark isn't a button anywhere else
	// in software, so the mark read as a label that happened to be clickable; a
	// logo in the sidebar of a single-owner appliance is pure chrome, since you
	// know perfectly well which app you're in; and the two glyphs had no
	// containers, so they read as decoration rather than targets.
	//
	// The mark's justification was "it gives the ∴ a job" — which only holds if
	// the mark needs one. It doesn't, so Home goes back to being a labelled nav
	// row where it can be read, and the mark leaves.
	//
	// What's left is the row's actual highest-traffic job: search. Shaped like
	// the input it stands in for, because that is what makes it legible without
	// a label explaining it. New chat moves to the + on the Chats row, with
	// every other collection.
	const hint = $derived(isAppleKeyboard ? "⌘K" : "Ctrl K");
</script>

<div class="masthead" class:collapsed>
	<button
		type="button"
		class="search-row animate-row"
		style="animation-delay: {animationDelay}ms; --stagger-delay: {animationDelay}ms"
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
	.masthead {
		padding: 0 8px;
		margin-top: var(--pane-inset);
		display: flex;
		align-items: center;
		height: var(--chrome-row-h);
		box-sizing: border-box;
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

	/* Input-shaped, but a button: it opens the palette rather than accepting
	   typing in place. Borrowing the input's shape is what makes that legible
	   without a caption — and the palette is where the real field lives, so a
	   second one here would be two places to type the same query. */
	.search-row {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		height: 28px;
		padding: 0 8px;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: var(--color-background);
		color: var(--color-foreground-subtle);
		font: inherit;
		font-size: 13px;
		text-align: left;
		cursor: pointer;
		transition:
			background 150ms ease,
			border-color 150ms ease,
			color 150ms ease;
	}

	.search-row:hover {
		background: var(--hover-bg);
		border-color: var(--color-border-strong);
		color: var(--color-foreground);
	}

	.search-row:focus-visible {
		outline: 2px solid var(--color-border-focus);
		outline-offset: 1px;
	}

	.search-label {
		flex: 1;
		min-width: 0;
	}

	/* The hint lives here rather than in a tooltip. On the old row it was the
	   only keyboard hint in the sidebar and read as chrome; on a control shaped
	   like a search field it reads as the field's shortcut, which is the one
	   place people expect to find one. */
	kbd {
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		line-height: 1;
		padding: 3px 4px;
		border-radius: 3px;
		background: var(--hover-bg);
		color: var(--color-foreground-subtle);
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
