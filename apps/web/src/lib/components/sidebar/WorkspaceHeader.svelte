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
	// So: one row. The mark on the left, inert — no hover, no cursor, no tab
	// stop — and search as a small icon button flexed to the right of it. The
	// mark is the one place the serif appears in the chrome, which concentrates
	// the typographic identity in a single deliberate spot instead of spreading
	// it thin.
	//
	// `virtues` is set at the SAME size as the nav labels below. A wordmark that
	// is bigger than everything around it is a logo demanding attention, and
	// this one has no job to do beyond saying whose desk this is. The serif and
	// the ∴ carry the identity; scale would just make it loud.
	const hint = $derived(isAppleKeyboard ? "⌘K" : "Ctrl K");
</script>

<div class="masthead" class:collapsed>
	<div
		class="masthead-row animate-row"
		style="animation-delay: {animationDelay}ms"
	>
		<!-- Inert. aria-hidden because "∴ virtues" read aloud between the window
		     title and the first destination is noise, not information. -->
		<div class="mark" aria-hidden="true">
			<span class="mark-glyph">∴</span><span class="mark-word">virtues</span>
		</div>

		<button
			type="button"
			class="search-btn"
			onclick={() => onSearch?.()}
			aria-label="Search"
			title="Search ({hint})"
		>
			<Icon icon="ri:search-line" width="16" />
		</button>
	</div>
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

	.mark {
		display: flex;
		align-items: baseline;
		gap: 6px;
		user-select: none;
		color: var(--color-foreground);
	}

	.mark-glyph {
		font-family: var(--font-serif, serif);
		font-size: 15px;
		line-height: 1;
	}

	/* Same size as the nav labels. See the note in the script block. */
	.mark-word {
		font-family: var(--font-serif, serif);
		font-size: var(--sidebar-interactive-font-size);
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

	.search-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 24px;
		height: 24px;
		flex-shrink: 0;
		border: none;
		border-radius: var(--sidebar-interactive-radius);
		background: transparent;
		color: var(--color-foreground-muted);
		opacity: var(--sidebar-icon-opacity);
		cursor: pointer;
		transition: background 150ms ease, opacity 150ms ease;
	}

	.search-btn:hover {
		background: var(--sidebar-hover-bg);
		opacity: 1;
	}

	.search-btn:focus-visible {
		opacity: 1;
		outline: 2px solid var(--color-border-focus);
		outline-offset: 1px;
	}

	@media (prefers-reduced-motion: reduce) {
		.animate-row {
			animation: none;
		}
		.search-btn {
			transition: none;
		}
	}
</style>
