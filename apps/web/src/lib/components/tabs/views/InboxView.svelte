<script lang="ts">
	/**
	 * The Inbox: everything that asks something of you, in one place. That is
	 * your open tasks, what your server noticed and what your server needs.
	 *
	 * The page is always split. The left side is how you find a thing, and
	 * the right side is the thing itself: a task's page, an insight's page or
	 * a thread. The left side has six views of the same list:
	 *
	 *   List    one row each, nearest first. The page opens here.
	 *   Radar   you in the middle, sooner things closer, one slice per thread
	 *   Cards   a column for now, today, this week and later
	 *   Graph   what is linked to what, in the radar's frame
	 *   Map     today as a route, with the day's clock under it
	 *   Week    the active threads as a week of dots (WeekChart)
		 *
	 * The view is in the route (`?view=list`), so a link can open any of
	 * them. J and K walk the things shown, Enter opens the chosen one on its
	 * own page, and Escape lets go.
	 *
	 * The sidebar's Inbox row opens it.
	 *
	 * TODO(2026-11-19): migrate ThreadsView onto this view. It is meant to
	 * replace ThreadsView's Overview, Tasks, All threads and Insights, which
	 * /threads still serves.
	 */
	import { onMount, untrack } from 'svelte';
	import type { Tab } from '$lib/tabs/types';
	import type { ProjectSummary, Week } from '$lib/api/client';
	import { getThreadWeek } from '$lib/api/client';
	import { Button, IconButton, Page, TextAction } from '$lib';
	import Icon from '$lib/components/Icon.svelte';
	import ProjectGlyph from '$lib/components/ProjectGlyph.svelte';
	import WeekChart from '$lib/components/threads/WeekChart.svelte';
	import ThreadPreview from '$lib/components/threads/ThreadPreview.svelte';
	import InsightDetail from '$lib/components/threads/InsightDetail.svelte';
	import { openInsight } from '$lib/components/threads/NoteBox.svelte';
	import TaskDetail from '$lib/components/tasks/TaskDetail.svelte';
	import InboxRadar from '$lib/components/inbox/InboxRadar.svelte';
	import InboxList from '$lib/components/inbox/InboxList.svelte';
	import InboxCards from '$lib/components/inbox/InboxCards.svelte';
	import InboxGraph from '$lib/components/inbox/InboxGraph.svelte';
	import InboxMap from '$lib/components/inbox/InboxMap.svelte';
	import InboxMark from '$lib/components/inbox/InboxMark.svelte';
	import SignalSlider from '$lib/components/inbox/SignalSlider.svelte';
	import type { ExtraLink } from '$lib/inbox/graph';
	import { buildItems, type InboxItem } from '$lib/inbox/items';
	import { LOOSE } from '$lib/inbox/radar';
	import { attention } from '$lib/stores/attention.svelte';
	import { contextMenu, type ContextMenuItem } from '$lib/stores/contextMenu.svelte';
	import { dialogStore } from '$lib/stores/dialog.svelte';
	import { newTaskDialog } from '$lib/stores/newTask.svelte';
	import { newThreadDialog } from '$lib/stores/newThread.svelte';
	import { notesStore } from '$lib/stores/notes.svelte';
	import { projectStore } from '$lib/stores/project.svelte';
	import { tasksStore } from '$lib/stores/tasks.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { homeOf, subjectName } from '$lib/tasks/homes';
	import { newProject, openProject } from '$lib/utils/projectActions';

	let { tab, active = false }: { tab: Tab; active?: boolean } = $props();

	type View = 'list' | 'radar' | 'cards' | 'graph' | 'map' | 'week';
	const VIEWS: { id: View; label: string }[] = [
		{ id: 'list', label: 'List' },
		{ id: 'radar', label: 'Radar' },
		{ id: 'cards', label: 'Cards' },
		{ id: 'graph', label: 'Graph' },
		{ id: 'map', label: 'Map' },
		{ id: 'week', label: 'Week' },
	];
	const view = $derived.by<View>(() => {
		const v = new URLSearchParams(tab.route.split('?')[1] ?? '').get('view');
		return VIEWS.find((x) => x.id === v)?.id ?? 'list';
	});
	function setView(next: View) {
		windowShellStore.updateTab(tab.id, { route: `/inbox${next === 'list' ? '' : `?view=${next}`}` });
	}

	/** The radar's center shows the time, so the clock moves with it. */
	let now = $state(new Date());

	onMount(() => {
		projectStore.load();
		void tasksStore.load();
		notesStore.start();
		attention.start();
		const tick = setInterval(() => (now = new Date()), 60_000);
		return () => {
			notesStore.stop();
			attention.stop();
			clearInterval(tick);
		};
	});

	// ── what is shown ──────────────────────────────────────────────────────
	const activeThreads = $derived(projectStore.projects.filter((p) => !p.is_backlog));
	const backlog = $derived(projectStore.projects.filter((p) => !!p.is_backlog));
	const everyThread = $derived([...projectStore.projects, ...projectStore.archived]);
	const nameOf = (p: Pick<ProjectSummary, 'name'>) => p.name || 'Untitled thread';

	const items = $derived(
		buildItems({
			tasks: tasksStore.open,
			notes: notesStore.items,
			attention: attention.items,
			active: new Set(activeThreads.map((p) => p.id)),
			unlisted: new Set(activeThreads.filter((p) => p.is_listed === false).map((p) => p.id)),
			now,
		}),
	);
	const radarThreads = $derived(activeThreads.map((p) => ({ id: p.id, name: nameOf(p), group: p.thing_title ?? null })));

	/**
	 * How far from signal (0) toward noise (1) the page shows, and which
	 * kinds. Each view already says how soon a thing is, so the control
	 * thins it by how much the thing asks of you. It starts at everything.
	 */
	let cut = $state(1);
	type Show = 'all' | 'tasks' | 'insights';
	let show = $state<Show>('all');
	const SHOWS: { value: Show; label: string }[] = [
		{ value: 'all', label: 'All' },
		{ value: 'tasks', label: 'Tasks' },
		{ value: 'insights', label: 'Insights' },
	];
	const matches = (it: InboxItem) => (show === 'all' || (show === 'tasks' ? !!it.task : it.kind === 'insight')) && it.noise <= cut;
	const listed = $derived(items.filter(matches));
	/** The things for now and today, which is what the map draws. */
	const todays = $derived(items.filter((it) => it.ring <= 1));
	/** What J and K walk: the things the view shows. */
	const walked = $derived(view === 'map' ? todays.filter(matches) : view === 'list' ? listed.filter((it) => !it.attention) : listed);

	/**
	 * A task's links beyond its own active thread, for the graph: a home that
	 * is a person, a place, an organization, a thing or a thread that is not
	 * active, and what else the task is about.
	 */
	function graphLinks(it: InboxItem): ExtraLink[] {
		if (!it.task) return [];
		const activeIds = new Set(activeThreads.map((p) => p.id));
		const out: ExtraLink[] = [];
		const home = homeOf(it.task);
		const also = it.task.also_about ?? [];
		for (const link of [...(home ? [home] : []), ...also]) {
			const rel = link !== home ? 'also about' : link.kind === 'place' ? 'at' : link.kind === 'thread' ? 'in' : 'for';
			if (link.kind === 'thread' && activeIds.has(link.id)) {
				if (link !== home) out.push({ to: { thread: link.id }, rel });
			} else {
				out.push({ to: { subject: { key: `${link.kind}:${link.id}`, label: subjectName(link) } }, rel });
			}
		}
		return out;
	}

	/** The thread a thing sits in. A task keeps its thread even when the thread is not active. */
	function threadOf(it: InboxItem): ProjectSummary | null {
		const id = it.task?.thread_id ?? it.threadId;
		return (id && everyThread.find((p) => p.id === id)) || null;
	}
	function threadName(it: InboxItem): string {
		const thread = threadOf(it);
		return thread ? nameOf(thread) : '';
	}

	// ── what is chosen ─────────────────────────────────────────────────────
	// One choice for every view: a thing, a thread, or nothing.
	type Choice = { kind: 'rest' } | { kind: 'item'; key: string } | { kind: 'thread'; id: string };
	let choice = $state<Choice>({ kind: 'rest' });
	/** Escape in the list lets go: the right side shows nothing until you choose again. */
	let released = $state(false);

	/** The door's own width. A narrow pane has no room for the right side, so there a click opens the page. */
	let doorWidth = $state(0);
	const wide = $derived(doorWidth === 0 || doorWidth >= 960);

	// A thing that has since gone (a task you finished) no longer counts as chosen.
	const pickedKey = $derived(choice.kind === 'item' ? choice.key : null);
	const picked = $derived(pickedKey ? (items.find((it) => it.key === pickedKey) ?? null) : null);
	const pickedThreadId = $derived(choice.kind === 'thread' ? choice.id : null);
	const thread = $derived(pickedThreadId && pickedThreadId !== LOOSE ? (everyThread.find((p) => p.id === pickedThreadId) ?? null) : null);
	const detail = $derived(thread ? (projectStore.getCached(thread.id) ?? null) : null);
	const atRest = $derived(!picked && !thread && pickedThreadId !== LOOSE);
	/**
	 * What the right side shows. The list opens on its first thing of yours,
	 * as a mail app opens on its first message, until you choose another or
	 * let go. The other views show only what you chose.
	 */
	const first = $derived(listed.find((it) => !it.attention) ?? listed[0] ?? null);
	const current = $derived(picked ?? (view === 'list' && wide && choice.kind === 'rest' && !released ? first : null));

	// Letting go lasts until you leave the view.
	$effect(() => {
		void view;
		untrack(() => (released = false));
	});

	$effect(() => {
		const id = thread?.id;
		if (id && !projectStore.getCached(id)) {
			projectStore.get(id).catch((e) => console.error('[InboxView] thread load failed:', e));
		}
	});

	function openItem(it: InboxItem) {
		if (it.task) windowShellStore.openTabFromRoute(`/task/${it.task.id}`, { label: it.task.title, focusExisting: true });
		else if (it.note) openInsight(it.note);
		else if (it.attention) windowShellStore.openTabFromRoute(it.attention.route, { focusExisting: true });
	}

	function pick(key: string) {
		const it = items.find((i) => i.key === key);
		if (!it) return;
		released = false;
		if (wide) choice = { kind: 'item', key };
		else openItem(it);
	}

	function pickThread(id: string) {
		const p = everyThread.find((t) => t.id === id);
		released = false;
		if (!wide && p) openProject(p);
		else choice = { kind: 'thread', id };
	}

	function step(by: number) {
		if (walked.length === 0) return;
		const from = current ? walked.indexOf(current) : by > 0 ? -1 : 0;
		const next = walked[(from + by + walked.length) % walked.length];
		released = false;
		choice = { kind: 'item', key: next.key };
		document.querySelector(`[data-inbox-key="${CSS.escape(next.key)}"]`)?.scrollIntoView({ block: 'nearest' });
	}

	/** The pager on a thread walks the threads it is listed with. */
	function stepThread(by: number) {
		if (!thread) return;
		const peers = thread.archived_at ? projectStore.archived : thread.is_backlog ? backlog : activeThreads;
		const i = peers.findIndex((p) => p.id === thread.id);
		if (i >= 0) choice = { kind: 'thread', id: peers[(i + by + peers.length) % peers.length].id };
	}

	/** The right side starts at the top for each thing. */
	let readerEl = $state<HTMLElement | null>(null);
	const readerKey = $derived(current?.key ?? pickedThreadId ?? '');
	$effect(() => {
		void readerKey;
		untrack(() => readerEl?.scrollTo({ top: 0 }));
	});

	// ── the week ───────────────────────────────────────────────────────────
	let weekStart = $state<string | null>(null);
	let week = $state<Week | null>(null);
	let weekError = $state(false);
	/** Bumped by Try again, to fetch the week again. */
	let weekTry = $state(0);
	let hoveredThread = $state<string | null>(null);

	$effect(() => {
		const start = weekStart;
		void projectStore.projects.length;
		void weekTry;
		getThreadWeek(start ?? undefined)
			.then((w) => {
				week = w;
				weekError = false;
			})
			.catch((e) => {
				console.error('[InboxView] week failed:', e);
				weekError = true;
			});
	});

	/**
	 * Week draws threads, so there the control thins the threads. A thread
	 * sits where the strongest thing in it does, and a thread that asks
	 * nothing of you sits at the noise end.
	 */
	const threadNoise = $derived.by(() => {
		const at = new Map<string, number>();
		for (const it of items) if (it.threadId) at.set(it.threadId, Math.min(at.get(it.threadId) ?? 1, it.noise));
		return at;
	});
	const shownWeek = $derived(week ? { ...week, threads: week.threads.filter((t) => (threadNoise.get(t.id) ?? 1) <= cut) } : null);

	function shiftWeek(days: number) {
		if (!week) return;
		const d = new Date(`${week.start}T12:00:00`);
		d.setDate(d.getDate() + days);
		const m = String(d.getMonth() + 1).padStart(2, '0');
		weekStart = `${d.getFullYear()}-${m}-${String(d.getDate()).padStart(2, '0')}`;
	}

	// ── threads that are out of view ───────────────────────────────────────
	// An unlisted thread has no row in the sidebar, so this shelf is where
	// you find one that has nothing in the Inbox right now.
	type Shelf = 'unlisted' | 'backlog' | 'finished';
	const SHELVES: { id: Shelf; label: string; empty: string }[] = [
		{ id: 'unlisted', label: 'Unlisted', empty: 'Threads you unlist wait here.' },
		{ id: 'backlog', label: 'Backlog', empty: 'Threads you mean to come back to wait here.' },
		{ id: 'finished', label: 'Finished', empty: 'Threads you finish or drop wait here.' },
	];
	let shelf = $state<Shelf | null>(null);
	const shelfInfo = $derived(SHELVES.find((x) => x.id === shelf));
	const shelfRows = $derived(
		shelf === 'unlisted'
			? projectStore.projects.filter((p) => p.is_listed === false)
			: shelf === 'backlog'
				? backlog
				: shelf === 'finished'
					? projectStore.archived
					: [],
	);

	/** A new task starts in the thread you chose, if any. */
	function newTaskHere() {
		newTaskDialog.open(thread && !thread.archived_at ? { home: { kind: 'thread', id: thread.id } } : undefined);
	}

	function onKey(e: KeyboardEvent) {
		if (!active || e.defaultPrevented || e.metaKey || e.ctrlKey || e.altKey || e.repeat) return;
		// Enter on a focused button or link is that control's, not the list's.
		if (e.key === 'Enter' && e.target instanceof Element && e.target.closest('button, a, [role="dialog"]')) return;
		if (newTaskDialog.isOpen || newThreadDialog.isOpen || dialogStore.pending) return;
		const target = e.target;
		if (target instanceof Element && target.closest('input, textarea, [contenteditable="true"], [role="menu"]')) return;
		if (e.key === 'c') {
			e.preventDefault();
			newTaskHere();
		} else if (e.key === 'C') {
			e.preventDefault();
			void newProject();
		} else if (e.key === 'Escape' && (!atRest || current)) {
			choice = { kind: 'rest' };
			released = true;
		} else if (view === 'week') {
			return;
		} else if (e.key === 'j' || e.key === 'ArrowDown') {
			e.preventDefault();
			step(1);
		} else if (e.key === 'k' || e.key === 'ArrowUp') {
			e.preventDefault();
			step(-1);
		} else if (e.key === 'Enter' && current) {
			e.preventDefault();
			openItem(current);
		} else if (e.key === 'Enter' && thread) {
			e.preventDefault();
			openProject(thread);
		}
	}

	// ── the menus on the line above the list ───────────────────────────────
	function menuUnder(e: MouseEvent, menu: ContextMenuItem[], placement: 'bottom-start' | 'bottom-end' = 'bottom-start') {
		const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
		contextMenu.show({ x: rect.left, y: rect.bottom }, menu, {
			anchor: { x: rect.left, y: rect.top, width: rect.width, height: rect.height },
			placement,
		});
	}
	const viewItems = (): ContextMenuItem[] =>
		VIEWS.map((v) => ({ id: v.id, label: v.label, role: 'menuitemradio', checked: v.id === view, action: () => setView(v.id) }));
	const showItems = (): ContextMenuItem[] =>
		SHOWS.map((s) => ({ id: s.value, label: s.label, role: 'menuitemradio', checked: s.value === show, action: () => void (show = s.value) }));
	const newItems = (): ContextMenuItem[] => [
		{ id: 'task', label: 'New task', shortcut: 'C', action: newTaskHere },
		{ id: 'thread', label: 'New thread', shortcut: '⇧C', action: () => newProject() },
	];

	const near = $derived(items.filter((it) => it.ring <= 1));
	const loose = $derived(items.filter((it) => it.threadId === null));
	const loadFailed = $derived(!tasksStore.loaded && !!tasksStore.error);
