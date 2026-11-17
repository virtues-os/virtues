<script lang="ts">
	/**
	 * The search palette (⌘K): a card near the top of the window, its field
	 * on top, the results under it (SearchList) and, on a desktop wide enough,
	 * a preview of the selected row beside them (SearchPreview). It is its
	 * own object, so it opens at once and at one width, wherever it was
	 * opened from. The page bar's magnifier, the sidebar and the phone's menu
	 * all open it.
	 *
	 * On the desktop it stands centred on the window. The veil behind it
	 * covers the sidebar as well as the pages, so the window is what the eye
	 * measures against. The window blurs and fades into a fine dot screen,
	 * clear around the card and denser toward the edges.
	 *
	 * The card and the veil come up on one clock and go on a shorter one.
	 * The veil fades by opacity over a blur that never animates, since
	 * animating `backdrop-filter` itself repaints the whole window per frame.
	 */
	import Icon from "$lib/components/Icon.svelte";
	import { reducedMotion } from "$lib/stores/motion.svelte";
	import SearchList from "$lib/components/search/SearchList.svelte";
	import SearchPreview from "$lib/components/search/SearchPreview.svelte";
	import { fade } from "svelte/transition";
	import { cubicIn, expoOut } from "svelte/easing";
	import { search } from "$lib/stores/search.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import type { SearchRow } from "$lib/search/results";

	interface Props {
		open?: boolean;
		onClose: () => void;
	}

	let { open = false, onClose }: Props = $props();

	const IN_MS = 160;
	const OUT_MS = 90;

	let query = $state("");
	let mode = $state<"search" | "theme">("search");
	let list = $state<SearchList | null>(null);
	let selected = $state<SearchRow | null>(null);
	/** The route of the tab search was opened from, for the empty state. */
	let here = $state<string | null>(null);

	$effect(() => {
		if (!open) return;
		query = search.startQuery;
		mode = "search";
		here = windowShellStore.activeTab?.route ?? null;
	});

	/** What Return does to the selected row, for the footer. */
	const enterLabel = $derived(
		mode === "theme"
			? "Choose"
			: selected?.kind === "ask"
				? "Ask"
				: selected?.kind === "command"
					? "Run"
					: "Open",
	);

	// The cursor goes after a scope the field opened with (`#pages `).
	function focus(node: HTMLInputElement) {
		node.focus();
		node.setSelectionRange(node.value.length, node.value.length);
	}

	const still = () => reducedMotion();

	/** Rises a few pixels into place as it grows to full size. */
	function appear(_node: HTMLElement) {
		return {
			duration: IN_MS,
			easing: expoOut,
			css: (t: number) =>
				still() ? `opacity: ${t}` : `opacity: ${t}; transform: translateY(${(1 - t) * 6}px) scale(${0.97 + 0.03 * t})`,
		};
	}

	/** Shrinks a little as it fades, faster than it came. */
	function leave(_node: HTMLElement) {
		return {
			duration: OUT_MS,
			easing: cubicIn,
			css: (t: number) => (still() ? `opacity: ${t}` : `opacity: ${t}; transform: scale(${0.98 + 0.02 * t})`),
		};
	}
</script>

