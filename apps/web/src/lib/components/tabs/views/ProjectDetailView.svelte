<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import type { ProjectDetail, ProjectItem } from '$lib/api/client';
	import { onMount } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { projectsStore } from '$lib/stores/projects.svelte';
	import { spaceStore } from '$lib/stores/space.svelte';
	import { contextMenu, type ContextMenuItem } from '$lib/stores/contextMenu.svelte';
	import { iconPickerStore } from '$lib/stores/iconPicker.svelte';

	let { tab, active: _active }: { tab: Tab; active: boolean } = $props();

	// Parse project id from tab.route: "/projects/prj_xxx"
	const projectId = $derived.by(() => {
		const match = tab.route.match(/^\/projects\/(prj_[^/]+)$/);
		return match?.[1] ?? null;
	});

	let detail = $state<ProjectDetail | null>(null);
	let loading = $state(false);
	let error = $state<string | null>(null);

	// Local editable fields (for inline rename/description edit)
	let editingName = $state(false);
	let nameDraft = $state('');
	let editingDescription = $state(false);
	let descriptionDraft = $state('');

	async function loadDetail(force = false) {
		if (!projectId) return;
		loading = true;
		error = null;
		try {
			detail = await projectsStore.loadDetail(projectId, force);
		} catch (e) {
			console.error('[ProjectDetailView] Failed to load project:', e);
			error = e instanceof Error ? e.message : 'Failed to load project';
			detail = null;
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		loadDetail();
	});

	$effect(() => {
		// Reload when the tab switches to a different project
		if (projectId) {
			loadDetail();
		}
	});

	function itemTypeLabel(url: string): string {
		if (url.startsWith('/page/')) return 'Page';
		if (url.startsWith('/chat/')) return 'Chat';
		if (url.startsWith('/person/')) return 'Person';
		if (url.startsWith('/place/')) return 'Place';
		if (url.startsWith('/org/')) return 'Organization';
		if (url.startsWith('/thing/')) return 'Thing';
		if (url.startsWith('/day/')) return 'Day';
		if (url.startsWith('/drive/')) return 'File';
		if (url.startsWith('/source/') || url.startsWith('/sources/')) return 'Source';
		if (url.startsWith('http')) return 'Link';
		return 'Reference';
	}

	function itemFallbackIcon(url: string): string {
		if (url.startsWith('/page/')) return 'ri:file-text-line';
		if (url.startsWith('/chat/')) return 'ri:chat-1-line';
		if (url.startsWith('/person/')) return 'ri:user-line';
		if (url.startsWith('/place/')) return 'ri:map-pin-line';
		if (url.startsWith('/org/')) return 'ri:building-line';
		if (url.startsWith('/thing/')) return 'ri:cube-line';
		if (url.startsWith('/day/')) return 'ri:calendar-line';
		if (url.startsWith('/drive/')) return 'ri:file-line';
		return 'ri:link';
	}

	function openItem(item: ProjectItem, e?: MouseEvent) {
		const forceNew = !!(e && (e.metaKey || e.ctrlKey));
		spaceStore.openTabFromRoute(item.url, {
			forceNew,
			label: item.name ?? undefined,
			preferEmptyPane: true,
		});
	}

	function formatDate(dateStr: string): string {
		return new Date(dateStr).toLocaleDateString('en-US', {
			month: 'short',
			day: 'numeric',
			year: 'numeric',
		});
	}

	function startRenameName() {
		if (!detail) return;
		nameDraft = detail.name;
		editingName = true;
	}

	async function commitRenameName() {
		if (!detail) return;
		const name = nameDraft.trim();
		if (!name || name === detail.name) {
			editingName = false;
			return;
		}
		try {
			const updated = await projectsStore.update(detail.id, { name });
			detail = { ...detail, ...updated };
		} catch (e) {
			console.error('[ProjectDetailView] Failed to rename project:', e);
		} finally {
			editingName = false;
		}
	}

	function startEditDescription() {
		if (!detail) return;
		descriptionDraft = detail.description ?? '';
		editingDescription = true;
	}

	async function commitEditDescription() {
		if (!detail) return;
		const trimmed = descriptionDraft.trim();
		const value: string | null = trimmed === '' ? null : trimmed;
		if (value === (detail.description ?? null)) {
			editingDescription = false;
			return;
		}
		try {
			const updated = await projectsStore.update(detail.id, { description: value });
			detail = { ...detail, ...updated };
		} catch (e) {
			console.error('[ProjectDetailView] Failed to update description:', e);
		} finally {
			editingDescription = false;
		}
	}

	function changeProjectIcon() {
		if (!detail) return;
		iconPickerStore.show(detail.icon ?? null, async (icon) => {
			try {
				const updated = await projectsStore.update(detail!.id, { icon });
				detail = { ...detail!, ...updated };
			} catch (e) {
				console.error('[ProjectDetailView] Failed to change icon:', e);
			}
		});
	}

	async function removeItem(item: ProjectItem) {
		if (!detail) return;
		try {
			await projectsStore.removeItem(detail.id, item.url);
			detail = { ...detail, items: detail.items.filter((i) => i.url !== item.url) };
		} catch (e) {
			console.error('[ProjectDetailView] Failed to remove item:', e);
		}
	}

	function handleItemContextMenu(e: MouseEvent, item: ProjectItem) {
		e.preventDefault();
		e.stopPropagation();
		const items: ContextMenuItem[] = [
			{
				id: 'open-new-tab',
				label: 'Open in New Tab',
				icon: 'ri:external-link-line',
				action: () => {
					spaceStore.openTabFromRoute(item.url, {
						forceNew: true,
						label: item.name ?? undefined,
						preferEmptyPane: true,
					});
				},
			},
			{
				id: 'remove-from-project',
				label: 'Remove from Project',
				icon: 'ri:close-line',
				variant: 'destructive',
				dividerBefore: true,
				action: () => removeItem(item),
			},
		];
		contextMenu.show({ x: e.clientX, y: e.clientY }, items);
	}
