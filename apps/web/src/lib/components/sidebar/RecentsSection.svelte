<script lang="ts">
	/**
	 * Recents — where you've been, newest first.
	 *
	 * Deliberately built from the same markup as `SystemSection`: the same
	 * `sidebar-interactive` row, the same folder-toggle that swaps icon for
	 * chevron on hover, the same child rows. Recents is a destination like Chats
	 * or Pages, and giving it its own header treatment made it read as a
	 * different kind of thing sitting in the same list.
	 *
	 * The filter is a floating context menu, not an inline panel. Opening it used
	 * to expand in flow and shove every row below it down the sidebar, which is
	 * the one thing a menu should never do to the list it belongs to.
	 */
	import Icon from '$lib/components/Icon.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { historyStore, HISTORY_KINDS } from '$lib/stores/history.svelte';
	import { contextMenu, type ContextMenuItem } from '$lib/stores/contextMenu.svelte';
	import { onMount } from 'svelte';

	interface Props {
		collapsed?: boolean;
		animationDelay?: number;
	}

	let { collapsed = false, animationDelay = 0 }: Props = $props();

	const EXPANDED_KEY = 'virtues-recents-open';

	let isExpanded = $state(
		typeof localStorage !== 'undefined'
			? localStorage.getItem(EXPANDED_KEY) === 'true'
			: false,
	);

	const entries = $derived(historyStore.entries);
	const filterActive = $derived(historyStore.kinds.length > 0);

	onMount(() => {
		if (isExpanded) void historyStore.load();
	});

	function toggle() {
		isExpanded = !isExpanded;
		localStorage.setItem(EXPANDED_KEY, String(isExpanded));
		if (isExpanded) void historyStore.load();
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter' || e.key === ' ') {
			e.preventDefault();
			toggle();
		}
	}

	function open(url: string, label: string | null) {
		windowShellStore.openTabFromRoute(url, { label: label ?? undefined });
	}

	/** The `···` menu — floats over the sidebar rather than displacing it. */
	function showFilterMenu(e: MouseEvent) {
		e.stopPropagation();

		const items: ContextMenuItem[] = HISTORY_KINDS.map((k) => ({
			id: k.id,
			label: k.label,
			checked: historyStore.kinds.includes(k.id),
			action: () => historyStore.toggleKind(k.id),
		}));

		if (filterActive) {
			items.push({
				id: 'all',
				label: 'Show everything',
				icon: 'ri:filter-off-line',
				dividerBefore: true,
				action: () => historyStore.clearFilter(),
			});
		}

		items.push({
			id: 'clear',
			label: 'Clear history',
			icon: 'ri:delete-bin-line',
			variant: 'destructive',
			dividerBefore: true,
			action: () => historyStore.clearAll(),
		});

		contextMenu.show({ x: e.clientX, y: e.clientY }, items);
	}

	function forget(e: MouseEvent, url: string) {
		e.stopPropagation();
		void historyStore.forget(url);
	}
</script>

