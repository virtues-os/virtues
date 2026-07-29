<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { isAppleKeyboard } from "$lib/utils/platform";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { HOME_ROUTE } from "$lib/sidebar/sections";
	import { routeCloth } from "$lib/sidebar/pin-colors";

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
	// notebooks), with the thing's dot riding beside its name — the same dot
	// as on its Desk spine and nowhere else. Routes without a cloth get no
	// tail: a path that narrated every room would be a status bar.
	const hint = $derived(isAppleKeyboard ? "⌘K" : "Ctrl K");

	const activeTab = $derived.by(() => {
		const pane = windowShellStore.activePane;
		if (!pane) return null;
		return pane.tabs.find((t) => t.id === pane.activeTabId) ?? null;
	});

	const tailCloth = $derived(routeCloth(activeTab?.route));
	const tailLabel = $derived(tailCloth && activeTab ? activeTab.label : null);

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
				<span class="path-dot" style="background: {tailCloth}" aria-hidden="true"></span>
				<span class="path-tail">{tailLabel}</span>
			{/if}
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

	.path {
		display: flex;
		align-items: center;
		gap: 6px;
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
		font-size: 13px;
		color: var(--color-foreground-subtle);
		flex-shrink: 0;
	}

	/* The thing's dot, riding beside its name — same cloth as its Desk spine. */
	.path-dot {
		width: 6px;
		height: 6px;
		border-radius: 999px;
		flex-shrink: 0;
		box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.12);
	}

	.path-tail {
		font-family: var(--font-serif, serif);
		font-size: 13px;
		font-weight: 400;
		color: var(--color-foreground-muted);
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
