<script lang="ts">
	/**
	 * Projects: the list, laid out like Pages because it is the same kind of
	 * page. A table first (a project is a row you scan for, not a tile you
	 * admire), the cards one toggle away, the header's count, and one "New
	 * project" that asks for a name the way the sidebar and ⌘K do.
	 *
	 * Every row has the project's own menu (right-click, or the row's ⋯):
	 * the same one the sidebar row has, from `projectRowMenuItems`.
	 */
	import { onMount } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import IconButton from '$lib/components/IconButton.svelte';
	import ProjectGlyph from '$lib/components/ProjectGlyph.svelte';
	import { projectColor } from '$lib/sidebar/pin-colors';
	import { projectStore } from '$lib/stores/project.svelte';
	import { contextMenu } from '$lib/stores/contextMenu.svelte';
	import { Button, Page } from '$lib';
	import UniversalDataGrid, { type Column } from '$lib/components/datagrid/UniversalDataGrid.svelte';
	import type { ProjectSummary } from '$lib/api/client';
	import { formatRelativeTimestamp } from '$lib/utils/dateUtils';
	import { PROJECT_ICON } from '$lib/utils/iconHelpers';
	import {
		newProject,
		openProject,
		projectRowMenuItems,
		unarchiveProject,
	} from '$lib/utils/projectActions';

	let { active: _active }: { tab?: unknown; active?: boolean } = $props();

	onMount(() => {
		projectStore.load();
	});

	const projects = $derived(projectStore.projects);
	const archived = $derived(projectStore.archived);
	const firstLoad = $derived(!projectStore.loaded && projectStore.loading);

	// The header says how many, as Pages does; the explanation is for the
	// empty page, where it is the only thing to read.
	const description = $derived(
		projects.length === 1 ? '1 project' : `${projects.length} projects`
	);

	// Folded by default: the archive is where finished things wait, not the
	// list you scan. The count on the head says there is something there.
	let archivedOpen = $state(false);

	const columns: Column<ProjectSummary>[] = [
		{ key: 'name', label: 'Name', icon: PROJECT_ICON, width: '52%', minWidth: '200px' },
		{ key: 'chat_count', label: 'Chats', icon: 'ri:chat-3-line', width: '14%', minWidth: '70px', format: 'number' },
		{ key: 'item_count', label: 'Items', icon: 'ri:stack-line', width: '14%', minWidth: '70px', format: 'number', hideOnMobile: true },
		{
			key: 'updated_at',
			label: 'Updated',
			icon: 'ri:time-line',
			width: '20%',
			minWidth: '110px',
			hideOnMobile: true,
			getValue: (p) => formatRelativeTimestamp(p.updated_at)
		}
	];

	function showMenu(p: ProjectSummary, e: MouseEvent) {
		e.preventDefault();
		contextMenu.show({ x: e.clientX, y: e.clientY }, projectRowMenuItems(p));
	}

	/** "3 chats · 5 items", saying only what is there. */
	function countsLine(p: ProjectSummary): string {
		const parts: string[] = [];
		if (p.chat_count > 0) parts.push(p.chat_count === 1 ? '1 chat' : `${p.chat_count} chats`);
		if (p.item_count > 0) parts.push(p.item_count === 1 ? '1 item' : `${p.item_count} items`);
		return parts.join(' · ') || 'Empty';
	}
</script>

