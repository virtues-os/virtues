<script lang="ts">
	/**
	 * The Threads door: what you follow and the steps in it.
	 *
	 *   Overview  the next five tasks, then the active threads as a week of
	 *             dots (WeekChart).
	 *   Tasks     every open task, whatever its home (TaskList). The add row
	 *             is what "New task" and ⌘⇧E open.
	 *   Threads   Active (the week), Backlog and Finished (rows with where
	 *             each stands).
	 *
	 * Pointing at a thread shows it in the preview column; a click keeps it
	 * there, and opening it goes to its page. A thread's row takes a dropped
	 * chat, page or pin. The view is in the route (`?view=tasks`,
	 * `?view=threads`), so a pin or a link can open any of them.
	 */
	import type { Tab } from '$lib/tabs/types';
	import type { ProjectSummary, Task, Week } from '$lib/api/client';
	import { getThreadWeek } from '$lib/api/client';
	import WeekChart from '$lib/components/threads/WeekChart.svelte';
	import { tasksStore } from '$lib/stores/tasks.svelte';
	import { TextAction } from '$lib';
	import Icon from '$lib/components/Icon.svelte';
	import { projectColor } from '$lib/sidebar/pin-colors';
	import { THREAD_ICON } from '$lib/utils/iconHelpers';
	import { homeOf, subjectName } from '$lib/tasks/homes';
	import ProjectGlyph from '$lib/components/ProjectGlyph.svelte';
	import TaskList from '$lib/components/tasks/TaskList.svelte';
	import TaskPreview from '$lib/components/tasks/TaskPreview.svelte';
	import { newTaskDialog } from '$lib/stores/newTask.svelte';
	import { notesStore } from '$lib/stores/notes.svelte';
	import type { Note } from '$lib/api/client';
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
	import { renameRef } from '$lib/refs/identity.svelte';
	import { onMount } from 'svelte';

	let { tab, active = false }: { tab: Tab; active?: boolean } = $props();

	type View = 'overview' | 'tasks' | 'threads' | 'insights';
	type Filter = 'active' | 'backlog' | 'finished';

	const params = $derived(new URLSearchParams(tab.route.split('?')[1] ?? ''));
	const view = $derived.by<View>(() => {
		const v = params.get('view');
		return v === 'tasks' || v === 'threads' || v === 'insights' ? v : 'overview';
	});

	function setView(next: View) {
		const query = next === 'overview' ? '' : `?view=${next}`;
		windowShellStore.updateTab(tab.id, { route: `/threads${query}` });
	}

	onMount(() => {
		projectStore.load();
		void tasksStore.load();
		notesStore.start();
		return () => notesStore.stop();
	});

	function subjectLabel(url: string): string {
		return url.startsWith('/task/') ? 'Task' : 'Thread';
	}

	function runNote(n: Note) {
		windowShellStore.openTabFromRoute(n.action_route, { focusExisting: true });
	}

	// ── the week ───────────────────────────────────────────────────────────
	let weekStart = $state<string | null>(null);
	let week = $state<Week | null>(null);
	let weekError = $state(false);

	$effect(() => {
		if (view === 'tasks') return;
		const start = weekStart;
		void projectStore.projects.length;
		getThreadWeek(start ?? undefined)
			.then((w) => {
				week = w;
				weekError = false;
			})
			.catch((e) => {
				console.error('[ThreadsView] week failed:', e);
				weekError = true;
			});
	});

	function shiftWeek(days: number) {
		if (!week) return;
		const d = new Date(`${week.start}T12:00:00`);
		d.setDate(d.getDate() + days);
		const m = String(d.getMonth() + 1).padStart(2, '0');
		weekStart = `${d.getFullYear()}-${m}-${String(d.getDate()).padStart(2, '0')}`;
	}

	function hours(minutes: number): string {
		const h = Math.floor(minutes / 60);
		const m = minutes % 60;
		if (h === 0) return `${m} m`;
		return m === 0 ? `${h} h` : `${h} h ${m} m`;
	}

	/** "6 h spent, 6 h 30 m ahead", saying only what is there. */
	function weekLine(id: string): string | null {
		const t = week?.threads.find((x) => x.id === id);
		if (!t) return null;
		const done = t.days.reduce((n, d) => n + d.done_minutes, 0);
		const ahead = t.days.reduce((n, d) => n + d.ahead_minutes, 0);
		const parts = [done > 0 ? `${hours(done)} spent` : null, ahead > 0 ? `${hours(ahead)} ahead` : null].filter(Boolean);
		return parts.length ? parts.join(', ') : null;
	}

	// ── the overview's tasks ───────────────────────────────────────────────
	const nextTasks = $derived(
		tasksStore.open
			.filter((t) => t.assignee === 'owner')
			.sort((a, b) => (a.due_on ?? '~').localeCompare(b.due_on ?? '~') || a.sort_order - b.sort_order)
			.slice(0, 5),
	);

	function newTaskHere() {
		newTaskDialog.open();
	}

	/** On the Overview the preview shows whichever kind of row was pointed at last. */
	let lastKind = $state<'thread' | 'task'>('thread');

	// ── the Tasks view's preview ───────────────────────────────────────────
	let hoveredTaskId = $state<string | null>(null);
	let pinnedTaskId = $state<string | null>(null);
	const shownTask = $derived.by(() => {
		const id = pinnedTaskId ?? hoveredTaskId;
		const picked = id ? tasksStore.find(id) : undefined;
		if (picked) return picked;
		return tasksStore.open.find((t) => t.assignee === 'owner') ?? tasksStore.open[0] ?? null;
	});

	function openById(id: string) {
		const p = rows.find((r) => r.id === id);
		if (p) openProject(p);
	}

	async function dropOnThread(id: string, e: DragEvent) {
		const p = rows.find((r) => r.id === id);
		const url = droppedRefUrl(e);
		if (p && url) await fileIntoProject(p, url);
	}

	let filter = $state<Filter>('active');
	const FILTERS: { id: Filter; label: string }[] = [
		{ id: 'active', label: 'Active' },
		{ id: 'backlog', label: 'Backlog' },
		{ id: 'finished', label: 'Finished' },
	];

	const firstLoad = $derived(!projectStore.loaded && projectStore.loading);
	// The overview shows the active threads, as the Active filter does.
	const listFilter = $derived<Filter>(view === 'overview' ? 'active' : filter);
	const rows = $derived.by<ProjectSummary[]>(() => {
		if (listFilter === 'finished') return projectStore.archived;
		const backlog = listFilter === 'backlog';
		const open = projectStore.projects.filter((p) => !!p.is_backlog === backlog);
		if (backlog || !week) return open;
		// Active follows the week's order: threads with time first, then the
		// quiet ones, so J, K and the pager walk the chart as it is drawn.
		const busy = (id: string) =>
			week?.threads.find((t) => t.id === id)?.days.some((d) => d.done_minutes + d.ahead_minutes >= 8) ?? false;
		return [...open.filter((p) => busy(p.id)), ...open.filter((p) => !busy(p.id))];
	});

	// ── the preview ─────────────────────────────────────────────────────
	// It follows the pointer until a click pins a row; Escape lets go.
	let hoveredId = $state<string | null>(null);
	let pinnedId = $state<string | null>(null);
	/** With nothing pointed at, the preview shows the first thread with time this week. */
	const firstBusy = $derived(
		listFilter === 'active'
			? (week?.threads.find((t) => t.days.some((d) => d.done_minutes + d.ahead_minutes > 0))?.id ?? null)
			: null,
	);
	const shownId = $derived(pinnedId ?? hoveredId ?? firstBusy ?? rows[0]?.id ?? null);
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

	/** G, then O, T or A: the door's views. */
	let gPressedAt = 0;

	function onKey(e: KeyboardEvent) {
		if (!active || e.metaKey || e.ctrlKey || e.altKey || newTaskDialog.isOpen) return;
		const target = e.target as HTMLElement | null;
		if (target?.closest('input, textarea, [contenteditable="true"], [role="menu"]')) return;
		if (e.key === 'g') {
			gPressedAt = Date.now();
			return;
		}
		if (Date.now() - gPressedAt < 1200) {
			const to = ({ o: 'overview', t: 'tasks', a: 'threads', i: 'insights' } as const)[e.key as 'o' | 't' | 'a' | 'i'];
			gPressedAt = 0;
			if (to) {
				e.preventDefault();
				setView(to);
				return;
			}
		}
		if (view === 'overview' && e.key === 'c') {
			e.preventDefault();
			newTaskHere();
			return;
		}
		if (view === 'tasks' || view === 'insights') return;
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
		contextMenu.show({ x: e.clientX, y: e.clientY }, projectRowMenuItems(p, { rename: () => startRename(p) }));
	}

	// ── renaming a row in place ───────────────────────────────────────────
	// Rename (the row's menu) or F2 makes the name a field. A double-click
	// opens the thread here, as it does in any list, so it does not rename.
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
	 * The thread as a line, from the day it began to its last planned step:
	 * a solid dot for each chat, page, file or record on the day it happened,
	 * a ring for each dated task and planned record still ahead, and a tick
	 * at today.
	 */
	const line = $derived.by(() => {
		if (!detail) return null;
		const now = Date.now();
		const at = (iso: string | null | undefined) => (iso ? new Date(iso).getTime() : NaN);
		const past = [
			...detail.items.filter((i) => !i.url.startsWith('/chat/')).map((i) => at(i.occurred_at ?? i.added_at)),
			...(detail.chats ?? []).map((c) => at(c.last_message_at)),
		].filter((t) => !Number.isNaN(t));
		const ahead = [
			...(detail.tasks ?? []).filter((t) => t.due_on).map((t) => at(`${t.due_on}T12:00:00`)),
			...past.filter((t) => t > now),
		].filter((t) => t > now);
		const done = past.filter((t) => t <= now);
		if (done.length + ahead.length === 0) return null;
		const start = Math.min(at(detail.created_at), ...done);
		const end = Math.max(now, ...ahead);
		const span = Math.max(end - start, 1);
		const x = (t: number) => Math.min(100, Math.max(0, ((t - start) / span) * 100));
		return {
			nodes: [...done.map((t) => ({ x: x(t), ahead: false })), ...ahead.map((t) => ({ x: x(t), ahead: true }))],
			todayX: x(now),
			from: dayOf(new Date(start).toISOString()),
			to: ahead.length ? dayOf(new Date(Math.max(...ahead)).toISOString()) : 'Today',
		};
	});

	/** What is next: the first dated task, else the first task. */
	const nextUp = $derived(detail ? nextTask(detail.tasks ?? []) : null);

	/** What is ahead: the soonest planned record, such as a calendar event still to come. */
	const aheadUp = $derived.by(() => {
		if (!detail) return null;
		const now = Date.now();
		const planned = detail.items
			.filter((i) => i.occurred_at && new Date(i.occurred_at).getTime() > now)
			.sort((a, b) => (a.occurred_at ?? '').localeCompare(b.occurred_at ?? ''))[0];
		return planned ? `${planned.title ?? 'Something planned'}, ${dayOf(planned.occurred_at!)}` : null;
	});

	/** A home's name for "lives in …". */
	function livesIn(t: Task): string {
		const home = homeOf(t);
		return home ? subjectName(home) : 'no home yet';
	}

	const tasksHere = $derived([
		...(detail?.tasks ?? []).map((t) => ({ task: t, also: false })),
		...(detail?.tasks_also_about ?? []).map((t) => ({ task: t, also: true })),
	]);
