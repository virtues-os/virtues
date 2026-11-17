<script lang="ts">
	/**
	 * The Threads door: what you follow and the steps in it.
	 *
	 *   Tasks    every open task, whatever its home (TaskList). The add row is
	 *            what "New task" and ⌘⇧E open.
	 *   Threads  Active, Backlog and Finished. A row is a thread's name and
	 *            where it stands. Pointing at a row shows it in the preview
	 *            column; a click keeps it there, and opening it goes to its
	 *            page. A row takes a dropped chat, page or pin.
	 *
	 * The view is in the route (`?view=tasks`), so a pin or a link can open
	 * either one.
	 */
	import type { Tab } from '$lib/tabs/types';
	import type { ProjectSummary, Task } from '$lib/api/client';
	import Icon from '$lib/components/Icon.svelte';
	import ProjectGlyph from '$lib/components/ProjectGlyph.svelte';
	import TaskList from '$lib/components/tasks/TaskList.svelte';
	import { Button, IconButton, Page } from '$lib';
	import { projectStore } from '$lib/stores/project.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { contextMenu } from '$lib/stores/contextMenu.svelte';
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
	import { onMount } from 'svelte';

	let { tab, active = false }: { tab: Tab; active?: boolean } = $props();

	type View = 'tasks' | 'threads';
	type Filter = 'active' | 'backlog' | 'finished';

	const params = $derived(new URLSearchParams(tab.route.split('?')[1] ?? ''));
	const view = $derived<View>(params.get('view') === 'tasks' ? 'tasks' : 'threads');

	function setView(next: View) {
		const query = next === 'tasks' ? '?view=tasks' : '';
		windowShellStore.updateTab(tab.id, { route: `/threads${query}` });
	}

	onMount(() => {
		projectStore.load();
	});

	let filter = $state<Filter>('active');
	const FILTERS: { id: Filter; label: string }[] = [
		{ id: 'active', label: 'Active' },
		{ id: 'backlog', label: 'Backlog' },
		{ id: 'finished', label: 'Finished' },
	];

	const firstLoad = $derived(!projectStore.loaded && projectStore.loading);
	const rows = $derived.by<ProjectSummary[]>(() => {
		if (filter === 'finished') return projectStore.archived;
		const backlog = filter === 'backlog';
		return projectStore.projects.filter((p) => !!p.is_backlog === backlog);
	});

	// ── the preview ─────────────────────────────────────────────────────
	// It follows the pointer until a click pins a row; Escape lets go.
	let hoveredId = $state<string | null>(null);
	let pinnedId = $state<string | null>(null);
	const shownId = $derived(pinnedId ?? hoveredId ?? rows[0]?.id ?? null);
	const shown = $derived(rows.find((r) => r.id === shownId) ?? null);
	const shownIndex = $derived(shown ? rows.indexOf(shown) : -1);
	const detail = $derived(shownId ? (projectStore.getCached(shownId) ?? null) : null);

	$effect(() => {
		const id = shownId;
		if (id && !projectStore.getCached(id)) {
			projectStore.get(id).catch((e) => console.error('[ThreadsView] preview load failed:', e));
		}
	});

	// A filter shows other rows, so a pin from the last one lets go.
	$effect(() => {
		void filter;
		pinnedId = null;
		hoveredId = null;
	});

	function step(by: number) {
		if (rows.length === 0) return;
		const from = shownIndex < 0 ? 0 : shownIndex;
		const next = rows[(from + by + rows.length) % rows.length];
		pinnedId = next.id;
		document.getElementById(`thread-row-${next.id}`)?.scrollIntoView({ block: 'nearest' });
	}

	function onKey(e: KeyboardEvent) {
		if (!active || view !== 'threads' || e.metaKey || e.ctrlKey || e.altKey) return;
		const target = e.target as HTMLElement | null;
		if (target?.closest('input, textarea, [contenteditable="true"], [role="menu"]')) return;
		if (e.key === 'j' || e.key === 'ArrowDown') {
			e.preventDefault();
			step(1);
		} else if (e.key === 'k' || e.key === 'ArrowUp') {
			e.preventDefault();
			step(-1);
		} else if (e.key === 'Enter' && shown) {
			e.preventDefault();
			openProject(shown);
		} else if (e.key === 'Escape' && pinnedId) {
			pinnedId = null;
		}
	}

	function rowMenu(p: ProjectSummary, e: MouseEvent) {
		e.preventDefault();
		contextMenu.show({ x: e.clientX, y: e.clientY }, projectRowMenuItems(p));
	}

	// ── dropping onto a row ────────────────────────────────────────────────
	let dropTarget = $state<string | null>(null);

	function statusOf(p: ProjectSummary): string {
		if (p.archived_at) return outcomeLabel(p);
		return p.is_backlog ? 'Backlog' : 'Active';
	}

	function dayOf(iso: string): string {
		const d = new Date(iso);
		const sameYear = d.getFullYear() === new Date().getFullYear();
		return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: sameYear ? undefined : 'numeric' });
	}

	/** Its next step: the first open task with a date, else the first one. */
	function nextTask(tasks: Task[]): Task | null {
		return tasks.find((t) => t.due_on) ?? tasks[0] ?? null;
	}

	/**
	 * The thread as a line of dots, from the day it began to today: one dot
	 * per chat, page, file or subject in it, placed on the day it arrived.
	 */
	const nodes = $derived.by<number[]>(() => {
		if (!detail) return [];
		const start = new Date(detail.created_at).getTime();
		const end = Date.now();
		const span = Math.max(end - start, 1);
		const stamps = [
			...detail.items.filter((i) => !i.url.startsWith('/chat/')).map((i) => i.added_at),
			...(detail.chats ?? []).map((c) => c.last_message_at),
		];
		return stamps
			.map((s) => new Date(s).getTime())
			.filter((t) => !Number.isNaN(t))
			.map((t) => Math.min(100, Math.max(0, ((t - start) / span) * 100)));
	});

	const tasksHere = $derived([
		...(detail?.tasks ?? []).map((t) => ({ task: t, also: false })),
		...(detail?.tasks_also_about ?? []).map((t) => ({ task: t, also: true })),
	]);