<Page title="Projects" {description} maxWidth="wide">
	{#snippet actions()}
		<Button variant="secondary" size="sm" icon="ri:add-line" onclick={newProject}>New project</Button>
	{/snippet}

	{#if projects.length === 0 && archived.length === 0 && !firstLoad && !projectStore.error}
		<div class="empty">
			<span class="empty-glyph"><Icon icon={PROJECT_ICON} width="28" /></span>
			<p class="empty-title">No projects yet</p>
			<p class="empty-body">
				A project keeps the chats, pages and files for one piece of work together. Every chat in
				it follows the brief you give it.
			</p>
			<Button variant="secondary" size="sm" icon="ri:add-line" onclick={newProject}>New project</Button>
		</div>
	{:else}
		<UniversalDataGrid
			items={projects}
			{columns}
			entityType="project"
			loading={firstLoad}
			error={projectStore.error}
			emptyIcon={PROJECT_ICON}
			emptyMessage={projects.length === 0 ? 'Every project is archived' : 'No projects match'}
			loadingMessage="Loading projects…"
			searchPlaceholder="Search projects…"
			defaultViewMode="table"
			gridMinWidth="220px"
			onItemClick={(p) => openProject(p)}
			onItemContextMenu={showMenu}
			onRetry={() => projectStore.load()}
		>
			{#snippet tableRow(p: ProjectSummary)}
				<td class="col-name">
					<span class="name-cell">
						<ProjectGlyph project={p} size={16} />
						<span class="name-text">{p.name}</span>
					</span>
				</td>
<td class="col-num">{p.chat_count || '—'}</td>
				<td class="col-num hide-mobile">{p.item_count || '—'}</td>
				<td class="col-dim hide-mobile">{formatRelativeTimestamp(p.updated_at)}</td>
			{/snippet}

			{#snippet rowActions(p: ProjectSummary)}
				<IconButton
					icon="ri:more-line"
					label={`Actions for ${p.name}`}
					size="sm"
					haspopup="menu"
					onclick={(e) => showMenu(p, e)}
				/>
			{/snippet}

			{#snippet card(p: ProjectSummary)}
				<div class="card" style={`--room-accent: ${projectColor(p)}`}>
					<div class="card-glyph"><ProjectGlyph project={p} size={18} inherit /></div>
					<div class="card-name">{p.name}</div>
<div class="card-meta">{countsLine(p)}</div>
				</div>
			{/snippet}
		</UniversalDataGrid>
	{/if}

	{#if archived.length > 0}
		<section class="archived">
			<button
				type="button"
				class="archived-head"
				aria-expanded={archivedOpen}
				onclick={() => (archivedOpen = !archivedOpen)}
			>
				<Icon icon={archivedOpen ? 'ri:arrow-down-s-line' : 'ri:arrow-right-s-line'} width="14" />
				<span>Archived</span>
				<span class="archived-count">{archived.length}</span>
			</button>
			{#if archivedOpen}
				<ul class="archived-list">
					{#each archived as p (p.id)}
						<li class="archived-row" oncontextmenu={(e) => showMenu(p, e)}>
							<button type="button" class="archived-name" onclick={() => openProject(p)}>
								<ProjectGlyph project={p} size={15} />
								<span>{p.name}</span>
							</button>
							<span class="archived-when">
								{p.archived_at ? `Archived ${formatRelativeTimestamp(p.archived_at)}` : ''}
							</span>
							<Button variant="secondary" size="sm" onclick={() => unarchiveProject(p)}>Unarchive</Button>
						</li>
					{/each}
				</ul>
			{/if}
		</section>
	{/if}
</Page>

<style>
	/* Table cells: the same padding as the grid's own headers, so every label
	   sits over its values (Pages uses the same). */
	.col-name {
		padding: 0.625rem 0.75rem;
	}
	.name-cell {
		display: inline-flex;
		align-items: center;
		gap: 0.5rem;
		min-width: 0;
		max-width: 100%;
	}
	.name-cell > :global(svg),
	.name-cell > :global(.v-icon-emoji) {
		flex-shrink: 0;
	}
	.name-text {
		font-weight: 500;
		color: var(--color-foreground);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.col-dim,
	.col-num {
		padding: 0.625rem 0.75rem;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}
	.col-num {
		font-variant-numeric: tabular-nums;
	}
@media (max-width: 768px) {
		.hide-mobile {
			display: none;
		}
	}

	/* Cards: quiet, the project's color in its chip rather than a stripe down
	   the side, which read as a selected state. */
	.card {
		display: flex;
		flex-direction: column;
		gap: 6px;
		width: 100%;
		height: 100%;
		padding: 14px;
		border: 1px solid var(--color-border);
		border-radius: 12px;
		background: var(--color-surface);
		transition:
			border-color 0.12s ease,
			background 0.12s ease;
	}
	.card:hover {
		background: var(--color-background-hover);
		border-color: color-mix(in srgb, var(--room-accent) 30%, var(--color-border));
	}
	.card-glyph {
		display: grid;
		place-items: center;
		width: 32px;
		height: 32px;
		margin-bottom: 2px;
		border-radius: 8px;
		background: color-mix(in srgb, var(--room-accent) 16%, transparent);
		color: color-mix(in srgb, var(--room-accent) 78%, var(--color-foreground));
	}
	.card-name {
		font-size: 0.9375rem;
		font-weight: 550;
		color: var(--color-foreground);
	}
.card-meta {
		margin-top: auto;
		padding-top: 4px;
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
	}

	/* Empty: the one place the page explains itself. */
	.empty {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 8px;
		max-width: 26rem;
		margin: 0 auto;
		padding: 72px 0;
		text-align: center;
	}
	.empty-glyph {
		color: var(--color-foreground-subtle);
	}
	.empty-title {
		margin: 4px 0 0;
		font-size: 0.9375rem;
		font-weight: 500;
		color: var(--color-foreground);
	}
	.empty-body {
		margin: 0 0 8px;
		font-size: 0.875rem;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	/* The Archived fold: a group head like the sidebar's, air above it, no rule. */
	.archived {
		margin-top: 32px;
	}
	.archived-head {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 4px 0;
		border: none;
		background: none;
		cursor: pointer;
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
	}
	.archived-head:hover {
		color: var(--color-foreground-muted);
	}
	.archived-count {
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground-disabled);
	}
	.archived-list {
		list-style: none;
		margin: 8px 0 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.archived-row {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 6px 0.75rem;
		border-radius: 8px;
	}
	.archived-row:hover {
		background: var(--color-background-hover);
	}
	.archived-name {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		flex: 1;
		min-width: 0;
		border: none;
		background: none;
		padding: 0;
		cursor: pointer;
		text-align: left;
		font-size: 0.875rem;
		color: var(--color-foreground-muted);
	}
	.archived-name:hover {
		color: var(--color-foreground);
	}
	.archived-name span {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.archived-when {
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
		white-space: nowrap;
	}
</style>
