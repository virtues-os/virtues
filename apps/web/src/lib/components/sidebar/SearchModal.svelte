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
	 * (FocusVeil) covers the sidebar as well as the pages, so the window is
	 * what the eye measures against, and it opens like a camera pulling
	 * focus. The create dialogs open the same way.
	 */
	import Icon from "$lib/components/Icon.svelte";
	import FocusVeil, { cardIn, cardOut } from "$lib/components/FocusVeil.svelte";
	import SearchList from "$lib/components/search/SearchList.svelte";
	import { search } from "$lib/stores/search.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import type { SearchRow } from "$lib/search/results";

	interface Props {
		open?: boolean;
		onClose: () => void;
	}

	let { open = false, onClose }: Props = $props();

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
</script>

<FocusVeil {open} onBackdrop={onClose} sinking={() => search.sinking}>
		<div class="card" class:has-preview={mode === "search" && !!selected} role="dialog" aria-modal="true" aria-label="Search" data-search in:cardIn|global out:cardOut|global={{ sinking: search.sinking }}>
			<div class="field">
				<Icon icon="atlas:search" width="16" class="field-icon" />
				<input
					bind:value={query}
					onkeydown={(e) => list?.keydown(e)}
					placeholder={mode === "theme" ? "Filter themes" : "Search, ask, or type an address"}
					aria-label="Search"
					autocomplete="off"
					autocorrect="off"
					autocapitalize="off"
					spellcheck="false"
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
					<!-- Loaded with the first search, so it is not part of the launch. -->
					{#await import("$lib/components/search/SearchPreview.svelte") then { default: SearchPreview }}
						<SearchPreview row={selected} {query} />
					{/await}
				</div>
			</div>
			<div class="footer" aria-hidden="true">
				<span><kbd>↵</kbd>{enterLabel}</span>
				<span><kbd>↑</kbd><kbd>↓</kbd>Move</span>
				<span class="end"><kbd>esc</kbd>{mode === "theme" ? "Back" : "Close"}</span>
			</div>
		</div>
</FocusVeil>

<style>
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
		background: var(--fill-bg);
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
