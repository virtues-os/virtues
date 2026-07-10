<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import type { ThingSummary } from '$lib/api/client';
	import { onMount } from 'svelte';
	import { Page, Button } from '$lib';
	import Icon from '$lib/components/Icon.svelte';
	import UniversalDataGrid, { type Column } from '$lib/components/datagrid/UniversalDataGrid.svelte';
	import { thingsStore } from '$lib/stores/things.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { contextMenu, type ContextMenuItem } from '$lib/stores/contextMenu.svelte';
	import { iconPickerStore } from '$lib/stores/iconPicker.svelte';

	let { tab: _tab, active: _active }: { tab: Tab; active: boolean } = $props();

	const things = $derived(thingsStore.things);
	const loading = $derived(thingsStore.loading);
	const error = $derived(thingsStore.error);

	let creating = $state(false);
	let newName = $state('');

	onMount(() => {
		thingsStore.load();
	});

	function formatDate(dateStr: string): string {
		const d = new Date(dateStr);
		const now = new Date();
		const diffDays = Math.floor((now.getTime() - d.getTime()) / 86_400_000);
		if (diffDays === 0) return d.toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit' });
		if (diffDays === 1) return 'Yesterday';
		if (diffDays < 7) return d.toLocaleDateString('en-US', { weekday: 'long' });
		return d.toLocaleDateString('en-US', {
			month: 'short',
			day: 'numeric',
			year: d.getFullYear() !== now.getFullYear() ? 'numeric' : undefined,
		});
	}

	const columns: Column<ThingSummary>[] = [
		{ key: 'name', label: 'Name', icon: 'ri:folder-open-line', width: '30%', minWidth: '160px' },
		{ key: 'description', label: 'Description', icon: 'ri:text', width: '52%', minWidth: '200px', hideOnMobile: true },
		{ key: 'updated_at', label: 'Updated', icon: 'ri:time-line', width: '18%', minWidth: '120px', hideOnMobile: true, getValue: (t) => formatDate(t.updated_at) },
	];

	function openThing(thing: ThingSummary, e?: MouseEvent) {
		const forceNew = !!(e && (e.metaKey || e.ctrlKey));
		windowShellStore.openTabFromRoute(`/thing/${thing.id}`, {
			forceNew,
			label: thing.name,
			preferEmptyPane: true,
		});
	}

	function startCreate() {
		creating = true;
		newName = '';
	}

	async function submitCreate() {
		const name = newName.trim();
		if (!name) {
			creating = false;
			return;
		}
		try {
			const thing = await thingsStore.create(name);
			creating = false;
			newName = '';
			openThing(thing);
		} catch (e) {
			console.error('[ThingsView] Failed to create thing:', e);
		}
	}

	function cancelCreate() {
		creating = false;
		newName = '';
	}

	function handleContextMenu(thing: ThingSummary, e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();
		const items: ContextMenuItem[] = [
			{
				id: 'open-new-tab',
				label: 'Open in New Tab',
				icon: 'ri:external-link-line',
				action: () => {
					windowShellStore.openTabFromRoute(`/thing/${thing.id}`, {
						forceNew: true,
						label: thing.name,
						preferEmptyPane: true,
					});
				},
			},
			{
				id: 'change-icon',
				label: 'Change Icon',
				icon: 'ri:emotion-line',
				action: () => {
					iconPickerStore.show(thing.icon ?? null, async (icon) => {
						try {
							await thingsStore.update(thing.id, { icon });
						} catch (err) {
							console.error('[ThingsView] Failed to change icon:', err);
						}
					});
				},
			},
			{
				id: 'delete',
				label: 'Delete',
				icon: 'ri:delete-bin-line',
				variant: 'destructive',
				dividerBefore: true,
				action: async () => {
					if (!confirm(`Delete thing "${thing.name}"? Items are detached, not deleted.`)) return;
					try {
						windowShellStore.closeTabsByRoute(`/thing/${thing.id}`);
						await thingsStore.remove(thing.id);
					} catch (err) {
						console.error('[ThingsView] Failed to delete thing:', err);
					}
				},
			},
		];
		contextMenu.show({ x: e.clientX, y: e.clientY }, items);
	}
</script>

<Page
	title="Things"
	description="A thing is an entity you reference — a pet, a car, a book, a concept: anything worth resolving and linking that isn't a person, place, or org. @-mention it in chat, or gather things into a Notebook to work with them."
	maxWidth="wide"
>
	{#snippet actions()}
		<Button onclick={startCreate} disabled={creating}>
			<Icon icon="ri:add-line" width="14" />
			<span class="ml-1">New Thing</span>
		</Button>
	{/snippet}

	{#if creating}
		<!-- svelte-ignore a11y_autofocus -->
		<div class="create-row">
			<Icon icon="ri:folder-open-line" width="16" />
			<input
				type="text"
				class="create-input"
				placeholder="Thing name…"
				bind:value={newName}
				autofocus
				onkeydown={(e) => {
					if (e.key === 'Enter') submitCreate();
					else if (e.key === 'Escape') cancelCreate();
				}}
				onblur={submitCreate}
			/>
		</div>
	{/if}

	<UniversalDataGrid
		items={things}
		{columns}
		entityType="thing"
		{loading}
		{error}
		emptyIcon="ri:folder-open-line"
		emptyMessage="No things yet"
		loadingMessage="Loading things…"
		searchPlaceholder="Search things…"
		onItemClick={openThing}
		onItemContextMenu={handleContextMenu}
		onRetry={() => thingsStore.load()}
	>
		{#snippet tableRow(thing: ThingSummary)}
			<td class="col-icon">
				<Icon icon={thing.icon || 'ri:folder-open-line'} width="18" />
			</td>
			<td class="col-name">{thing.name}</td>
			<td class="col-desc">{thing.description ?? ''}</td>
			<td class="col-updated">{formatDate(thing.updated_at)}</td>
		{/snippet}
	</UniversalDataGrid>
</Page>

<style>
	.create-row {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		padding: 0.625rem 0.75rem;
		border: 1px dashed var(--color-border-strong);
		border-radius: 6px;
		margin-bottom: 0.75rem;
	}
	.create-input {
		flex: 1;
		font: inherit;
		font-size: 0.875rem;
		border: none;
		outline: none;
		background: transparent;
		color: var(--color-foreground);
	}

	.col-icon {
		width: 36px;
		text-align: center;
		padding: 0.625rem 0.75rem;
	}
	.col-name {
		font-weight: 500;
		color: var(--color-foreground);
		padding: 0.625rem 0.75rem;
	}
	.col-desc {
		color: var(--color-foreground-muted);
		max-width: 40ch;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		padding: 0.625rem 0.75rem;
	}
	.col-updated {
		width: 120px;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
		padding: 0.625rem 0.75rem;
	}
</style>
