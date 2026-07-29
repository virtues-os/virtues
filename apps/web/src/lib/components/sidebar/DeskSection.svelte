<script lang="ts">
	/**
	 * The Desk — what you've taken off the shelf to work on.
	 *
	 * Notebook spines set in the serif with a bookcloth dot each: the names of
	 * the user's things deserve a bookface, and the type distinction (serif
	 * here, sans on the Library shelf below) encodes OWNERSHIP, not
	 * decoration. No icons — a made thing has no natural glyph, which is what
	 * the dot is for. No selection state — the tabs and the path mast own
	 * "where am I"; the rail is the deterministic launcher.
	 *
	 * Uncapped and in the user's own sort_order: a curated shelf that
	 * reorders itself is how a stable list stops being a place.
	 */
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { listNotebooks, createNotebook } from "$lib/api/client";
	import { pinColor } from "$lib/sidebar/pin-colors";

	interface Props {
		collapsed?: boolean;
		animationDelay?: number;
	}

	let { collapsed = false, animationDelay = 0 }: Props = $props();

	interface DeskItem {
		id: string;
		name: string;
		route: string;
	}

	let items = $state<DeskItem[]>([]);
	let loading = $state(false);
	let loaded = $state(false);
	let lastCacheVersion = $state(-1);

	async function fetchDesk() {
		if (loading) return;
		loading = true;
		try {
			const data = await listNotebooks();
			// `/notebook/{id}`, singular — the registry's detail pattern. The old
			// sidebar linked `/notebooks/{id}`, which matches no pattern at all
			// and fell through parseRoute's chain to the chat fallback, so
			// opening a notebook from the rail produced a chat tab.
			items = (data.notebooks || []).map((n) => ({
				id: n.id,
				name: n.name,
				route: `/notebook/${n.id}`,
			}));
			loaded = true;
		} catch (e) {
			console.error("[DeskSection] Failed to fetch notebooks:", e);
		} finally {
			loading = false;
		}
	}

	// Refetch when any surface invalidates the view cache (create/rename/delete),
	// same signal the smart sections listen to.
	$effect.pre(() => {
		const version = windowShellStore.viewCacheVersion;
		if (lastCacheVersion !== version) {
			lastCacheVersion = version;
			fetchDesk();
		}
	});

	function open(item: DeskItem) {
		windowShellStore.openTabFromRoute(item.route, {
			label: item.name,
			focusExisting: true,
		});
	}

	function handleKeydown(e: KeyboardEvent, item: DeskItem) {
		if (e.key === "Enter" || e.key === " ") {
			e.preventDefault();
			open(item);
		}
	}

	async function handleNew(e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();
		try {
			const notebook = await createNotebook({ name: "Untitled notebook" });
			windowShellStore.openTabFromRoute(`/notebook/${notebook.id}`, {
				label: notebook.name,
				forceNew: true,
				preferEmptyPane: true,
			});
		} catch (err) {
			console.error("[DeskSection] Failed to create notebook:", err);
		}
	}
</script>

{#if !collapsed}
	<div class="desk" style="--stagger-delay: {animationDelay}ms">
		<div class="desk-header">
			<span class="desk-title">Desk</span>
			<button class="sidebar-item-action desk-add" title="New notebook" onclick={handleNew}>
				<svg width="14" height="14" viewBox="0 0 16 16" fill="none">
					<path d="M8 3.5v9M3.5 8h9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
				</svg>
			</button>
		</div>

		{#if loaded && items.length === 0}
			<div class="desk-empty">Nothing checked out.</div>
		{:else}
			{#each items as item, i (item.id)}
				<div
					class="sidebar-interactive desk-spine"
					role="link"
					tabindex="0"
					style="animation-delay: calc(var(--stagger-delay) + {i * 30}ms)"
					onclick={() => open(item)}
					onkeydown={(e) => handleKeydown(e, item)}
					title={item.name}
				>
					<span class="desk-pin" aria-hidden="true">
						<i style="background: {pinColor(item.id)}"></i>
					</span>
					<span class="sidebar-label desk-spine-label">{item.name}</span>
				</div>
			{/each}
		{/if}
	</div>
{/if}

<style>
	@reference "../../../app.css";
	@reference "$lib/styles/sidebar.css";

	.desk {
		display: flex;
		flex-direction: column;
	}

	.desk-header {
		display: flex;
		align-items: center;
		height: 26px;
		padding: 0 8px 0 var(--sidebar-padding-left-base);
		margin-bottom: 2px;
		user-select: none;
		animation: deskRowIn 200ms cubic-bezier(0.2, 0, 0, 1) backwards;
		animation-delay: var(--stagger-delay, 0ms);
	}

	/* The zone subtitle: a whisper, not a headline. Sans, small, quiet —
	   Capitalized word, no smallcaps apparatus. */
	.desk-title {
		font-size: 11px;
		font-weight: 500;
		letter-spacing: 0.015em;
		color: var(--color-foreground-subtle);
	}

	.desk-add {
		margin-left: auto;
		opacity: 0;
		transition: opacity 150ms ease;
	}

	.desk-header:hover .desk-add {
		opacity: 1;
	}

	/* Spines: the serif appears in the chrome exactly where ownership does.
	   Display cut at text size wants its tracking back and a hair of optical
	   weight — the stroke reads as a medium, not a faux bold. */
	.desk-spine {
		font-family: var(--font-serif);
		font-size: 13.5px;
		font-weight: 400;
		letter-spacing: 0.02em;
		-webkit-text-stroke: 0.2px currentColor;
		animation: deskRowIn 200ms cubic-bezier(0.2, 0, 0, 1) backwards;
	}

	.desk-pin {
		width: 16px;
		height: 16px;
		flex-shrink: 0;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.desk-pin i {
		width: 6.5px;
		height: 6.5px;
		border-radius: 999px;
		display: block;
		box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.12);
	}

	.desk-spine-label {
		line-height: 1.5;
	}

	.desk-empty {
		padding: 4px 8px 4px var(--sidebar-padding-left-base);
		font-family: var(--font-serif);
		font-style: italic;
		font-size: 12.5px;
		color: var(--color-foreground-subtle);
	}

	@keyframes deskRowIn {
		from {
			opacity: 0;
			transform: translateX(-8px);
		}
		to {
			opacity: 1;
			transform: translateX(0);
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.desk-header,
		.desk-spine {
			animation: none;
		}
	}
</style>