</script>

<div class="project-detail">
	{#if !projectId}
		<div class="status error">Invalid project route: {tab.route}</div>
	{:else if loading && !detail}
		<div class="status">Loading…</div>
	{:else if error}
		<div class="status error">Failed to load project: {error}</div>
	{:else if detail}
		<header class="header">
			<button type="button" class="icon-btn" onclick={changeProjectIcon} title="Change icon">
				<Icon icon={detail.icon || 'ri:folder-open-line'} width="28" />
			</button>
			<div class="title-block">
				{#if editingName}
					<!-- svelte-ignore a11y_autofocus -->
					<input
						type="text"
						class="name-input"
						bind:value={nameDraft}
						autofocus
						onkeydown={(e) => {
							if (e.key === 'Enter') commitRenameName();
							else if (e.key === 'Escape') editingName = false;
						}}
						onblur={commitRenameName}
					/>
				{:else}
					<!-- svelte-ignore a11y_click_events_have_key_events -->
					<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
					<h1 onclick={startRenameName}>{detail.name}</h1>
				{/if}

				{#if editingDescription}
					<!-- svelte-ignore a11y_autofocus -->
					<input
						type="text"
						class="description-input"
						bind:value={descriptionDraft}
						placeholder="What's this project about?"
						autofocus
						onkeydown={(e) => {
							if (e.key === 'Enter') commitEditDescription();
							else if (e.key === 'Escape') editingDescription = false;
						}}
						onblur={commitEditDescription}
					/>
				{:else if detail.description}
					<!-- svelte-ignore a11y_click_events_have_key_events -->
					<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
					<p class="description" onclick={startEditDescription}>{detail.description}</p>
				{:else}
					<button type="button" class="description-add" onclick={startEditDescription}>
						Add description…
					</button>
				{/if}
			</div>
		</header>

		<main class="content">
			<div class="items-header">
				<h2>Items</h2>
				<span class="items-count">{detail.items.length}</span>
			</div>

			{#if detail.items.length === 0}
				<div class="empty">
					No items yet. Add pages, chats, people, places, or files to this project by
					right-clicking them in the sidebar or on their detail pages.
				</div>
			{:else}
				<table class="items-table">
					<thead>
						<tr>
							<th class="col-icon"></th>
							<th class="col-name">Name</th>
							<th class="col-desc">Description</th>
							<th class="col-kind">Kind</th>
							<th class="col-actions"></th>
						</tr>
					</thead>
					<tbody>
						{#each detail.items as item (item.id)}
							<tr
								class="item-row"
								onclick={(e) => openItem(item, e)}
								oncontextmenu={(e) => handleItemContextMenu(e, item)}
							>
								<td class="col-icon">
									<Icon icon={itemFallbackIcon(item.url)} width="16" />
								</td>
								<td class="col-name">
									<span class="item-name">{item.name ?? item.url}</span>
								</td>
								<td class="col-desc">
									<span class="item-desc">{item.description ?? ''}</span>
								</td>
								<td class="col-kind">
									<span class="kind-badge" class:external={item.url.startsWith('http')}>
										{item.url.startsWith('http') ? 'External' : itemTypeLabel(item.url)}
									</span>
								</td>
								<td class="col-actions">
									<button
										type="button"
										class="row-action"
										title="Remove from project"
										onclick={(e) => {
											e.stopPropagation();
											removeItem(item);
										}}
									>
										<Icon icon="ri:close-line" width="14" />
									</button>
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			{/if}
		</main>
	{/if}
</div>

<style>
	@reference "../../../../app.css";

	.project-detail {
		display: flex;
		flex-direction: column;
		height: 100%;
		min-height: 0;
	}

	.header {
		display: flex;
		align-items: flex-start;
		gap: 1rem;
		padding: 2rem 2rem 1.25rem;
		border-bottom: 1px solid var(--color-border, #e5e7eb);
		flex-shrink: 0;
	}

	.icon-btn {
		width: 48px;
		height: 48px;
		display: flex;
		align-items: center;
		justify-content: center;
		border: 1px solid var(--color-border, #e5e7eb);
		border-radius: 8px;
		background: var(--color-surface, #fff);
		cursor: pointer;
		flex-shrink: 0;
	}
	.icon-btn:hover {
		background: var(--color-surface-hover, #f9fafb);
	}

	.title-block {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}

	.title-block h1 {
		font-size: 1.5rem;
		font-weight: 600;
		margin: 0;
		color: var(--color-foreground, inherit);
		cursor: text;
	}

	.name-input {
		font: inherit;
		font-size: 1.5rem;
		font-weight: 600;
		border: 1px solid var(--color-border, #e5e7eb);
		border-radius: 6px;
		padding: 0.125rem 0.375rem;
		background: var(--color-surface, #fff);
		color: var(--color-foreground, inherit);
		width: 100%;
	}

	.description {
		font-size: 0.875rem;
		color: var(--color-foreground-muted, #6b7280);
		margin: 0;
		cursor: text;
	}

	.description-input {
		font: inherit;
		font-size: 0.875rem;
		border: 1px solid var(--color-border, #e5e7eb);
		border-radius: 6px;
		padding: 0.25rem 0.5rem;
		background: var(--color-surface, #fff);
		width: 100%;
	}

	.description-add {
		align-self: flex-start;
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle, #9ca3af);
		background: transparent;
		border: none;
		padding: 0;
		cursor: pointer;
	}
	.description-add:hover {
		color: var(--color-foreground-muted, #6b7280);
	}

	.content {
		flex: 1;
		overflow-y: auto;
		padding: 1.25rem 2rem 2rem;
	}

	.items-header {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
		margin-bottom: 0.75rem;
	}
	.items-header h2 {
		font-size: 0.875rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--color-foreground-subtle, #9ca3af);
		margin: 0;
	}
	.items-count {
		font-size: 0.75rem;
		color: var(--color-foreground-subtle, #9ca3af);
		font-variant-numeric: tabular-nums;
	}

	.status,
	.empty {
		padding: 2rem 1rem;
		text-align: center;
		color: var(--color-foreground-muted, #6b7280);
		font-size: 0.8125rem;
		max-width: 44ch;
		margin: 0 auto;
		line-height: 1.5;
	}
	.status.error {
		color: #b91c1c;
	}

	.items-table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.875rem;
	}
	.items-table th {
		text-align: left;
		font-weight: 500;
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		color: var(--color-foreground-subtle, #9ca3af);
		padding: 0.5rem 0.75rem;
		border-bottom: 1px solid var(--color-border, #e5e7eb);
	}
	.items-table td {
		padding: 0.625rem 0.75rem;
		border-bottom: 1px solid var(--color-border-subtle, #f3f4f6);
		vertical-align: middle;
	}
	.item-row {
		cursor: pointer;
	}
	.item-row:hover td {
		background: var(--color-surface-hover, #f9fafb);
	}
	.item-row:hover .row-action {
		opacity: 1;
	}

	.col-icon {
		width: 30px;
		text-align: center;
	}
	.col-name {
		color: var(--color-foreground, inherit);
		font-weight: 500;
		max-width: 32ch;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.item-name {
		display: block;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.col-desc {
		color: var(--color-foreground-muted, #6b7280);
		max-width: 36ch;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.item-desc {
		display: block;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: 0.8125rem;
	}
	.col-kind {
		width: 90px;
	}
	.kind-badge {
		display: inline-block;
		font-size: 0.6875rem;
		font-weight: 500;
		padding: 0.0625rem 0.375rem;
		border-radius: 999px;
		color: var(--color-foreground-muted, #6b7280);
		background: var(--color-surface-raised, #f3f4f6);
	}
	.kind-badge.external {
		color: var(--color-primary, #4338ca);
		background: color-mix(in srgb, var(--color-primary, #4338ca) 10%, transparent);
	}
	.col-actions {
		width: 40px;
		text-align: right;
	}

	.row-action {
		background: transparent;
		border: none;
		padding: 0.25rem;
		color: var(--color-foreground-subtle, #9ca3af);
		cursor: pointer;
		border-radius: 4px;
		opacity: 0;
		transition: opacity 80ms ease;
	}
	.row-action:hover {
		color: var(--color-foreground, inherit);
		background: var(--color-surface-raised, #f3f4f6);
	}
</style>