</script>

<svelte:window onkeydown={onKey} />

<div class="door" class:with-preview={view === 'threads'}>
	<div class="main">
		<Page title="Threads" maxWidth="prose">
			{#snippet actions()}
				<Button variant="secondary" size="sm" icon="mynaui:plus" onclick={() => newProject()}>New thread</Button>
			{/snippet}

			<div class="views" role="tablist" aria-label="Views">
				<button class="vt" role="tab" aria-selected={view === 'tasks'} onclick={() => setView('tasks')}>Tasks</button>
				<button class="vt" role="tab" aria-selected={view === 'threads'} onclick={() => setView('threads')}>Threads</button>
			</div>

			{#if view === 'tasks'}
				<TaskList {active} route={tab.route} />
			{:else}
				<div class="filters" role="group" aria-label="Show">
					{#each FILTERS as f (f.id)}
						<button class="vt" aria-pressed={filter === f.id} onclick={() => (filter = f.id)}>{f.label}</button>
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
						{:else if filter === 'backlog'}
							Threads you mean to come back to wait here.
						{:else}
							Threads you finish or drop wait here.
						{/if}
					</p>
				{:else}
					<div class="rows" role="listbox" tabindex="-1" aria-label="Threads" onmouseleave={() => (hoveredId = null)}>
						{#each rows as p (p.id)}
							<div
								id={`thread-row-${p.id}`}
								class="row"
								class:shown={p.id === shownId}
								class:drop-on={dropTarget === p.id}
								role="option"
								tabindex="0"
								aria-selected={p.id === pinnedId}
								onmouseenter={() => (hoveredId = p.id)}
								onfocus={() => (pinnedId = p.id)}
								onclick={() => (pinnedId = p.id)}
								ondblclick={() => openProject(p)}
								onkeydown={(e) => {
									if (e.key === 'Enter') openProject(p);
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
								<span class="glyph"><ProjectGlyph project={p} size={16} /></span>
								<span class="text">
									<span class="name">{p.name || 'Untitled thread'}</span>
									{#if p.current_status || p.instructions}
										<span class="line">{p.current_status || p.instructions}</span>
									{/if}
								</span>
								<span class="when">
									{p.archived_at ? `${outcomeLabel(p)} ${dayOf(p.archived_at)}` : formatRelativeTimestamp(p.updated_at)}
								</span>
							</div>
						{/each}
					</div>
				{/if}
			{/if}
		</Page>
	</div>

	{#if view === 'threads'}
		<aside class="preview" aria-live="polite" aria-label="Preview">
			{#if shown}
				<div class="ph">
					<span class="kind">Thread</span>
					<span class="pager">
						<span class="count">{shownIndex + 1} of {rows.length}</span>
						<IconButton icon="mynaui:chevron-up" label="Previous thread" size="sm" onclick={() => step(-1)} />
						<IconButton icon="mynaui:chevron-down" label="Next thread" size="sm" onclick={() => step(1)} />
						<Button variant="secondary" size="sm" onclick={() => openProject(shown)}>Open</Button>
					</span>
				</div>

				<div class="title">
					<ProjectGlyph project={shown} size={20} />
					<h2>{shown.name || 'Untitled thread'}</h2>
				</div>
				{#if shown.current_status}
					<p class="stands">{shown.current_status}</p>
				{:else if shown.instructions}
					<p class="stands">{shown.instructions}</p>
				{/if}

				{#if detail}
					<div class="sec">Since it began</div>
					<div class="line-of-dots" aria-hidden="true">
						<span class="rule"></span>
						{#each nodes as x, i (i)}<span class="node" style={`left: ${x}%`}></span>{/each}
						<span class="lab start">{dayOf(detail.created_at)}</span>
						<span class="lab end">Today</span>
					</div>

					<dl class="props">
						<dt>Status</dt>
						<dd>{statusOf(shown)}</dd>
						<dt>Started</dt>
						<dd>{dayOf(shown.created_at)}</dd>
						{#if shown.thing_title}
							<dt>Under</dt>
							<dd>{shown.thing_title}</dd>
						{/if}
						{#if nextTask(detail.tasks ?? [])}
							<dt>Next</dt>
							<dd>{nextTask(detail.tasks ?? [])?.title}</dd>
						{/if}
						<dt>Adding</dt>
						<dd>{shown.is_adding_automatic ? 'Automatic' : 'Manual'}</dd>
					</dl>

					{#if tasksHere.length > 0}
						<div class="sec">Tasks here</div>
						<ul class="tasks">
							{#each tasksHere.slice(0, 8) as { task, also } (task.id)}
								<li>
									<button
										class="task-row"
										onclick={() => windowShellStore.openTabFromRoute(`/task/${task.id}`, { label: task.title, focusExisting: true })}
									>
										<span class="ring" aria-hidden="true"></span>
										<span class="task-title">{task.title}</span>
										{#if also}<span class="also">Also about it</span>{/if}
									</button>
								</li>
							{/each}
						</ul>
					{/if}
				{/if}

				<div class="pf">
					<span><kbd>↵</kbd> Open</span>
					<span><kbd>J</kbd> <kbd>K</kbd> Move</span>
				</div>
			{:else if !firstLoad}
				<div class="ph"><span class="kind">Preview</span></div>
				<p class="quiet">Point at a thread to see it here.</p>
			{/if}
		</aside>
	{/if}
</div>

<style>
	.door {
		display: flex;
		height: 100%;
		min-height: 0;
	}
	.main {
		flex: 1 1 auto;
		min-width: 0;
		height: 100%;
	}

	.views,
	.filters {
		display: flex;
		gap: 4px;
	}
	.filters {
		margin: 24px 0 8px;
	}
	.vt {
		height: 28px;
		padding: 0 12px;
		border: 0;
		border-radius: var(--radius-md);
		background: transparent;
		font: inherit;
		font-size: 13px;
		color: var(--color-foreground-muted);
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.vt:hover {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}
	.vt[aria-pressed='true'],
	.vt[aria-selected='true'] {
		background: var(--active-bg);
		color: var(--color-foreground);
	}
	.vt:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
	}
	.views {
		margin-bottom: 16px;
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
	.row:hover,
	.row.shown {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}
	.row[aria-selected='true'] {
		background: var(--active-bg);
	}
	.row.drop-on {
		background: var(--drop-tint);
	}
	.row:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
	}
	.row.placeholder {
		cursor: default;
		background: transparent;
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
		font-size: 16px;
		line-height: 20px;
		color: var(--color-foreground);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.line {
		font-size: 13px;
		line-height: 20px;
		color: var(--color-foreground-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.when {
		flex: none;
		font-size: 13px;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}
	.empty,
	.quiet {
		margin: 16px 0 0;
		font-size: 15px;
		line-height: 24px;
		color: var(--color-foreground-muted);
		max-width: 520px;
	}

	/* The preview: a full-height column in the desk tone that stays put
	   while the list scrolls. */
	.preview {
		flex: none;
		width: 320px;
		height: 100%;
		box-sizing: border-box;
		padding: 0 24px 64px;
		background: var(--desk);
		overflow-y: auto;
		position: relative;
	}
	.ph {
		display: flex;
		align-items: center;
		justify-content: space-between;
		height: 48px;
		gap: 8px;
	}
	.kind,
	.count {
		font-size: 13px;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
	}
	.pager {
		display: inline-flex;
		align-items: center;
		gap: 4px;
	}
	.count {
		margin-right: 4px;
	}
	.title {
		display: flex;
		align-items: center;
		gap: 12px;
		margin-top: 16px;
	}
	.title h2 {
		margin: 0;
		font-family: var(--font-serif-ui);
		font-weight: 400;
		font-size: var(--md-h2-size);
		line-height: 28px;
		color: var(--color-foreground);
	}
	.stands {
		margin: 8px 0 0;
		font-family: var(--font-serif);
		font-size: 16px;
		line-height: 24px;
		color: var(--color-foreground);
	}
	.sec {
		margin: 24px 0 8px;
		font-size: 13px;
		line-height: 16px;
		color: var(--color-foreground-muted);
	}
	.line-of-dots {
		position: relative;
		height: 40px;
	}
	.line-of-dots .rule {
		position: absolute;
		left: 0;
		right: 0;
		top: 12px;
		height: 1px;
		background: var(--color-border);
	}
	.node {
		position: absolute;
		top: 12px;
		width: 6px;
		height: 6px;
		transform: translate(-50%, -50%);
		border-radius: 50%;
		background: var(--color-foreground);
	}
	.lab {
		position: absolute;
		top: 20px;
		font-size: 12px;
		line-height: 16px;
		color: var(--color-foreground-muted);
		white-space: nowrap;
	}
	.lab.start {
		left: 0;
	}
	.lab.end {
		right: 0;
	}
	.props {
		display: grid;
		grid-template-columns: 64px minmax(0, 1fr);
		gap: 8px 12px;
		margin: 16px 0 0;
		align-items: baseline;
	}
	.props dt {
		font-size: 13px;
		line-height: 20px;
		color: var(--color-foreground-muted);
	}
	.props dd {
		margin: 0;
		font-size: 14px;
		line-height: 20px;
		color: var(--color-foreground);
		overflow-wrap: anywhere;
	}
	.tasks {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	.task-row {
		width: 100%;
		margin: 0 -8px;
		padding: 0 8px;
		border: 0;
		border-radius: var(--radius-md);
		background: transparent;
		font: inherit;
		text-align: left;
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.task-row:hover {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}
	.tasks li,
	.task-row {
		display: flex;
		align-items: baseline;
		gap: 8px;
		padding: 4px 0;
		font-size: 14px;
		line-height: 20px;
		color: var(--color-foreground);
	}
	.ring {
		flex: none;
		width: 12px;
		height: 12px;
		border-radius: 50%;
		border: 1px solid var(--color-foreground-muted);
		box-sizing: border-box;
		transform: translateY(2px);
	}
	.task-title {
		min-width: 0;
		flex: 1 1 auto;
	}
	.also {
		flex: none;
		font-size: 12px;
		color: var(--color-foreground-muted);
	}
	.pf {
		display: flex;
		justify-content: space-between;
		margin-top: 32px;
		font-size: 12px;
		color: var(--color-foreground-muted);
	}
	kbd {
		display: inline-grid;
		place-items: center;
		min-width: 20px;
		height: 20px;
		padding: 0 4px;
		box-sizing: border-box;
		border-radius: var(--radius-sm);
		background: var(--hover-bg);
		font: inherit;
		font-size: 12px;
		color: var(--color-foreground-muted);
	}

	/* A narrow pane has no room beside the list. */
	@media (max-width: 1100px) {
		.preview {
			display: none;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.row,
		.vt {
			transition: none;
		}
	}
</style>
