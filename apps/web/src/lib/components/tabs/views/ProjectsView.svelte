<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import type { ProjectSummary } from '$lib/api/client';
	import { onMount } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { projectsStore } from '$lib/stores/projects.svelte';
	import { spaceStore } from '$lib/stores/space.svelte';
	import { contextMenu, type ContextMenuItem } from '$lib/stores/contextMenu.svelte';
	import { iconPickerStore } from '$lib/stores/iconPicker.svelte';

	let { tab: _tab, active: _active }: { tab: Tab; active: boolean } = $props();

	const projects = $derived(projectsStore.projects);
	const loading = $derived(projectsStore.loading);
	const error = $derived(projectsStore.error);

	let searchQuery = $state('');
	let creating = $state(false);
	let newName = $state('');

	const filtered = $derived.by(() => {
		const q = searchQuery.trim().toLowerCase();
		if (!q) return projects;
		return projects.filter(
			(p) =>
				p.name.toLowerCase().includes(q) ||
				(p.description?.toLowerCase().includes(q) ?? false),
		);
	});

	onMount(() => {
		projectsStore.load();
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

	function openProject(project: ProjectSummary, e?: MouseEvent) {
		const forceNew = !!(e && (e.metaKey || e.ctrlKey));
		spaceStore.openTabFromRoute(`/projects/${project.id}`, {
			forceNew,
			label: project.name,
			preferEmptyPane: true,
		});
	}

	async function startCreate() {
		creating = true;
		newName = '';
		// Focus is applied via autofocus on the input below
	}

	async function submitCreate() {
		const name = newName.trim();
		if (!name) {
			creating = false;
			return;
		}
		try {
			const project = await projectsStore.create(name);
			creating = false;
			newName = '';
			openProject({ ...project, item_count: 0 });
		} catch (e) {
			console.error('[ProjectsView] Failed to create project:', e);
		}
	}

	function cancelCreate() {
		creating = false;
		newName = '';
	}

	function handleContextMenu(e: MouseEvent, project: ProjectSummary) {
		e.preventDefault();
		e.stopPropagation();
		const items: ContextMenuItem[] = [
			{
				id: 'open-new-tab',
				label: 'Open in New Tab',
				icon: 'ri:external-link-line',
				action: () => {
					spaceStore.openTabFromRoute(`/projects/${project.id}`, {
						forceNew: true,
						label: project.name,
						preferEmptyPane: true,
					});
				},
			},
			{
				id: 'change-icon',
				label: 'Change Icon',
				icon: 'ri:emotion-line',
				action: () => {
					iconPickerStore.show(project.icon ?? null, async (icon) => {
						try {
							await projectsStore.update(project.id, { icon });
						} catch (err) {
							console.error('[ProjectsView] Failed to change icon:', err);
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
					if (!confirm(`Delete project "${project.name}"? Items are detached, not deleted.`)) return;
					try {
						spaceStore.closeTabsByRoute(`/projects/${project.id}`);
						await projectsStore.remove(project.id);
					} catch (err) {
						console.error('[ProjectsView] Failed to delete project:', err);
					}
				},
			},
		];
		contextMenu.show({ x: e.clientX, y: e.clientY }, items);
	}
</script>

<div class="projects-view">
	<header class="header">
		<div class="title-row">
			<h1>Projects</h1>
			<button type="button" class="new-btn" onclick={startCreate} disabled={creating}>
				<Icon icon="ri:add-line" width="14" />
				<span>New Project</span>
			</button>
		</div>
		<p class="subtitle">
			An annotated bookmark collection — internal items (pages, chats, entities, files) and external URLs (articles, videos, docs) — that you @-mention in chat to focus the agent's attention.
		</p>
		<input
			type="search"
			class="search"
			placeholder="Search projects…"
			bind:value={searchQuery}
		/>
	</header>

	<main class="content">
		{#if creating}
			<!-- svelte-ignore a11y_autofocus -->
			<div class="create-row">
				<Icon icon="ri:folder-open-line" width="16" />
				<input
					type="text"
					class="create-input"
					placeholder="Project name…"
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

		{#if loading && projects.length === 0}
			<div class="status">Loading projects…</div>
		{:else if error}
			<div class="status error">Failed to load projects: {error}</div>
		{:else if projects.length === 0 && !creating}
			<div class="empty">
				<div class="empty-title">No projects yet</div>
				<div class="empty-body">
					A project is a table of references — pages, chats, people, places, files — that
					you can @-mention in chat to focus the agent. Create one above to get started.
				</div>
			</div>
		{:else}
			<table class="projects-table">
				<thead>
					<tr>
						<th class="col-icon"></th>
						<th class="col-name">Name</th>
						<th class="col-desc">Description</th>
						<th class="col-count">Items</th>
						<th class="col-updated">Updated</th>
					</tr>
				</thead>
				<tbody>
					{#each filtered as project (project.id)}
						<tr
							class="project-row"
							onclick={(e) => openProject(project, e)}
							oncontextmenu={(e) => handleContextMenu(e, project)}
						>
							<td class="col-icon">
								<Icon icon={project.icon || 'ri:folder-open-line'} width="18" />
							</td>
							<td class="col-name">{project.name}</td>
							<td class="col-desc">{project.description ?? ''}</td>
							<td class="col-count">{project.item_count}</td>
							<td class="col-updated">{formatDate(project.updated_at)}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		{/if}
	</main>
</div>

<style>
	@reference "../../../../app.css";

	.projects-view {
		display: flex;
		flex-direction: column;
		height: 100%;
		min-height: 0;
	}

	.header {
		padding: 1.5rem 2rem 1rem;
		border-bottom: 1px solid var(--color-border, #e5e7eb);
		flex-shrink: 0;
	}

	.title-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		margin-bottom: 0.25rem;
	}

	.title-row h1 {
		font-size: 1.5rem;
		font-weight: 600;
		margin: 0;
		color: var(--color-foreground, inherit);
	}

	.subtitle {
		font-size: 0.8125rem;
		color: var(--color-foreground-muted, #6b7280);
		margin: 0 0 0.75rem;
	}

	.new-btn {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0.375rem 0.75rem;
		font-size: 0.8125rem;
		font-weight: 500;
		color: var(--color-foreground, inherit);
		background: var(--color-surface-raised, #f9fafb);
		border: 1px solid var(--color-border, #e5e7eb);
		border-radius: 6px;
		cursor: pointer;
	}
	.new-btn:hover:not(:disabled) {
		background: var(--color-surface-hover, #f3f4f6);
	}
	.new-btn:disabled {
		opacity: 0.5;
		cursor: default;
	}

	.search {
		width: 100%;
		max-width: 320px;
		padding: 0.375rem 0.625rem;
		font-size: 0.8125rem;
		border: 1px solid var(--color-border, #e5e7eb);
		border-radius: 6px;
		background: var(--color-surface, #fff);
	}

	.content {
		flex: 1;
		overflow-y: auto;
		padding: 1rem 2rem 2rem;
	}

	.create-row {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		padding: 0.625rem 0.75rem;
		border: 1px dashed var(--color-border-strong, #d1d5db);
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
	}

	.status,
	.empty {
		padding: 2rem 1rem;
		text-align: center;
		color: var(--color-foreground-muted, #6b7280);
	}
	.status.error {
		color: #b91c1c;
	}
	.empty-title {
		font-size: 1rem;
		font-weight: 500;
		margin-bottom: 0.5rem;
		color: var(--color-foreground, inherit);
	}
	.empty-body {
		font-size: 0.8125rem;
		max-width: 44ch;
		margin: 0 auto;
		line-height: 1.5;
	}

	.projects-table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.875rem;
	}
	.projects-table th {
		text-align: left;
		font-weight: 500;
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		color: var(--color-foreground-subtle, #9ca3af);
		padding: 0.5rem 0.75rem;
		border-bottom: 1px solid var(--color-border, #e5e7eb);
	}
	.projects-table td {
		padding: 0.625rem 0.75rem;
		border-bottom: 1px solid var(--color-border-subtle, #f3f4f6);
		vertical-align: middle;
	}
	.project-row {
		cursor: pointer;
	}
	.project-row:hover td {
		background: var(--color-surface-hover, #f9fafb);
	}

	.col-icon {
		width: 36px;
		text-align: center;
	}
	.col-name {
		font-weight: 500;
		color: var(--color-foreground, inherit);
	}
	.col-desc {
		color: var(--color-foreground-muted, #6b7280);
		max-width: 40ch;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.col-count {
		width: 60px;
		text-align: right;
		color: var(--color-foreground-muted, #6b7280);
		font-variant-numeric: tabular-nums;
	}
	.col-updated {
		width: 120px;
		color: var(--color-foreground-muted, #6b7280);
		font-variant-numeric: tabular-nums;
	}
</style>