{#if open}
	<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
	<div
		class="backdrop"
		onclick={(e) => e.target === e.currentTarget && onClose()}
		in:fade={{ duration: IN_MS, easing: expoOut }}
		out:fade={{ duration: OUT_MS, easing: cubicIn }}
	>
		<div class="card" class:has-preview={mode === "search" && !!selected} role="dialog" aria-modal="true" aria-label="Search" data-search in:appear out:leave>
			<div class="field">
				<Icon icon="atlas:search" width="16" class="field-icon" />
				<input
					bind:value={query}
					onkeydown={(e) => list?.keydown(e)}
					placeholder={mode === "theme" ? "Filter themes" : "Search, ask, or type an address"}
					aria-label="Search"
					use:focus
				/>
				<button class="close" onclick={onClose} aria-label="Close search">
					<Icon icon="atlas:close" width="16" />
				</button>
			</div>
			<div class="body">
				<div class="results">
					<SearchList bind:this={list} bind:query bind:mode bind:selected {here} {onClose} />
				</div>
				<div class="side" aria-live="polite">
					<SearchPreview row={selected} {query} />
				</div>
			</div>
			<div class="footer" aria-hidden="true">
				<span><kbd>↵</kbd>{enterLabel}</span>
				{#if mode === "search"}<span><kbd>Tab</kbd>Ask</span>{/if}
				<span><kbd>↑</kbd><kbd>↓</kbd>Move</span>
				<span class="end"><kbd>esc</kbd>{mode === "theme" ? "Back" : "Close"}</span>
			</div>
		</div>
	</div>
{/if}

<style>
	/* Centred on the window, with the window's air on both sides. */
	.backdrop {
		position: fixed;
		inset: 0;
		display: flex;
		align-items: flex-start;
		justify-content: center;
		padding: calc(var(--chrome-row-h) + 8vh) var(--window-air) 16px;
		z-index: var(--z-modal);
		background: color-mix(in srgb, var(--color-background) 28%, transparent);
		backdrop-filter: blur(10px) saturate(0.9);
		-webkit-backdrop-filter: blur(10px) saturate(0.9);
	}

	/* The dot screen: dots of ink 4px apart, masked so the card's
	   surroundings stay clear and the screen thickens toward the edges. */
	.backdrop::before {
		content: "";
		position: absolute;
		inset: 0;
		pointer-events: none;
		background-image: radial-gradient(
			color-mix(in srgb, var(--color-foreground) 22%, transparent) 0.75px,
			transparent 1.25px
		);
		background-size: 4px 4px;
		mask-image: radial-gradient(ellipse 70% 75% at 50% 30%, transparent 25%, var(--color-foreground) 100%);
	}

	@media (prefers-reduced-transparency: reduce) {
		.backdrop {
			backdrop-filter: none;
			-webkit-backdrop-filter: none;
			background: color-mix(in srgb, var(--color-background) 70%, transparent);
		}
	}

	@media (hover: none) {
		.backdrop {
			/* Clears the notch on top, and at the bottom the home indicator and
			   the keyboard this raises the moment it opens. `--keyboard-inset`
			   is 0 anywhere there isn't one. */
			padding: max(6vh, env(safe-area-inset-top)) 16px
				max(16px, env(safe-area-inset-bottom), var(--keyboard-inset, 0px));
			background: rgba(0, 0, 0, 0.4);
			backdrop-filter: none;
			-webkit-backdrop-filter: none;
		}

		.backdrop::before {
			display: none;
		}
	}

	/* A column, so the body is the part that gives when space runs out. */
	.card {
		/* Over the dot screen, which is positioned. */
		position: relative;
		display: flex;
		flex-direction: column;
		width: 100%;
		max-width: 600px;
		max-height: min(560px, 100%);
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-xl);
		box-shadow:
			var(--shadow-float),
			0 24px 64px color-mix(in srgb, var(--color-foreground) 12%, transparent);
		overflow: hidden;
		transform-origin: 50% 20%;
	}

	.field {
		display: flex;
		flex: none;
		align-items: center;
		gap: 10px;
		height: 52px;
		padding: 0 14px 0 16px;
	}

	:global(.field-icon) {
		flex: none;
		color: var(--color-foreground-muted);
	}

	input {
		flex: 1;
		min-width: 0;
		border: none;
		background: transparent;
		font-size: 15px;
		color: var(--color-foreground);
		outline: none;
	}

	input::placeholder {
		color: var(--color-foreground-subtle);
	}

	/* The phone has no Escape key, so it gets a button. */
	.close {
		display: none;
		flex: none;
		align-items: center;
		justify-content: center;
		width: 32px;
		height: 32px;
		margin-right: -8px;
		border: none;
		border-radius: var(--radius-md);
		background: transparent;
		color: var(--color-foreground-muted);
	}

	.body {
		display: flex;
		min-height: 0;
	}

	.results {
		flex: 1;
		min-width: 0;
		min-height: 0;
		overflow-y: auto;
	}

	.side {
		display: none;
	}

	.footer {
		display: none;
	}

	/* A desktop wide enough for two columns: the list on the left, the
	   preview on the right, at one height whatever the row, so moving
	   down the list never moves the card. */
	@media (hover: hover) and (min-width: 960px) {
		.card {
			max-width: 880px;
		}

		.card.has-preview .body {
			flex: 0 1 404px;
			border-top: 1px solid var(--color-border);
		}

		/* The list draws its own top rule; with the body ruled, it drops it. */
		.card.has-preview .results :global(.list) {
			border-top: none;
		}

		.card.has-preview .results {
			flex: none;
			width: 380px;
			border-right: 1px solid var(--color-border);
		}

		.card.has-preview .side {
			display: block;
			flex: 1;
			min-width: 0;
			overflow-y: auto;
		}

		.footer {
			display: flex;
			flex: none;
			align-items: center;
			gap: 16px;
			height: 36px;
			padding: 0 16px;
			border-top: 1px solid var(--color-border);
			font-size: 12px;
			color: var(--color-foreground-subtle);
			user-select: none;
		}

		.footer span {
			display: inline-flex;
			align-items: center;
			gap: 6px;
		}

		.footer .end {
			margin-left: auto;
		}

		/* An empty field with nothing under it needs no key hints. */
		.card:not(.has-preview) .footer {
			display: none;
		}
	}

	.footer kbd {
		display: inline-flex;
		align-items: center;
		height: 18px;
		padding: 0 5px;
		border-radius: var(--radius-sm);
		background: var(--hover-bg);
		font-family: inherit;
		font-size: 11px;
		line-height: 1;
		color: var(--color-foreground-subtle);
	}

	/* 16px keeps iOS from zooming the page when the field takes focus. */
	@media (hover: none) {
		input {
			font-size: 16px;
		}
		.close {
			display: flex;
		}
	}
</style>
