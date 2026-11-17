<script lang="ts">
	/**
	 * The search palette (⌘K): a card near the top of the pages, its field
	 * on top and the results under it (SearchList). It is its own object, so
	 * it opens at once and at one width, wherever it was opened from. The
	 * page bar's magnifier, the sidebar and the phone's menu all open it.
	 *
	 * On the desktop it stands centred over the pages, right of the
	 * sidebar, and the window behind it blurs and fades into a fine dot
	 * screen, clear around the card and denser toward the window's edges.
	 */
	import Icon from "$lib/components/Icon.svelte";
	import SearchList from "$lib/components/search/SearchList.svelte";
	import { fade } from "svelte/transition";
	import { search } from "$lib/stores/search.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { sidebarState } from "$lib/stores/sidebarState.svelte";

	interface Props {
		open?: boolean;
		onClose: () => void;
	}

	let { open = false, onClose }: Props = $props();

	let query = $state("");
	let mode = $state<"search" | "theme">("search");
	let list = $state<SearchList | null>(null);
	/** The route of the tab search was opened from, for the empty state. */
	let here = $state<string | null>(null);

	$effect(() => {
		if (!open) return;
		query = search.startQuery;
		mode = "search";
		here = windowShellStore.activeTab?.route ?? null;
	});

	// The cursor goes after a scope the field opened with (`#pages `).
	function focus(node: HTMLInputElement) {
		node.focus();
		node.setSelectionRange(node.value.length, node.value.length);
	}

	/** Comes up in one short step and goes in a shorter one: no travel. */
	function appear(_node: HTMLElement) {
		const still = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
		return {
			duration: 100,
			css: (t: number) => (still ? `opacity: ${t}` : `opacity: ${t}; transform: scale(${0.98 + 0.02 * t})`),
		};
	}
</script>

{#if open}
	<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
	<div
		class="backdrop"
		style:--pages-left="{sidebarState.cardLeft}px"
		onclick={(e) => e.target === e.currentTarget && onClose()}
		out:fade={{ duration: 120 }}
	>
		<div class="card" role="dialog" aria-modal="true" aria-label="Search" data-search in:appear>
			<div class="field">
				<Icon icon="atlas:search" width="16" class="field-icon" />
				<input
					bind:value={query}
					onkeydown={(e) => list?.keydown(e)}
					placeholder={mode === "theme" ? "Filter themes" : "Search, ask, or type an address"}
					aria-label="Search"
					use:focus
				/>
				<kbd class="esc">esc</kbd>
				<button class="close" onclick={onClose} aria-label="Close search">
					<Icon icon="atlas:close" width="16" />
				</button>
			</div>
			<div class="results">
				<SearchList bind:this={list} bind:query bind:mode {here} {onClose} />
			</div>
		</div>
	</div>
{/if}

<style>
	/* On the desktop the card is centred over the pages, from the pane's
	   left edge (--pages-left, the sidebar's width and gap) to the window's
	   air on the right. The window behind it blurs a little and takes a
	   wash of the background, so the card is the one sharp thing. */
	.backdrop {
		position: fixed;
		inset: 0;
		display: flex;
		align-items: flex-start;
		justify-content: center;
		padding: calc(var(--chrome-row-h) + 9vh) var(--window-air) 16px var(--pages-left, 16px);
		z-index: var(--z-modal);
		background: color-mix(in srgb, var(--color-background) 28%, transparent);
		backdrop-filter: blur(10px) saturate(0.9);
		-webkit-backdrop-filter: blur(10px) saturate(0.9);
		animation: veil-in 220ms var(--ease-premium);
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
		mask-image: radial-gradient(
			ellipse 70% 75% at calc(var(--pages-left, 0px) / 2 + 50%) 30%,
			transparent 25%,
			var(--color-foreground) 100%
		);
		animation: dots-in 320ms var(--ease-premium);
	}

	@keyframes veil-in {
		from {
			background: transparent;
			backdrop-filter: blur(0) saturate(1);
			-webkit-backdrop-filter: blur(0) saturate(1);
		}
	}

	@keyframes dots-in {
		from {
			opacity: 0;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.backdrop,
		.backdrop::before {
			animation: none;
		}
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
			animation: none;
		}

		.backdrop::before {
			display: none;
		}
	}

	/* A column, so the results are the part that gives when space runs out.
	   Its height follows the results without animating, so a row never
	   slides under the selection. */
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
		box-shadow: var(--shadow-float);
		overflow: hidden;
	}

	.field {
		display: flex;
		flex: none;
		align-items: center;
		gap: 10px;
		height: 48px;
		padding: 0 14px 0 15px;
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

	/* 16px keeps iOS from zooming the page when the field takes focus. */
	@media (hover: none) {
		input {
			font-size: 16px;
		}
		.esc {
			display: none;
		}
		.close {
			display: flex;
		}
	}

	input::placeholder {
		color: var(--color-foreground-subtle);
	}

	.esc {
		flex: none;
		padding: 1px 5px;
		border-radius: var(--radius-sm);
		background: var(--hover-bg);
		font-family: inherit;
		font-size: 11px;
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

	.results {
		min-height: 0;
		overflow-y: auto;
	}
</style>
