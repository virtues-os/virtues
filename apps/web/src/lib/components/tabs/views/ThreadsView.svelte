<script lang="ts">
	/**
	 * The Threads page: every thread you have, under three tabs.
	 *
	 *   Active    listed, open threads (backlog included)
	 *   Passive   unlisted, open threads: lesser concerns kept out of the sidebar
	 *   Archived  threads you finished or dropped
	 *
	 * Tasks and insights live in the Inbox. A click opens a thread, J and K
	 * walk the list, Enter opens the chosen one, F2 renames it, C makes a new
	 * thread. A row takes a dropped chat, page or pin.
	 */
	import type { Tab } from '$lib/tabs/types';
	import type { ProjectSummary } from '$lib/api/client';
	import ProjectGlyph from '$lib/components/ProjectGlyph.svelte';
	import { Button, Page } from '$lib';
	import { projectStore } from '$lib/stores/project.svelte';
	import { contextMenu } from '$lib/stores/contextMenu.svelte';
	import { dialogStore } from '$lib/stores/dialog.svelte';
	import { newThreadDialog } from '$lib/stores/newThread.svelte';
	import { newTaskDialog } from '$lib/stores/newTask.svelte';
	import { dropZone } from '$lib/actions/dropZone';
	import { formatRelativeTimestamp } from '$lib/utils/dateUtils';
	import {
		droppedRefUrl,
		fileIntoProject,
		newProject,
		openProject,
		outcomeLabel,
		projectRowMenuItems,
		REF_DRAG_TYPE,
	} from '$lib/utils/projectActions';
	import { renameRef } from '$lib/refs/identity.svelte';
	import { onMount } from 'svelte';

	let { tab: _tab, active = false }: { tab: Tab; active?: boolean } = $props();

	type Filter = 'active' | 'passive' | 'archived';
	const FILTERS: { id: Filter; label: string }[] = [
		{ id: 'active', label: 'Active' },
		{ id: 'passive', label: 'Passive' },
		{ id: 'archived', label: 'Archived' },
	];
	let filter = $state<Filter>('active');

	onMount(() => {
		projectStore.load();
	});

	const firstLoad = $derived(!projectStore.loaded && projectStore.loading);
	const rows = $derived.by<ProjectSummary[]>(() => {
		if (filter === 'archived') return projectStore.archived;
		const passive = filter === 'passive';
		// A server older than is_listed sends none, which means listed.
		return projectStore.projects.filter((p) => (p.is_listed === false) === passive);
	});

	// ── choosing with the keys ─────────────────────────────────────────────
	let chosenId = $state<string | null>(null);
	$effect(() => {
		void filter;
		chosenId = null;
	});

	function step(by: number) {
		if (rows.length === 0) return;
		const from = Math.max(
			0,
			rows.findIndex((r) => r.id === chosenId),
		);
		const next = chosenId ? rows[(from + by + rows.length) % rows.length] : rows[0];
		chosenId = next.id;
		const el = document.getElementById(`thread-row-${next.id}`);
		el?.scrollIntoView({ block: 'nearest' });
		if (document.activeElement?.closest('[role="listbox"]')) el?.focus();
	}

	function onKey(e: KeyboardEvent) {
		if (!active || e.defaultPrevented || e.metaKey || e.ctrlKey || e.altKey || e.repeat) return;
		if (e.key === 'Enter' && e.target instanceof Element && e.target.closest('button, a, [role="dialog"]')) return;
		if (newTaskDialog.isOpen || newThreadDialog.isOpen || dialogStore.pending) return;
		const target = e.target;
		if (target instanceof Element && target.closest('input, textarea, [contenteditable="true"], [role="menu"]')) return;
		if (e.key === 'c' || e.key === 'C') {
			e.preventDefault();
			void newProject();
		} else if (e.key === 'j' || e.key === 'ArrowDown') {
			e.preventDefault();
			step(1);
		} else if (e.key === 'k' || e.key === 'ArrowUp') {
			e.preventDefault();
			step(-1);
		} else if (e.key === 'Escape' && chosenId) {
			chosenId = null;
		} else if (e.key === 'Enter' && chosenId) {
			const p = rows.find((r) => r.id === chosenId);
			if (p) {
				e.preventDefault();
				openProject(p);
			}
		}
	}

	function rowMenu(p: ProjectSummary, e: MouseEvent) {
		e.preventDefault();
		contextMenu.show({ x: e.clientX, y: e.clientY }, projectRowMenuItems(p, { rename: () => startRename(p) }));
	}

	// ── renaming a row in place ───────────────────────────────────────────
	// Rename (the row's menu) or F2 makes the name a field. A click opens the
	// thread, so it does not rename.
	let renaming = $state<{ id: string; draft: string; original: string } | null>(null);

	function startRename(p: ProjectSummary) {
		renaming = { id: p.id, draft: p.name, original: p.name };
	}

	async function commitRename() {
		const r = renaming;
		renaming = null;
		const name = r?.draft.trim();
		if (!r || !name || name === r.original) return;
		await renameRef(`/thread/${r.id}`, name);
	}

	function focusAndSelect(node: HTMLInputElement) {
		node.focus();
		node.select();
	}

	let dropTarget = $state<string | null>(null);

	function dayOf(iso: string): string {
		const d = new Date(iso);
		const sameYear = d.getFullYear() === new Date().getFullYear();
		return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: sameYear ? undefined : 'numeric' });
	}