</script>

<svelte:window onkeydown={onKey} />

<div class="door" bind:clientWidth={doorWidth}>
	<Page title="Inbox" maxWidth="none" padding="none" scrollable={false} class="flex flex-col md:flex-row">
		<div class="main" class:list={view === 'list' && wide}>
			<div class="bar">
				<button type="button" class="viewpick" aria-haspopup="menu" onclick={(e) => menuUnder(e, viewItems())}>
					{VIEWS.find((v) => v.id === view)?.label}
					<Icon icon="mynaui:chevron-down" width="14" />
				</button>
				<span class="acts">
					<SignalSlider value={cut} onchange={(v) => (cut = v)} />
					<!-- Week draws threads, so there is nothing for Show to filter. -->
					{#if view !== 'week'}
						<IconButton icon="mynaui:filter" label="Show" pressed={show !== 'all'} haspopup="menu" onclick={(e) => menuUnder(e, showItems(), 'bottom-end')} />
					{/if}
					<IconButton icon="mynaui:plus" label="New" haspopup="menu" onclick={(e) => menuUnder(e, newItems(), 'bottom-end')} />
				</span>
			</div>

			{#if loadFailed}
				<p class="empty">
					Your server couldn't load your tasks. Check that it's running, then
					<TextAction inline onclick={() => void tasksStore.load()}>try again</TextAction>.
				</p>
			{:else if view === 'radar'}
				<InboxRadar
					{items}
					threads={radarThreads}
					{now}
					picked={picked?.key ?? null}
					pickedThread={pickedThreadId}
					{atRest}
					faded={(it) => !matches(it)}
					onPick={pick}
					onThread={pickThread}
					onRest={() => (choice = { kind: 'rest' })}
				/>
			{:else if view === 'graph'}
				<InboxGraph
					{items}
					threads={radarThreads}
					picked={picked?.key ?? null}
					pickedThread={pickedThreadId}
					{atRest}
					faded={(it) => !matches(it)}
					extra={graphLinks}
					onPick={pick}
					onThread={pickThread}
					onRest={() => (choice = { kind: 'rest' })}
				/>
			{:else if view === 'map'}
				<InboxMap items={todays} {now} picked={picked?.key ?? null} faded={(it) => !matches(it)} onPick={pick} />
			{:else if view === 'week'}
				<div class="body">
					{#if shownWeek && week && week.threads.length > 0}
						<WeekChart
							week={shownWeek}
							shownId={thread?.id ?? null}
							pinnedId={thread?.id ?? null}
							hoverId={hoveredThread}
							onPrev={() => shiftWeek(-7)}
							onNext={() => shiftWeek(7)}
							onHover={(id) => (hoveredThread = id)}
							onOpen={pickThread}
						/>
					{:else if weekError}
						<p class="empty">
							Your server couldn't load the week. <TextAction inline onclick={() => (weekTry += 1)}>Try again</TextAction>
						</p>
					{:else if week}
						<p class="empty">
							A thread is something happening in your life that you follow. Its time shows here week by week. New thread starts
							one.
						</p>
					{/if}
				</div>
			{:else if view === 'list' && !tasksStore.loaded}
				<div class="body" aria-hidden="true">
					{#each [0, 1, 2, 3] as i (i)}<div class="placeholder"></div>{/each}
				</div>
			{:else if listed.length === 0 && tasksStore.loaded}
				<p class="empty">
					{items.length === 0 ? 'Nothing is waiting for you. New task adds one.' : 'Nothing this close to Signal. Move toward Noise to see more.'}
				</p>
			{:else if view === 'list'}
				<div class="body"><InboxList items={listed} picked={current?.key ?? null} serverLast {threadOf} onPick={pick} /></div>
			{:else}
				<div class="body"><InboxCards items={listed} picked={picked?.key ?? null} {threadName} onPick={pick} /></div>
			{/if}

			<!-- The shelf lists threads, so it shows in the views that draw threads. -->
			{#if view !== 'list' && view !== 'map'}
				<div class="shelf">
					{#if shelfInfo}
						<ul class="shelf-list" aria-label={shelfInfo.label}>
							{#each shelfRows as p (p.id)}
								<li>
									<button type="button" class="srow" class:on={pickedThreadId === p.id} aria-pressed={pickedThreadId === p.id} onclick={() => pickThread(p.id)}>
										<ProjectGlyph project={p} size={16} tinted />
										<span class="sname">{nameOf(p)}</span>
									</button>
								</li>
							{:else}
								<li class="none">{shelfInfo.empty}</li>
							{/each}
						</ul>
					{/if}
					<div class="shelf-bar">
						<span class="acts">
							{#each SHELVES as x (x.id)}
								<Button variant="ghost" onclick={() => (shelf = shelf === x.id ? null : x.id)}>{x.label}</Button>
							{/each}
						</span>
						{#if view !== 'week'}
							<span class="legend">
								<span><InboxMark kind="needs" /> Waits on you</span>
								<span><InboxMark kind="insight" /> Insight</span>
								<span><InboxMark kind="task" /> Task</span>
								<span><InboxMark kind="waiting" /> Request in progress</span>
							</span>
						{/if}
					</div>
				</div>
			{/if}
		</div>

		{#if wide}
			<section class="reader" bind:this={readerEl} aria-label="Chosen" aria-live="polite">
				<div class="reader-bar">
					<span class="reader-title">{current?.title ?? (thread ? nameOf(thread) : '')}</span>
					{#if current}
						<span class="acts">
							<IconButton icon="mynaui:chevron-up" label="Previous" shortcut="K" onclick={() => step(-1)} />
							<IconButton icon="mynaui:chevron-down" label="Next" shortcut="J" onclick={() => step(1)} />
							<IconButton icon="mynaui:arrow-up-right" label="Open on its own" shortcut="↵" onclick={() => openItem(current)} />
						</span>
					{/if}
				</div>
				{#if current?.task}
					<TaskDetail taskId={current.task.id} layout="pane" />
				{:else if current?.note}
					<InsightDetail key={current.note.key} layout="pane" />
				{:else if current?.attention}
					{@const a = current.attention}
					<div class="block">
						<h2 class="h2">{a.title}</h2>
						<p class="para">{a.body}</p>
						<Button variant="secondary" onclick={() => openItem(current)} tip="Open the page that shows it" tipKey="enter">Open</Button>
					</div>
				{:else if thread}
					{#key thread.id}
						<div class="block narrow">
							<ThreadPreview {thread} {detail} {week} onOpen={() => openProject(thread)} onStep={stepThread} />
						</div>
					{/key}
				{:else if view === 'list' && pickedThreadId !== LOOSE}
					<!-- The list is already on the left, so the right side does not repeat it. -->
					{#if listed.length > 0}<p class="reader-empty">Choose something to see it here.</p>{/if}
				{:else}
					{@const rows = pickedThreadId === LOOSE ? loose : near}
					<div class="block">
						<h2 class="h2">{pickedThreadId === LOOSE ? 'Not in a thread' : 'Closest to you'}</h2>
						{#if rows.length > 0}
							<InboxList items={rows} {threadOf} onPick={pick} />
						{:else if tasksStore.loaded}
							<p class="para">{pickedThreadId === LOOSE ? 'Everything here sits in a thread.' : 'Nothing needs you today.'}</p>
						{/if}
					</div>
				{/if}
			</section>
		{/if}
	</Page>
</div>

<style>
	.door {
		height: 100%;
		min-height: 0;
	}
	/* The page is always split: how you find a thing on the left, the thing
	   itself on the right. */
	.main {
		display: flex;
		flex: 1 1 auto;
		flex-direction: column;
		min-width: 0;
		min-height: 0;
	}
	/* In the list the page is laid out as a mail app: the list takes a fixed
	   share of the width and the chosen thing gets the rest. */
	.main.list {
		flex: 0 0 clamp(400px, 36%, 520px);
	}
	.main.list + .reader {
		flex: 1 1 auto;
	}

	/* One quiet line above each side: the view, the signal control and two
	   menus on the left, the chosen thing's name and its three buttons on
	   the right. */
	.bar,
	.reader-bar {
		display: flex;
		flex: none;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
		height: 48px;
		box-sizing: border-box;
		padding: 0 12px 0 16px;
		border-bottom: 1px solid var(--color-border-subtle);
	}
	.viewpick {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		height: var(--control-h);
		padding: 0 8px;
		border: 0;
		border-radius: var(--radius-md);
		background: transparent;
		font: inherit;
		font-size: var(--text-label);
		color: var(--color-foreground);
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.viewpick:hover {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}
	.acts {
		display: inline-flex;
		flex: 0 1 auto;
		align-items: center;
		gap: 4px;
		min-width: 0;
	}
	/* The control keeps a little air before the buttons, and gives up width first in a narrow column. */
	.acts :global(.signal) {
		margin-right: 8px;
	}

	/* The list, the cards and the week scroll inside the left side. The side
	   padding leaves room for a row's wash, which is wider than its text. */
	.body {
		flex: 1 1 auto;
		min-height: 0;
		padding: 16px 24px 24px;
		overflow: auto;
	}
	.empty {
		flex: 1 1 auto;
		margin: 16px 24px 0;
		max-width: 520px;
		font-size: var(--text-label);
		line-height: 24px;
		color: var(--color-foreground-muted);
	}
	/* Where a row will be while the list loads. */
	.placeholder {
		height: 44px;
		margin: 4px 0;
		border-radius: var(--radius-md);
		background: var(--fill-bg);
	}

	/* Threads that are not active: Backlog and Finished open a short list. */
	.shelf {
		flex: none;
		margin: 0 24px;
		border-top: 1px solid var(--color-border-subtle);
	}
	.shelf-bar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
		min-height: 48px;
	}
	.shelf-list {
		max-height: 200px;
		margin: 0;
		padding: 8px 0 0;
		overflow-y: auto;
		list-style: none;
	}
	.srow {
		display: flex;
		align-items: center;
		gap: 12px;
		width: 100%;
		min-height: 36px;
		padding: 0 12px;
		border: 0;
		border-radius: var(--radius-md);
		background: transparent;
		font: inherit;
		text-align: left;
		color: var(--color-foreground);
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.srow:hover {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}
	.srow.on {
		background: var(--active-bg);
	}
	.sname {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-family: var(--font-serif-ui);
		font-size: var(--text-body);
	}
	.none {
		padding: 8px 12px;
		font-size: var(--text-small);
		color: var(--color-foreground-muted);
	}
	.legend {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: flex-end;
		gap: 4px 16px;
		font-size: var(--text-caption);
		color: var(--color-foreground-muted);
	}
	.legend > span {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		white-space: nowrap;
	}

	/* The right side: the chosen thing, whole. */
	.reader {
		flex: 0 0 clamp(380px, 38%, 560px);
		min-width: 0;
		box-sizing: border-box;
		overflow-y: auto;
		border-left: 1px solid var(--color-border-subtle);
	}
	/* Its line stays put while the thing scrolls under it, on the page's own
	   paper so the text does not show through. */
	.reader-bar {
		position: sticky;
		top: 0;
		z-index: 2;
		padding: 0 12px 0 32px;
		background: var(--color-page);
	}
	.reader-title {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: var(--text-small);
		color: var(--color-foreground-muted);
	}
	.reader-empty {
		margin: 0;
		padding: 40px;
		font-size: var(--text-label);
		color: var(--color-foreground-muted);
	}
	.block {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 16px;
		padding: 40px 40px 64px;
	}
	.block > :global(section),
	.block > :global(ul) {
		align-self: stretch;
	}
	.block.narrow {
		align-items: stretch;
		max-width: 400px;
	}
	.h2 {
		margin: 0;
		font-family: var(--font-serif);
		font-size: var(--text-heading);
		font-weight: 400;
		line-height: 28px;
		color: var(--color-foreground);
	}
	.para {
		margin: 0;
		font-size: var(--text-label);
		line-height: 24px;
		color: var(--color-foreground-muted);
	}

	@media (prefers-reduced-motion: reduce) {
		.srow,
		.viewpick {
			transition: none;
		}
	}
</style>
