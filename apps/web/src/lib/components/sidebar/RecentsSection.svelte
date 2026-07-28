<script lang="ts">
	/**
	 * Recents — where you've been, newest first.
	 *
	 * Collapsible, and collapsed state persists: this sits at the foot of the
	 * sidebar and not everyone wants a list of their movements on screen all
	 * the time. The `···` menu filters by kind and can clear the log.
	 *
	 * Labels come from the visit row, which stores the title as it was. Good
	 * enough for a recents list, and the honest fallback when the target is
	 * gone — the alternative (resolving every row against live data) costs a
	 * fan-out of lookups to fix a stale word.
	 */
	import Icon from '$lib/components/Icon.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { historyStore, HISTORY_KINDS } from '$lib/stores/history.svelte';
	import { onMount } from 'svelte';

	interface Props {
		collapsed?: boolean;
	}

	let { collapsed = false }: Props = $props();

	const EXPANDED_KEY = 'virtues-recents-open';

	let expanded = $state(
		typeof localStorage !== 'undefined'
			? localStorage.getItem(EXPANDED_KEY) !== 'false'
			: true,
	);
	let menuOpen = $state(false);

	const entries = $derived(historyStore.entries);
	const filterActive = $derived(historyStore.kinds.length > 0);

	onMount(() => {
		if (expanded) void historyStore.load();
	});

	function toggleExpanded() {
		expanded = !expanded;
		localStorage.setItem(EXPANDED_KEY, String(expanded));
		// Nothing was fetched while closed, so opening has to fetch.
		if (expanded && entries.length === 0) void historyStore.load();
	}

	function open(url: string, label: string | null) {
		windowShellStore.openTabFromRoute(url, { label: label ?? undefined });
	}
</script>

{#if !collapsed}
	<div class="recents-section">
		<div class="section-header">
			<button type="button" class="header-toggle" onclick={toggleExpanded}>
				<Icon
					icon={expanded ? 'ri:arrow-down-s-line' : 'ri:arrow-right-s-line'}
					width="14"
				/>
				<span class="header-label">Recents</span>
			</button>

			<!-- Always rendered, not hover-revealed: a control that only exists on
			     hover is unreachable by keyboard and invisible on touch. It's just
			     quiet until you're near it. -->
			<button
				type="button"
				class="header-menu"
				class:active={menuOpen || filterActive}
				aria-label="Filter recents"
				aria-expanded={menuOpen}
				onclick={() => (menuOpen = !menuOpen)}
			>
				<Icon icon="ri:more-line" width="14" />
			</button>
		</div>

		{#if menuOpen}
			<div class="filter-menu">
				<span class="menu-label">Show</span>
				{#each HISTORY_KINDS as kind (kind.id)}
					<button
						type="button"
						class="menu-row"
						class:checked={historyStore.kinds.includes(kind.id)}
						onclick={() => historyStore.toggleKind(kind.id)}
					>
						<Icon
							icon={historyStore.kinds.includes(kind.id)
								? 'ri:checkbox-line'
								: 'ri:checkbox-blank-line'}
							width="13"
						/>
						<span>{kind.label}</span>
					</button>
				{/each}

				{#if filterActive}
					<button type="button" class="menu-row" onclick={() => historyStore.clearFilter()}>
						<Icon icon="ri:filter-off-line" width="13" />
						<span>Show everything</span>
					</button>
				{/if}

				<div class="menu-divider"></div>
				<button
					type="button"
					class="menu-row danger"
					onclick={() => {
						historyStore.clearAll();
						menuOpen = false;
					}}
				>
					<Icon icon="ri:delete-bin-line" width="13" />
					<span>Clear history</span>
				</button>
			</div>
		{/if}

		{#if expanded}
			{#if historyStore.loading && entries.length === 0}
				<div class="recents-empty">Loading…</div>
			{:else if entries.length === 0}
				<div class="recents-empty">
					{filterActive ? 'Nothing matches that filter' : 'Nowhere yet'}
				</div>
			{:else}
				<ul class="recents-list">
					{#each entries as entry (entry.url)}
						<li>
							<button
								type="button"
								class="recent-row"
								title={entry.label ?? entry.url}
								onclick={() => open(entry.url, entry.label)}
							>
								<Icon icon={entry.icon ?? 'ri:history-line'} width="14" />
								<span class="recent-label">{entry.label ?? entry.url}</span>
							</button>
						</li>
					{/each}
				</ul>
			{/if}
		{/if}
	</div>
{/if}

<style>
	.recents-section {
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
		padding: 0 0.375rem 0.375rem;
	}

	.section-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 4px;
		padding: 0 0.25rem;
	}

	.header-toggle {
		display: flex;
		align-items: center;
		gap: 2px;
		background: none;
		border: none;
		padding: 4px 2px;
		cursor: pointer;
		color: var(--color-foreground-subtle);
	}

	.header-label {
		font-family: var(--font-serif);
		font-size: 11px;
		font-weight: 500;
		text-transform: uppercase;
		letter-spacing: 0.14em;
	}

	.header-menu {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 20px;
		height: 20px;
		border: none;
		border-radius: 4px;
		background: none;
		cursor: pointer;
		color: var(--color-foreground-subtle);
		opacity: 0.5;
		transition: opacity 150ms ease, background 150ms ease;
	}

	.section-header:hover .header-menu,
	.header-menu.active,
	.header-menu:focus-visible {
		opacity: 1;
	}

	.header-menu:hover {
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
	}

	.filter-menu {
		display: flex;
		flex-direction: column;
		gap: 1px;
		margin: 2px 0 4px;
		padding: 4px;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: var(--color-surface-overlay);
	}

	.menu-label {
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--color-foreground-subtle);
		padding: 2px 6px;
	}

	.menu-row {
		display: flex;
		align-items: center;
		gap: 6px;
		width: 100%;
		padding: 4px 6px;
		border: none;
		border-radius: 4px;
		background: none;
		cursor: pointer;
		font-size: 12px;
		color: var(--color-foreground-muted);
		text-align: left;
	}

	.menu-row:hover {
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
		color: var(--color-foreground);
	}

	.menu-row.checked {
		color: var(--color-foreground);
	}

	.menu-row.danger:hover {
		background: var(--error-subtle);
		color: var(--error);
	}

	.menu-divider {
		height: 1px;
		margin: 3px 4px;
		background: var(--color-border);
	}

	.recents-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
	}

	.recent-row {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 4px 8px;
		border: none;
		border-radius: 6px;
		background: none;
		cursor: pointer;
		color: var(--color-foreground-muted);
		text-align: left;
	}

	.recent-row:hover {
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
		color: var(--color-foreground);
	}

	.recent-label {
		font-size: 13px;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.recents-empty {
		font-size: 12px;
		color: var(--color-foreground-subtle);
		padding: 4px 8px;
	}
</style>