</script>

<svelte:window onkeydown={onKey} />

{#snippet newTaskButton()}
	<Button variant="secondary" size="sm" icon="mynaui:plus" onclick={newTaskHere}>
		New task<kbd class="key">C</kbd>
	</Button>
{/snippet}
{#snippet newThreadButton()}
	<Button variant="secondary" size="sm" icon="mynaui:plus" onclick={() => newProject()}>New thread</Button>
{/snippet}

{#snippet weekBlock()}
	{#if week && week.threads.length > 0}
		<WeekChart
			{week}
			{shownId}
			{pinnedId}
			hoverId={hoveredId}
			onPrev={() => shiftWeek(-7)}
			onNext={() => shiftWeek(7)}
			onHover={(id) => {
				hoveredId = id;
				if (id) lastKind = 'thread';
			}}
			onPick={(id) => {
				pinnedId = id;
				pinnedTaskId = null;
				lastKind = 'thread';
			}}
			onOpen={openById}
			accepts={(data) => data.types.includes(REF_DRAG_TYPE)}
			onDropTo={dropOnThread}
		/>
	{:else if weekError}
		<p class="empty">Your server couldn't load the week. Try again in a moment.</p>
	{:else if week}
		<p class="empty">
			A thread is something happening in your life that you follow. It keeps its tasks, chats, pages and files
			together, and its time shows here week by week.
		</p>
	{/if}
{/snippet}

<div class="door">
	<div class="main">
		<Page title="Threads" maxWidth={view === 'tasks' ? 'reading' : 'full'}>
			{#if view === 'tasks'}
				<div class="hrow">
					<h2 class="h2">Tasks</h2>
					{@render newTaskButton()}
				</div>
				<TaskList
					{active}
					route={tab.route}
					hideNew
					emptyText="No tasks yet. New task adds one."
					shownId={shownTask?.id ?? null}
					pinnedId={pinnedTaskId}
					onPick={(t) => (pinnedTaskId = t.id)}
					onHover={(t) => (hoveredTaskId = t?.id ?? null)}
				/>
			{:else if view === 'overview'}
				<div class="hrow">
					<h2 class="h2">Tasks</h2>
					{@render newTaskButton()}
				</div>
				<TaskList
					{active}
					route={tab.route}
					hideNew
					limit={5}
					keys={false}
					emptyText="No tasks yet."
					shownId={lastKind === 'task' ? (shownTask?.id ?? null) : null}
					pinnedId={pinnedTaskId}
					onPick={(t) => {
						pinnedTaskId = t.id;
						pinnedId = null;
						lastKind = 'task';
					}}
					onHover={(t) => {
						hoveredTaskId = t?.id ?? null;
						if (t) lastKind = 'task';
					}}
				/>
				{#if nextTasks.length > 0}
					<div class="all"><TextAction onclick={() => setView('tasks')}>All tasks</TextAction></div>
				{/if}

				<div class="hrow week-head">
					<h2 class="h2">Active threads</h2>
					{@render newThreadButton()}
				</div>
				{@render weekBlock()}
			{:else if view === 'insights'}
				<div class="hrow">
					<h2 class="h2">Insights</h2>
				</div>
				<p class="lede">
					What your server noticed about your tasks and threads, while you can still act on it. Each note leaves on its
					own once its moment has passed.
				</p>
				{#if notesStore.items.length === 0}
					<p class="empty">Nothing to note right now.</p>
				{:else}
					<ul class="insights">
						{#each notesStore.items as n (n.key)}
							<li class="ins">
								<span class="mark" aria-hidden="true"></span>
								<div class="ins-body">
									<p class="ins-text">{n.text}</p>
									<p class="ins-meta">
										<Icon icon={n.subjects[0]?.startsWith('/task/') ? 'mynaui:circle' : THREAD_ICON} width="14" />
										<span class="ins-about">{n.about}</span>
										<span>{subjectLabel(n.subjects[0] ?? '')}</span>
										<span>{n.when}</span>
									</p>
								</div>
								<div class="ins-acts">
									<Button variant="secondary" size="sm" onclick={() => runNote(n)}>{n.action}</Button>
									<TextAction onclick={() => notesStore.dismiss(n)}>Not now</TextAction>
								</div>
							</li>
						{/each}
					</ul>
				{/if}
				<p class="foot">Home shows these under the new chat, one at a time.</p>
			{:else}
				<div class="hrow">
					<h2 class="h2">All threads</h2>
					{@render newThreadButton()}
				</div>
				<div class="filters" role="group" aria-label="Show">
					{#each FILTERS as f (f.id)}
						<button class="vt" aria-pressed={filter === f.id} onclick={() => (filter = f.id)}>{f.label}</button>
					{/each}
				</div>

				{#if filter === 'active' && week && week.threads.length > 0}
					{@render weekBlock()}
				{:else if firstLoad}
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
								class:renaming={renaming?.id === p.id}
								role="option"
								tabindex="0"
								aria-selected={p.id === pinnedId}
								onmouseenter={() => (hoveredId = p.id)}
								onfocus={() => (pinnedId = p.id)}
								onclick={() => (pinnedId = p.id)}
								ondblclick={() => openProject(p)}
								onkeydown={(e) => {
									if (e.key === 'Enter') openProject(p);
									else if (e.key === 'F2') {
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
								<span class="glyph"><ProjectGlyph project={p} size={16} /></span>
								<span class="text">
									{#if renaming?.id === p.id}
										<!-- The field keeps its keys and clicks: the row's own Enter
										     and double-click would open the thread. -->
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

	{#if view === 'tasks' || (view === 'overview' && lastKind === 'task' && shownTask)}
		<aside class="preview" aria-live="polite" aria-label="Preview">
			{#if shownTask}
				<TaskPreview task={shownTask} />
				<div class="pf">
					<span><kbd>↵</kbd> Open</span>
					<span><kbd>J</kbd> <kbd>K</kbd> Move</span>
				</div>
			{:else}
				<div class="ph"><span class="kind">Preview</span></div>
				<p class="quiet">Point at a task to see it here.</p>
			{/if}
		</aside>
	{:else if view !== 'insights'}
		<aside class="preview" aria-live="polite" aria-label="Preview">
			{#if shown}
				<div class="ph">
					<span class="kind">Thread</span>
					<span class="pager">
						<Button variant="secondary" size="sm" onclick={() => openProject(shown)}>Open</Button>
						<span class="count">{shownIndex + 1} of {rows.length}</span>
						<IconButton icon="mynaui:chevron-up" label="Previous thread" size="sm" onclick={() => step(-1)} />
						<IconButton icon="mynaui:chevron-down" label="Next thread" size="sm" onclick={() => step(1)} />
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
				{#each notesStore.about(`/thread/${shown.id}`).slice(0, 2) as n (n.key)}
					<div class="note">
						<p class="note-text"><span class="mark" aria-hidden="true"></span>{n.text}</p>
						<div class="note-acts">
							<Button variant="secondary" size="sm" onclick={() => runNote(n)}>{n.action}</Button>
							<TextAction onclick={() => notesStore.dismiss(n)}>Not now</TextAction>
						</div>
					</div>
				{/each}

				<div class="chips">
					<span class="chip"><Icon icon="mynaui:circle-half" width="14" />{statusOf(shown)}</span>
					{#if weekLine(shown.id)}
						<span class="chip"><Icon icon="mynaui:clock-circle" width="14" />{weekLine(shown.id)}</span>
					{/if}
					<span class="chip"><Icon icon="mynaui:calendar" width="14" />Started {dayOf(shown.created_at)}</span>
					{#if shown.thing_title}
						<span class="chip"><Icon icon="atlas:things" width="14" />{shown.thing_title}</span>
					{/if}
				</div>

				{#if detail}
					{#if line}
						<div class="sec">Since it began</div>
						<div class="line-of-dots" style={`--thread: ${projectColor(shown)}`} aria-hidden="true">
							<span class="rule"></span>
							{#each line.nodes as n, i (i)}<span class="node" class:ahead={n.ahead} style={`left: ${n.x}%`}></span>{/each}
							<span class="today-tick" style={`left: ${line.todayX}%`}></span>
							<span class="lab start">{line.from}</span>
							<span class="lab end">{line.to}</span>
						</div>
					{/if}

					{#if nextUp || aheadUp}
						<dl class="lines">
							{#if nextUp}<dt>Next</dt><dd>{nextUp.title}</dd>{/if}
							{#if aheadUp}<dt>Ahead</dt><dd>{aheadUp}</dd>{/if}
						</dl>
					{/if}

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
										<span class="task-title">
											{task.title}{#if also}<span class="also"> · lives in {livesIn(task)}</span>{/if}
										</span>
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

	.filters {
		display: flex;
		gap: 4px;
		margin: 12px 0 8px;
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
	.vt[aria-pressed='true'] {
		background: var(--active-bg);
		color: var(--color-foreground);
	}
	.lede {
		margin: 4px 0 0;
		max-width: 620px;
		font-size: 15px;
		line-height: 24px;
		color: var(--color-foreground-muted);
	}
	.insights {
		list-style: none;
		margin: 16px 0 0;
		padding: 0;
	}
	.ins {
		display: flex;
		align-items: flex-start;
		gap: 12px;
		padding: 12px;
		margin: 0 -12px;
		border-radius: var(--radius-md);
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.ins:hover {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}
	.mark {
		display: inline-block;
		flex: none;
		width: 6px;
		height: 6px;
		margin-top: 8px;
		border-radius: 50%;
		background: var(--color-primary);
	}
	.ins-body {
		flex: 1 1 auto;
		min-width: 0;
	}
	.ins-text {
		margin: 0;
		font-size: 14px;
		line-height: 20px;
		color: var(--color-foreground);
	}
	.ins-meta {
		display: flex;
		align-items: center;
		gap: 8px;
		margin: 4px 0 0;
		font-size: 12px;
		color: var(--color-foreground-muted);
	}
	.ins-about {
		font-family: var(--font-serif-ui);
		font-size: 14px;
		color: var(--color-foreground);
	}
	.ins-acts {
		display: flex;
		align-items: center;
		gap: 16px;
		flex: none;
	}
	.foot {
		margin: 16px 0 0;
		font-size: 12px;
		color: var(--color-foreground-muted);
	}
	.note {
		margin-top: 20px;
		padding: 12px 16px;
		border-radius: var(--radius-md);
		background: var(--hover-bg);
	}
	.note-text {
		display: flex;
		gap: 12px;
		margin: 0;
		font-size: 14px;
		line-height: 20px;
		color: var(--color-foreground);
	}
	.note-text .mark {
		margin-top: 8px;
	}
	.note-acts {
		display: flex;
		align-items: center;
		gap: 16px;
		margin: 12px 0 0 16px;
	}
	.key {
		margin-left: 4px;
		min-width: 20px;
		height: 20px;
	}
	.hrow {
		display: flex;
		align-items: center;
		justify-content: space-between;
		min-height: 32px;
		margin-top: 16px;
	}
	.week-head {
		margin-top: 40px;
	}
	.h2 {
		margin: 0;
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: var(--md-h2-size);
		line-height: 28px;
		color: var(--color-foreground);
	}
	.all {
		margin-top: 4px;
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
	.row.renaming {
		cursor: default;
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
	/* Renaming: the name alone becomes the field (app.css), since the
	   status line and date beside it are not edited here. Its padding and
	   height are taken back by equal negative margins, so the name and the
	   lines around it do not move. */
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
		height: 2px;
		transform: translateY(-50%);
		background: var(--thread);
		opacity: 0.35;
	}
	.today-tick {
		position: absolute;
		top: 4px;
		width: 2px;
		height: 16px;
		transform: translateX(-50%);
		background: var(--color-foreground);
	}
	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
		margin-top: 16px;
	}
	.chip {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		height: 28px;
		padding: 0 12px;
		border-radius: var(--radius-md);
		background: var(--hover-bg);
		font-size: 12px;
		color: var(--color-foreground);
		white-space: nowrap;
		max-width: 100%;
		overflow: hidden;
		text-overflow: ellipsis;
		box-sizing: border-box;
	}
	.chip :global(svg) {
		flex: none;
		color: var(--color-foreground-muted);
	}
	.lines {
		display: grid;
		grid-template-columns: 56px minmax(0, 1fr);
		gap: 8px 12px;
		margin: 20px 0 0;
		align-items: baseline;
	}
	.lines dt {
		font-size: 12px;
		line-height: 20px;
		color: var(--color-foreground-muted);
	}
	.lines dd {
		margin: 0;
		font-size: 14px;
		line-height: 20px;
		color: var(--color-foreground);
	}
	.node {
		position: absolute;
		top: 12px;
		width: 6px;
		height: 6px;
		transform: translate(-50%, -50%);
		border-radius: 50%;
		background: var(--thread);
	}
	.node.ahead {
		background: var(--desk);
		border: 1px solid var(--thread);
		box-sizing: border-box;
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
