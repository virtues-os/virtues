<script lang="ts">
	/**
	 * The Threads door: what you follow and the steps in it.
	 *
	 *   Overview  the next five tasks, then the active threads as a week of
	 *             dots (WeekChart).
	 *   Tasks     every open task, whatever its home (TaskList).
	 *   Threads   Active (the week), Backlog and Finished (rows with where
	 *             each stands).
	 *   Insights  what your server noticed.
	 *
	 * The views are named across the top. Overview and Threads are the wide
	 * page with a preview column: pointing at a row shows it there, a click
	 * opens it, J and K walk the threads and Enter opens the one shown.
	 * Tasks and Insights are an inbox: the list, and the chosen item whole
	 * beside it. A thread's row takes a dropped chat, page or pin. The view is in the route
	 * (`?view=tasks`, `?view=threads`), so a pin or a link can open any of
	 * them.
	 */
	import type { Tab } from '$lib/tabs/types';
	import type { ProjectSummary, Week } from '$lib/api/client';
	import { getNotes, getThreadWeek } from '$lib/api/client';
	import WeekChart, { BUSY_MIN } from '$lib/components/threads/WeekChart.svelte';
	import { tasksStore } from '$lib/stores/tasks.svelte';
	import { openInsight } from '$lib/components/threads/NoteBox.svelte';
	import ThreadPreview from '$lib/components/threads/ThreadPreview.svelte';
	import { fly } from '$lib/utils/transitions';
	import Icon from '$lib/components/Icon.svelte';
	import { THREAD_ICON } from '$lib/utils/iconHelpers';
	import ProjectGlyph from '$lib/components/ProjectGlyph.svelte';
	import TaskList from '$lib/components/tasks/TaskList.svelte';
	import TaskPreview from '$lib/components/tasks/TaskPreview.svelte';
	import { newTaskDialog } from '$lib/stores/newTask.svelte';
	import { dialogStore, promptText } from '$lib/stores/dialog.svelte';
	import { newThreadDialog } from '$lib/stores/newThread.svelte';
	import { notesStore } from '$lib/stores/notes.svelte';
	import type { Note, Task } from '$lib/api/client';
	import TaskDetail from '$lib/components/tasks/TaskDetail.svelte';
	import InsightDetail from '$lib/components/threads/InsightDetail.svelte';
	import { Button, IconButton, Page, TextAction } from '$lib';
	import SubNav, { type SubNavItem } from '$lib/components/SubNav.svelte';
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
	import { onMount, untrack } from 'svelte';

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
		return () => {
			notesStore.stop();
			clearTimeout(restTimer);
		};
	});

	// ── the week ───────────────────────────────────────────────────────────
	let weekStart = $state<string | null>(null);
	let week = $state<Week | null>(null);
	let weekError = $state(false);
	/** Bumped by Try again, to fetch the week again. */
	let weekTry = $state(0);

	$effect(() => {
		if (view === 'tasks' || view === 'insights') return;
		const start = weekStart;
		void projectStore.projects.length;
		void weekTry;
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

	// ── the overview's tasks ───────────────────────────────────────────────
	/** A new task starts in the thread you picked with J or K, if any. */
	function newTaskHere() {
		const thread = view === 'overview' && lastKind === 'thread' ? pinnedId : null;
		newTaskDialog.open(thread ? { home: { kind: 'thread', id: thread } } : undefined);
	}

	const VIEWS: SubNavItem[] = [
		{ id: 'overview', label: 'Overview' },
		{ id: 'tasks', label: 'Tasks' },
		{ id: 'threads', label: 'All threads' },
		{ id: 'insights', label: 'Insights' },
	];

	// ── the inbox: Tasks and Insights ──────────────────────────────────────
	// The list on the left and the chosen item whole on the right, as a mail
	// app shows a message. A click chooses; J and K move; Enter opens the
	// item's own page. A narrow pane has no room for the right side, so there
	// a click opens the page.
	const inbox = $derived(view === 'tasks' || view === 'insights');
	/** The door's own width, so a split pane in a wide window counts as narrow. */
	let doorWidth = $state(0);
	const wide = $derived(doorWidth === 0 || doorWidth >= 960);
	/** Escape lets go: the reader shows nothing until you choose again. */
	let released = $state(false);

	let chosenNoteKey = $state<string | null>(null);

	// Now, or the ones you set aside, to find one again and bring it back.
	// The set-aside list is read again whenever the main list reloads, so a
	// note brought back leaves it.
	let insightFilter = $state<'now' | 'aside'>('now');
	let setAsideNotes = $state<Note[]>([]);
	let setAsideState = $state<'loading' | 'ready' | 'failed'>('loading');
	$effect(() => {
		if (view !== 'insights' || insightFilter !== 'aside') return;
		void notesStore.revision;
		getNotes({ setAside: true })
			.then((list) => {
				setAsideNotes = list;
				setAsideState = 'ready';
			})
			.catch((e) => {
				console.error('[ThreadsView] set-aside insights failed:', e);
				setAsideState = 'failed';
			});
	});
	const insightItems = $derived(insightFilter === 'aside' ? setAsideNotes : notesStore.items);
	const insightsFailed = $derived(insightFilter === 'aside' ? setAsideState === 'failed' : notesStore.failed);
	const insightsLoading = $derived(insightFilter === 'aside' ? setAsideState === 'loading' : !notesStore.loaded);
	function setInsightFilter(f: 'now' | 'aside') {
		insightFilter = f;
		chosenNoteKey = null;
	}

	const shownNote = $derived(insightItems.find((n) => n.key === chosenNoteKey) ?? insightItems[0] ?? null);
	function chooseNote(n: Note) {
		released = false;
		if (wide) chosenNoteKey = n.key;
		else openInsight(n);
	}
	function stepNote(by: number) {
		const items = insightItems;
		if (items.length === 0) return;
		const from = shownNote ? items.indexOf(shownNote) : 0;
		const next = items[(from + by + items.length) % items.length];
		chosenNoteKey = next.key;
		released = false;
		document.getElementById(`note-row-${CSS.escape(next.key)}`)?.scrollIntoView({ block: 'nearest' });
	}

	/** How the Tasks view groups its to-dos, kept for next time on this device. */
	const GROUP_KEY = 'virtues.threads.tasks.groupBy';
	let taskGroup = $state<'day' | 'thread'>('day');
	onMount(() => {
		try {
			if (localStorage.getItem(GROUP_KEY) === 'thread') taskGroup = 'thread';
		} catch {
			/* no storage: by day */
		}
	});
	function setTaskGroup(g: 'day' | 'thread') {
		taskGroup = g;
		try {
			localStorage.setItem(GROUP_KEY, g);
		} catch {
			/* kept for this visit only */
		}
	}

	function chooseTask(t: Task) {
		pinnedTaskId = t.id;
		released = false;
	}


	// A choice made in one view does not follow you into another, however
	// the view changed (the bar, the path's menu, G and a letter).
	$effect(() => {
		void view;
		untrack(() => {
			pinnedTaskId = null;
			chosenNoteKey = null;
			released = false;
		});
	});


	/** On the Overview the preview shows whichever kind of row was pointed at last. */
	let lastKind = $state<'thread' | 'task'>('thread');

	// ── the Tasks view's preview ───────────────────────────────────────────
	let restTaskId = $state<string | null>(null);
	let pinnedTaskId = $state<string | null>(null);
	const shownTask = $derived.by(() => {
		const id = pinnedTaskId ?? restTaskId;
		const picked = id ? tasksStore.find(id) : undefined;
		if (picked) return picked;
		return tasksStore.open.find((t) => t.assignee === 'owner') ?? tasksStore.open[0] ?? null;
	});
	/** What the inbox's reader shows: the choice, unless Escape let go of it. */
	const readerTask = $derived(released ? null : shownTask);
	const readerNote = $derived(released ? null : shownNote);

	/** The reader starts at the top for each item, and stays put while the item changes. */
	let readerEl = $state<HTMLElement | null>(null);
	const readerKey = $derived(view === 'tasks' ? readerTask?.id : readerNote?.key);
	$effect(() => {
		void readerKey;
		untrack(() => readerEl?.scrollTo({ top: 0 }));
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
			(week?.threads.find((t) => t.id === id)?.days.reduce((n, d) => n + d.done_minutes + d.ahead_minutes, 0) ?? 0) >= BUSY_MIN;
		return [...open.filter((p) => busy(p.id)), ...open.filter((p) => !busy(p.id))];
	});

	// ── the preview ─────────────────────────────────────────────────────
	// It follows the pointer until a click pins a row; Escape lets go.
	let hoveredId = $state<string | null>(null);
	let pinnedId = $state<string | null>(null);

	/* The preview changes once the pointer rests on a row for a moment, so
	   running it down the list does not flash every row past. It keeps the
	   last row rested on after the pointer leaves. A click, J or K changes
	   it at once. */
	let restId = $state<string | null>(null);
	let restTimer: ReturnType<typeof setTimeout> | undefined;
	const REST_MS = 80;

	function rest(kind: 'thread' | 'task', id: string | null) {
		clearTimeout(restTimer);
		if (!id) return;
		restTimer = setTimeout(() => {
			if (kind === 'thread') restId = id;
			else restTaskId = id;
			lastKind = kind;
		}, REST_MS);
	}

	function hoverThread(id: string | null) {
		hoveredId = id;
		rest('thread', id);
	}
	/** With nothing pointed at, the preview shows the first thread with time this week. */
	const firstBusy = $derived(
		listFilter === 'active'
			? (week?.threads.find((t) => t.days.reduce((n, d) => n + d.done_minutes + d.ahead_minutes, 0) >= BUSY_MIN)?.id ?? null)
			: null,
	);
	/** A pin or a rest on a row that has since left the list (finished, filtered away) no longer counts. */
	const inRows = (id: string | null) => (id && rows.some((r) => r.id === id) ? id : null);
	const shownId = $derived(inRows(pinnedId) ?? inRows(restId) ?? firstBusy ?? rows[0]?.id ?? null);
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
		restId = null;
	});

	function step(by: number) {
		if (rows.length === 0) return;
		const from = shownIndex < 0 ? 0 : shownIndex;
		const next = rows[(from + by + rows.length) % rows.length];
		pinnedId = next.id;
		// J and K walk the threads, so the preview turns to the thread.
		lastKind = 'thread';
		pinnedTaskId = null;
		const el = document.getElementById(`thread-row-${next.id}`);
		el?.scrollIntoView({ block: 'nearest' });
		// Focus moves with the choice when it was in the list, so Tab and the
		// keys agree on where you are.
		if (document.activeElement?.closest('[role="listbox"], [role="grid"]')) {
			(el ?? document.querySelector<HTMLElement>(`[data-thread-id="${next.id}"]`))?.focus();
		}
	}

	/** G, then O, T or A: the door's views. */
	let gPressedAt = 0;

	function onKey(e: KeyboardEvent) {
		if (!active || e.defaultPrevented || e.metaKey || e.ctrlKey || e.altKey || e.repeat) return;
		// Enter on a focused button or link is that control's, not the list's.
		if (e.key === 'Enter' && e.target instanceof Element && e.target.closest('button, a, [role="dialog"]')) return;
		if (newTaskDialog.isOpen || newThreadDialog.isOpen || dialogStore.pending) return;
		const target = e.target;
		if (target instanceof Element && target.closest('input, textarea, [contenteditable="true"], [role="menu"]')) return;
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
		// C makes what the view lists: a task on the Overview, a thread on
		// Threads. Shift-C makes a thread from either.
		if ((view === 'overview' && e.key === 'C') || (view === 'threads' && (e.key === 'c' || e.key === 'C'))) {
			e.preventDefault();
			void newProject();
			return;
		}
		if (view === 'overview' && e.key === 'c') {
			e.preventDefault();
			newTaskHere();
			return;
		}
		if (e.key === 'Escape' && inbox && wide && !released) {
			released = true;
			return;
		}
		if (e.key === 'Escape' && (pinnedId || pinnedTaskId)) {
			pinnedId = null;
			pinnedTaskId = null;
			return;
		}
		if (view === 'tasks') return;
		if (view === 'insights') {
			if (e.key === 'j' || e.key === 'ArrowDown') {
				e.preventDefault();
				stepNote(1);
			} else if (e.key === 'k' || e.key === 'ArrowUp') {
				e.preventDefault();
				stepNote(-1);
			} else if (e.key === 'Enter' && readerNote) {
				e.preventDefault();
				openInsight(readerNote);
			}
			return;
		}
		if (e.key === 'j' || e.key === 'ArrowDown') {
			e.preventDefault();
			step(1);
		} else if (e.key === 'k' || e.key === 'ArrowUp') {
			e.preventDefault();
			step(-1);
		} else if (e.key === 'Enter' && view === 'overview' && lastKind === 'task' && shownTask) {
			// Enter opens what the preview shows, a task included.
			e.preventDefault();
			windowShellStore.openTabFromRoute(`/task/${shownTask.id}`, { label: shownTask.title, focusExisting: true });
		} else if (e.key === 'Enter' && shown) {
			e.preventDefault();
			openProject(shown);
		}
	}

	/** A thread's menu. A list row renames in place; the week chart has no field, so it asks. */
	function rowMenu(p: ProjectSummary, e: MouseEvent, rename = () => startRename(p)) {
		e.preventDefault();
		contextMenu.show({ x: e.clientX, y: e.clientY }, projectRowMenuItems(p, { rename }));
	}

	async function renameByPrompt(p: ProjectSummary) {
		const name = (
			await promptText({ title: 'Rename thread', initialValue: p.name, placeholder: 'Untitled thread', confirmLabel: 'Rename' })
		)?.trim();
		if (name && name !== p.name) await renameRef(`/thread/${p.id}`, name);
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

	// ── dropping onto a row ────────────────────────────────────────────────
	let dropTarget = $state<string | null>(null);

	function dayOf(iso: string): string {
		const d = new Date(iso);
		const sameYear = d.getFullYear() === new Date().getFullYear();
		return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: sameYear ? undefined : 'numeric' });
	}