</script>

<svelte:window onkeydown={onKey} />

<Page title="Threads">
	<div class="head">
		<p class="lede">Each thread gathers the chats, pages, events and emails about one part of your life.</p>
		<Button variant="secondary" icon="mynaui:plus" onclick={() => newProject()} tip="New thread" tipKey="c">New thread</Button>
	</div>

	<div class="filters" role="tablist" aria-label="Show">
		{#each FILTERS as f (f.id)}
			<button class="vt" role="tab" aria-selected={filter === f.id} onclick={() => (filter = f.id)}>{f.label}</button>
		{/each}
	</div>

	{#if firstLoad}
		<div class="rows" aria-hidden="true">
			{#each [0, 1, 2] as i (i)}<div class="row placeholder"></div>{/each}
		</div>
	{:else if projectStore.error}
		<p class="empty">Your server couldn't load your threads. Check that it's running, then try again.</p>
	{:else if rows.length === 0}
		<p class="empty">
			{#if filter === 'active'}
				A thread is something happening in your life that you follow. It keeps its tasks, chats, pages and files together.
			{:else if filter === 'passive'}
				Passive threads stay out of the sidebar. Their things reach your Inbox only when they come near.
			{:else}
				Threads you finish or drop wait here.
			{/if}
		</p>
	{:else}
		<div class="rows" role="listbox" tabindex="-1" aria-label="Threads">
			{#each rows as p (p.id)}
				<div
					id={`thread-row-${p.id}`}
					class="row"
					class:drop-on={dropTarget === p.id}
					class:renaming={renaming?.id === p.id}
					role="option"
					tabindex={p.id === chosenId ? 0 : -1}
					aria-selected={p.id === chosenId}
					onfocus={() => (chosenId = p.id)}
					onclick={() => openProject(p)}
					onkeydown={(e) => {
						if (e.key === 'Enter') {
							e.preventDefault();
							openProject(p);
						} else if (e.key === 'F2') {
							e.preventDefault();
							startRename(p);
						}
					}}
					oncontextmenu={(e) => rowMenu(p, e)}
					use:dropZone={{
						accepts: (data) => !p.archived_at && data.types.includes(REF_DRAG_TYPE),
						onActive: (on) => (dropTarget = on ? p.id : dropTarget === p.id ? null : dropTarget),
						onDrop: async (e) => {
							const url = droppedRefUrl(e);
							if (url) await fileIntoProject(p, url);
						},
						effect: () => 'link',
					}}
				>
					<span class="glyph"><ProjectGlyph project={p} size={16} tinted /></span>
					<span class="text">
						{#if renaming?.id === p.id}
							<!-- The field keeps its keys and clicks: the row's own Enter
							     and click would open the thread. -->
							<input
								class="rename-input name name-field"
								aria-label="Name"
								placeholder="Untitled thread"
								bind:value={renaming.draft}
								use:focusAndSelect
								onclick={(e) => e.stopPropagation()}
								ondblclick={(e) => e.stopPropagation()}
								onkeydown={(e) => {
									e.stopPropagation();
									if (e.key === 'Enter') {
										e.preventDefault();
										void commitRename();
									} else if (e.key === 'Escape') {
										e.preventDefault();
										renaming = null;
									}
								}}
								onblur={() => void commitRename()}
							/>
						{:else}
							<span class="name">{p.name || 'Untitled thread'}</span>
						{/if}
						<span class="line">
							{p.is_adding_automatic ? 'Gathers on its own.' : 'You file things here.'}
							{#if p.current_status || p.instructions}{p.current_status || p.instructions}{/if}
						</span>
					</span>
					<span class="when">
						{p.archived_at ? `${outcomeLabel(p)} ${dayOf(p.archived_at)}` : formatRelativeTimestamp(p.updated_at)}
					</span>
				</div>
			{/each}
		</div>
	{/if}
</Page>

<style>
	.head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
	}
	.lede {
		margin: 0;
		max-width: 620px;
		font-size: var(--text-label);
		line-height: 20px;
		color: var(--color-foreground-muted);
	}
	.filters {
		display: flex;
		gap: 4px;
		margin: 24px 0 8px;
	}
	.vt {
		height: 28px;
		padding: 0 12px;
		border: 0;
		border-radius: var(--radius-md);
		background: transparent;
		font: inherit;
		font-size: var(--text-small);
		color: var(--color-foreground-muted);
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.vt:hover {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}
	.vt[aria-selected='true'] {
		background: var(--active-bg);
		color: var(--color-foreground);
	}

	.rows {
		display: flex;
		flex-direction: column;
	}
	.row {
		display: flex;
		align-items: center;
		gap: 12px;
		min-height: 52px;
		padding: 8px 12px;
		margin: 0 -12px;
		border-radius: var(--radius-md);
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.row:hover {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}
	.row[aria-selected='true'] {
		background: var(--active-bg);
	}
	.row.drop-on {
		background: var(--drop-tint);
	}
	.row.renaming {
		cursor: default;
	}
	/* Where rows will be while they load. */
	.row.placeholder {
		height: 40px;
		margin: 4px 0;
		background: var(--fill-bg);
		cursor: default;
	}
	.glyph {
		display: inline-flex;
		flex: none;
	}
	.text {
		display: flex;
		flex-direction: column;
		min-width: 0;
		flex: 1 1 auto;
	}
	.name {
		font-family: var(--font-serif-ui);
		font-size: var(--text-body);
		line-height: 20px;
		color: var(--color-foreground);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	/* Renaming: the name alone becomes the field (app.css). Its padding and
	   height are taken back by equal negative margins, so nothing moves. */
	.name-field {
		flex: none;
		height: 24px;
		margin: -2px -6px;
		padding: 0 6px;
		border-radius: var(--radius-sm);
		background: var(--rename-fill);
		outline: 1px solid var(--rename-edge);
		outline-offset: -1px;
	}
	.line {
		font-size: var(--text-small);
		line-height: 20px;
		color: var(--color-foreground-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.when {
		flex: none;
		font-size: var(--text-small);
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}
	.empty {
		margin: 16px 0 0;
		font-size: var(--text-label);
		line-height: 24px;
		color: var(--color-foreground-muted);
		max-width: 520px;
	}
	@media (prefers-reduced-motion: reduce) {
		.row,
		.vt {
			transition: none;
		}
	}
</style>