{#if !collapsed}
	<div class="system-section" style="--stagger-delay: {animationDelay}ms">
		<div
			class="sidebar-interactive system"
			role="button"
			tabindex="0"
			onclick={toggle}
			onkeydown={handleKeydown}
			oncontextmenu={(e) => {
				e.preventDefault();
				showFilterMenu(e);
			}}
		>
			<span class="folder-toggle" class:expanded={isExpanded}>
				<span class="folder-toggle-icon">
					<Icon icon="ri:history-line" width="16" class="sidebar-icon" />
				</span>
				<svg class="folder-toggle-chevron" width="12" height="12" viewBox="0 0 16 16" fill="none">
					<path
						d="M6 4L10 8L6 12"
						stroke="currentColor"
						stroke-width="1.5"
						stroke-linecap="round"
						stroke-linejoin="round"
					/>
				</svg>
			</span>

			<span class="sidebar-label">Recents</span>

			<span class="sidebar-item-actions">
				<button
					class="sidebar-item-action"
					class:pinned={filterActive}
					title={filterActive ? 'Filtered — change' : 'Filter recents'}
					aria-label="Filter recents"
					onclick={showFilterMenu}
				>
					<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
						<circle cx="4" cy="8" r="1.25" />
						<circle cx="8" cy="8" r="1.25" />
						<circle cx="12" cy="8" r="1.25" />
					</svg>
				</button>
			</span>
		</div>

		{#if isExpanded}
			{#if historyStore.loading && entries.length === 0}
				<div class="recents-note">Loading…</div>
			{:else if entries.length === 0}
				<div class="recents-note">
					{filterActive ? 'Nothing matches that filter' : 'Nowhere yet'}
				</div>
			{:else}
				{#each entries as entry (entry.url)}
					<div
						class="sidebar-interactive child"
						role="button"
						tabindex="0"
						title={entry.label ?? entry.url}
						onclick={() => open(entry.url, entry.label)}
						onkeydown={(e) => {
							if (e.key === 'Enter') open(entry.url, entry.label);
						}}
					>
						<Icon icon={entry.icon ?? 'ri:file-line'} width="14" class="sidebar-icon" />
						<span class="sidebar-label">{entry.label ?? entry.url}</span>
						<span class="sidebar-item-actions">
							<button
								class="sidebar-item-action"
								title="Forget this"
								aria-label="Forget this"
								onclick={(e) => forget(e, entry.url)}
							>
								<svg width="14" height="14" viewBox="0 0 16 16" fill="none">
									<path
										d="M4 4l8 8M12 4l-8 8"
										stroke="currentColor"
										stroke-width="1.5"
										stroke-linecap="round"
									/>
								</svg>
							</button>
						</span>
					</div>
				{/each}
			{/if}
		{/if}
	</div>
{/if}

<style>
	@reference "../../../app.css";
	@reference "$lib/styles/sidebar.css";

	.system-section {
		display: flex;
		flex-direction: column;
	}

	/* Svelte scopes styles per component, so wearing SystemSection's classes
	   doesn't inherit its rules — the chevron rendered unstyled below the row
	   until these came across. Icon at rest, chevron on hover or when open. */
	.folder-toggle {
		position: relative;
		width: 16px;
		height: 16px;
		flex-shrink: 0;
		overflow: hidden;
		cursor: pointer;
	}

	.folder-toggle-icon,
	.folder-toggle-chevron {
		position: absolute;
		inset: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		transition:
			opacity 120ms ease,
			transform 160ms ease;
	}

	.folder-toggle-icon {
		opacity: 1;
		transform: translateY(0);
	}

	.folder-toggle-chevron {
		opacity: 0;
		transform: translateY(6px);
		color: var(--color-foreground-subtle);
		margin: auto;
	}

	.sidebar-interactive:hover .folder-toggle-icon {
		opacity: 0;
		transform: translateY(-6px);
	}

	.sidebar-interactive:hover .folder-toggle-chevron {
		opacity: 1;
		transform: translateY(0);
	}

	.folder-toggle.expanded .folder-toggle-icon {
		opacity: 0;
		transform: translateY(-6px);
	}

	.folder-toggle.expanded .folder-toggle-chevron {
		opacity: 1;
		transform: translateY(0) rotate(90deg);
	}

	/* Child rows indent to the same column as every other nested sidebar item. */
	.child {
		padding-left: calc(var(--sidebar-padding-left-base) + var(--sidebar-indent-width));
	}

	.recents-note {
		font-size: 12px;
		color: var(--color-foreground-subtle);
		padding: 4px 0 4px
			calc(var(--sidebar-padding-left-base) + var(--sidebar-indent-width));
	}

	/* Marks the filter as engaged, so a short list reads as filtered rather
	   than as an empty history. */
	.sidebar-item-action.pinned {
		opacity: 1;
		color: var(--color-foreground);
	}
</style>