</script>

<svelte:window onkeydown={onKey} />

<!-- The shortcut is in the tip, so the page at rest shows only the words. -->
{#snippet newTaskButton()}
	<Button variant="secondary" icon="mynaui:plus" onclick={newTaskHere} tip="New task" tipKey="c">New task</Button>
{/snippet}
{#snippet newThreadButton()}
	<Button variant="secondary" icon="mynaui:plus" onclick={() => newProject()} tip="New thread" tipKey={view === 'threads' ? 'c' : 'shift+c'}>
		New thread
	</Button>
{/snippet}

{#snippet weekBlock()}
	{#if week && week.threads.length > 0}
		<WeekChart
			{week}
			shownId={view === 'overview' && lastKind === 'task' ? null : shownId}
			{pinnedId}
			hoverId={hoveredId}
			onPrev={() => shiftWeek(-7)}
			onNext={() => shiftWeek(7)}
			onHover={hoverThread}
			onOpen={openById}
			onMenu={(id, e) => {
				const p = rows.find((r) => r.id === id);
				if (p) rowMenu(p, e, () => renameByPrompt(p));
			}}
			accepts={(data) => data.types.includes(REF_DRAG_TYPE)}
			onDropTo={dropOnThread}
		/>
	{:else if weekError}
		<p class="empty">
			Your server couldn't load the week. <TextAction inline onclick={() => (weekTry += 1)}>Try again</TextAction>
		</p>
	{:else if !week}
		<div class="rows" aria-hidden="true">
			{#each [0, 1, 2] as i (i)}<div class="row placeholder"></div>{/each}
		</div>
	{:else}
		<p class="empty">
			A thread is something happening in your life that you follow. It keeps its tasks, chats, pages and files
			together, and its time shows here week by week.
		</p>
	{/if}
{/snippet}

<div class="door" class:inbox={inbox && wide} bind:clientWidth={doorWidth}>
	<div class="main">
		<Page title="Threads" maxWidth={inbox && wide ? 'none' : 'full'} class={inbox && wide ? 'md:px-8' : ''}>
			<div class="viewbar">
				<SubNav
					tabId={tab.id}
					route={tab.route}
					base="/threads"
					default="overview"
					items={VIEWS}
					selected={view}
					onSelect={(id) => setView(id as View)}
					insetX="0"
					ariaLabel="Views"
				/>
				{#if view === 'tasks'}
					{@render newTaskButton()}
				{:else if view === 'threads'}
					{@render newThreadButton()}
				{/if}
			</div>
			{#if view === 'tasks'}
				<div class="filters" role="group" aria-label="Group">
					<button class="vt" aria-pressed={taskGroup === 'day'} onclick={() => setTaskGroup('day')}>By day</button>
					<button class="vt" aria-pressed={taskGroup === 'thread'} onclick={() => setTaskGroup('thread')}>By thread</button>
				</div>
				<TaskList
					groupBy={taskGroup}
					{active}
					route={tab.route}
					hideNew
					emptyText="No tasks yet. New task adds one."
					stacked={wide}
					shownId={wide ? (readerTask?.id ?? null) : null}
					pinnedId={wide ? (readerTask?.id ?? null) : null}
					onPick={wide ? chooseTask : undefined}
				/>
			{:else if view === 'overview'}
				<div class="hrow">
					<h2 class="h2">Tasks</h2>
					<span class="hacts">
						<Button variant="ghost" onclick={() => setView('tasks')} tip="All tasks" tipKey="g t">All tasks</Button>
						{@render newTaskButton()}
					</span>
				</div>
				<TaskList
					{active}
					route={tab.route}
					hideNew
					limit={5}
					keys={false}
					emptyText="No tasks yet."
					shownId={lastKind === 'task' ? (shownTask?.id ?? null) : null}
					onHover={(t) => rest('task', t?.id ?? null)}
				/>

				<div class="hrow week-head">
					<h2 class="h2">Active threads</h2>
					<span class="hacts">
						<Button variant="ghost" onclick={() => setView('threads')} tip="All threads" tipKey="g a">All threads</Button>
						{@render newThreadButton()}
					</span>
				</div>
				{@render weekBlock()}
			{:else if view === 'insights'}
				<p class="lede">What your server noticed about your tasks and threads, while you can still act on it.</p>
				<div class="filters" role="group" aria-label="Show">
					<button class="vt" aria-pressed={insightFilter === 'now'} onclick={() => setInsightFilter('now')}>Now</button>
					<button class="vt" aria-pressed={insightFilter === 'aside'} onclick={() => setInsightFilter('aside')}>Set aside</button>
				</div>
				{#if insightItems.length === 0 && insightsFailed}
					<p class="empty">
						Your server couldn't load your insights. <TextAction inline onclick={() => void notesStore.load()}>Try again</TextAction>
					</p>
				{:else if insightItems.length === 0 && insightsLoading}
					<div class="rows" aria-hidden="true">
						{#each [0, 1] as i (i)}<div class="row placeholder tall"></div>{/each}
					</div>
				{:else if insightItems.length === 0 && insightFilter === 'aside'}
					<p class="empty">Nothing set aside. Insights you set aside with Not now wait here until their moment passes.</p>
				{:else if insightItems.length === 0}
					<p class="empty">No insights right now. Your server raises one when something needs you in time to act.</p>
				{:else}
					<ul class="insights" aria-label="Insights">
						{#each insightItems as n (n.key)}
							<li>
								<button
									id={`note-row-${n.key}`}
									type="button"
									class="ins"
									class:chosen={wide && readerNote?.key === n.key}
									aria-current={wide && readerNote?.key === n.key ? 'true' : undefined}
									onclick={() => chooseNote(n)}
									ondblclick={() => openInsight(n)}
								>
									<span class="ins-kind" aria-hidden="true">
										<Icon icon={n.kind === 'clash' ? 'mynaui:calendar' : 'mynaui:clock-circle'} width="16" />
									</span>
									<span class="ins-body">
										<span class="ins-text">{n.text}</span>
										<span class="ins-meta">
											<Icon icon={n.subjects[0]?.startsWith('/task/') ? 'mynaui:circle' : THREAD_ICON} width="14" />
											<span class="ins-about">{n.about}</span>
											<span>{n.when}</span>
										</span>
									</span>
								</button>
							</li>
						{/each}
					</ul>
				{/if}
			{:else}
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
					<div class="rows" role="listbox" tabindex="-1" aria-label="Threads" onmouseleave={() => hoverThread(null)}>
						{#each rows as p (p.id)}
							<div
								id={`thread-row-${p.id}`}
								class="row"
								class:shown={p.id === shownId}
								class:drop-on={dropTarget === p.id}
								class:renaming={renaming?.id === p.id}
								role="option"
								tabindex={p.id === shownId ? 0 : -1}
								aria-selected={p.id === pinnedId}
								onmouseenter={() => hoverThread(p.id)}
								onfocus={() => (pinnedId = p.id)}
								onclick={() => openProject(p)}
								onkeydown={(e) => {
									if (e.key === 'Enter') {
										e.preventDefault();
										openProject(p);
									}
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

	{#if inbox && wide}
		<section class="reader" bind:this={readerEl} aria-label={view === 'tasks' ? 'Task' : 'Insight'}>
			{#if view === 'tasks' && readerTask}
				{@const t = readerTask}
				<div class="reader-bar">
					<Button
						variant="ghost"
						onclick={() => windowShellStore.openTabFromRoute(`/task/${t.id}`, { label: t.title, focusExisting: true })}
						tip="Open the task on its own"
						tipKey="enter"
					>
						Open
					</Button>
				</div>
				<TaskDetail taskId={t.id} layout="pane" />
			{:else if view === 'insights' && readerNote}
				{@const n = readerNote}
				<div class="reader-bar">
					<IconButton icon="mynaui:chevron-up" label="Previous insight" onclick={() => stepNote(-1)} />
					<IconButton icon="mynaui:chevron-down" label="Next insight" onclick={() => stepNote(1)} />
					<Button variant="ghost" onclick={() => openInsight(n)} tip="Open the insight on its own" tipKey="enter">Open</Button>
				</div>
				<InsightDetail key={n.key} layout="pane" />
			{:else}
				<p class="reader-empty">{view === 'tasks' ? 'Choose a task to see it here.' : 'Choose an insight to see it here.'}</p>
			{/if}
		</section>
	{:else if view === 'overview' && lastKind === 'task' && shownTask && wide}
		<aside class="preview" aria-live="polite" aria-label="Preview">
			{#key shownTask.id}
				<div class="pv" in:fly={{ y: 4 }}><TaskPreview task={shownTask} /></div>
			{/key}
		</aside>
	{:else if !inbox && wide}
		<aside class="preview" aria-live="polite" aria-label="Preview">
			{#if shown}
				{#key shown.id}
					<div class="pv" in:fly={{ y: 4 }}>
						<ThreadPreview thread={shown} {detail} {week} onOpen={() => openProject(shown)} onStep={step} />
					</div>
				{/key}
			{:else if !firstLoad}
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

	/* The door's views across the top, with the view's New button at the end. */
	.viewbar {
		display: flex;
		align-items: flex-end;
		justify-content: space-between;
		gap: 16px;
		min-height: 34px;
		margin-bottom: 24px;
		border-bottom: 1px solid var(--color-border-subtle);
	}
	.viewbar :global(.subnav button) {
		padding-bottom: 8px;
	}
	.viewbar > :global(:not(.subnav)) {
		margin-bottom: 8px;
	}
	.filters {
		display: flex;
		gap: 4px;
		margin: 0 0 8px;
	}
	.lede + .filters {
		margin-top: 16px;
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
	.vt[aria-pressed='true'] {
		background: var(--active-bg);
		color: var(--color-foreground);
	}
	.lede {
		margin: 0;
		max-width: 620px;
		font-size: var(--text-label);
		line-height: 20px;
		color: var(--color-foreground-muted);
	}
	/* An insight's row: what kind it is, the sentence (two lines at most),
	   then what it is about and when. The whole row chooses it. */
	.insights {
		list-style: none;
		margin: 16px 0 0;
		padding: 0;
	}
	.ins {
		display: flex;
		align-items: flex-start;
		gap: 12px;
		width: calc(100% + 24px);
		margin: 0 -12px;
		padding: 12px;
		border: 0;
		border-radius: var(--radius-md);
		background: transparent;
		font: inherit;
		text-align: left;
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.ins:hover {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}
	.ins.chosen {
		background: var(--active-bg);
	}
	.ins-kind {
		display: grid;
		place-items: center;
		flex: none;
		width: 20px;
		height: 20px;
		color: var(--color-foreground-muted);
	}
	.ins-body {
		display: flex;
		flex-direction: column;
		gap: 4px;
		flex: 1 1 auto;
		min-width: 0;
	}
	.ins-text {
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
		font-size: var(--text-label);
		line-height: 20px;
		color: var(--color-foreground);
	}
	.ins-meta {
		display: flex;
		align-items: center;
		gap: 8px;
		min-width: 0;
		font-size: var(--text-caption);
		line-height: 16px;
		color: var(--color-foreground-muted);
	}
	.ins-about {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-family: var(--font-serif-ui);
		font-size: var(--text-small);
		color: var(--color-foreground);
	}

	/* The inbox: the list a fixed share of the width, the chosen item in the
	   rest, set apart by space. */
	.door.inbox .main {
		flex: 0 0 clamp(400px, 40%, 560px);
	}
	.reader {
		flex: 1 1 auto;
		min-width: 0;
		height: 100%;
		overflow-y: auto;
		box-sizing: border-box;
	}
	.reader-bar {
		position: sticky;
		top: 0;
		z-index: 2;
		display: flex;
		align-items: center;
		justify-content: flex-end;
		gap: 4px;
		padding: 40px 32px 8px;
		/* The page's own paper, so text scrolling under it does not show through. */
		background: var(--color-page);
	}
	.reader-empty {
		margin: 0;
		padding: 88px 40px;
		font-size: var(--text-label);
		color: var(--color-foreground-muted);
	}

	/* A section's title, then its buttons: where to see all of it, and New. */
	.hrow {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
		min-height: 34px;
		margin: 0 0 12px;
	}
	.hacts {
		display: inline-flex;
		align-items: center;
		gap: 8px;
	}
	.week-head {
		margin-top: 48px;
	}
	.h2 {
		margin: 0;
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: var(--md-h2-size);
		line-height: 28px;
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
	/* Where rows will be while they load. */
	.row.placeholder {
		height: 40px;
		margin: 4px 0;
		border-radius: var(--radius-md);
		background: var(--fill-bg);
		cursor: default;
	}
	.row.placeholder.tall {
		height: 64px;
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
	.empty,
	.quiet {
		margin: 16px 0 0;
		font-size: var(--text-label);
		line-height: 24px;
		color: var(--color-foreground-muted);
		max-width: 520px;
	}

	/* The preview: a column on the same paper as the list, set apart by
	   space. It stays put while the list scrolls. */
	.preview {
		flex: none;
		width: 320px;
		height: 100%;
		box-sizing: border-box;
		padding: 64px 24px 64px 8px;
		overflow-y: auto;
	}
	.preview .quiet {
		margin-top: 0;
		font-size: var(--text-label);
	}
	@media (prefers-reduced-motion: reduce) {
		.row,
		.vt {
			transition: none;
		}
	}
</style>
