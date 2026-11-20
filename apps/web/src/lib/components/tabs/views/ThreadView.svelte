<script lang="ts">
	/**
	 * A thread's page. The main column is the thread itself: its name and
	 * brief, where it stands, any note your server raised about it, the way
	 * into a chat about it, its open tasks (TaskList, with the tasks also
	 * about it), and the thread as a line. The line runs from the day you
	 * started it, through everything added to it and the tasks you finished,
	 * down to today, then on to what is planned. A task you tick off leaves
	 * the list and lands on the line.
	 *
	 * The side column holds its properties (status, Adding, the thing it sits
	 * under, who it is with), this week's time, and what is coming up. Each
	 * fact is said in one place. Anything dropped on the page is added to it.
	 */
	import type { Tab } from '$lib/tabs/types';
	import type { ProjectChat, ProjectDetail, ProjectGraphNode, ProjectItemRole, Task, Week } from '$lib/api/client';
	import DetailFrame from '$lib/components/DetailFrame.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import MenuItem from '$lib/components/MenuItem.svelte';
	import ProjectGlyph from '$lib/components/ProjectGlyph.svelte';
	import RefPicker from '$lib/components/RefPicker.svelte';
	import TaskCheck from '$lib/components/tasks/TaskCheck.svelte';
	import TaskList from '$lib/components/tasks/TaskList.svelte';
	import NoteBox from '$lib/components/threads/NoteBox.svelte';
	import PreviewRow from '$lib/components/threads/PreviewRow.svelte';
	import ThreadHeader from '$lib/components/threads/ThreadHeader.svelte';
	import ThreadUpNext from '$lib/components/threads/ThreadUpNext.svelte';
	import ThreadWeekBars from '$lib/components/threads/ThreadWeekBars.svelte';
	import { Popover } from '$lib/floating';
	import { THREAD_ICON } from '$lib/utils/iconHelpers';
	import { IconButton, TextAction } from '$lib';
	import { projectStore } from '$lib/stores/project.svelte';
	import { chatSessions } from '$lib/stores/chatSessions.svelte';
	import { notesStore } from '$lib/stores/notes.svelte';
	import { tasksStore } from '$lib/stores/tasks.svelte';
	import { thingsStore } from '$lib/stores/things.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { contextMenu } from '$lib/stores/contextMenu.svelte';
	import type { ContextMenuItem } from '$lib/stores/contextMenu.svelte';
	import { toast } from 'svelte-sonner';
	import { onMount } from 'svelte';
	import { ApiError, uploadDriveFile, addProjectItem, reextractDriveFile, getProjectGraph, getThreadWeek } from '$lib/api/client';
	import { projectColor } from '$lib/sidebar/pin-colors';
	import { askVirtues } from '$lib/stores/pendingPrompt.svelte';
	import { newTaskDialog } from '$lib/stores/newTask.svelte';
	import { dropZone, hasFiles } from '$lib/actions/dropZone';
	import { toggleTaskDone } from '$lib/tasks/actions';
	import { homeOf } from '$lib/tasks/homes';
	import { isRequest, requestState } from '$lib/tasks/requests';
	import {
		changeOutcome,
		droppedRefUrl,
		endProject,
		fileIntoProject,
		memberIcon,
		memberName,
		newChatInProject,
		outcomeLabel,
		projectMemberUrl,
		REF_DRAG_TYPE,
		removeFromProject,
		reopenProject,
	} from '$lib/utils/projectActions';
	import { getProjectMenuItems } from '$lib/utils/contextMenuItems';
	import { openWebLink } from '$lib/utils/webLinks';

	let { tab, active = false }: { tab: Tab; active?: boolean } = $props();

	const threadId = $derived.by(() => {
		const m = tab.route.match(/^\/thread\/([^/?#]+)/);
		return m?.[1] ?? null;
	});

	/**
	 * A live view of the store's copy, not a snapshot of it. A rename, recolor
	 * or ending from the sidebar, ⌘K or another pane lands in the store, and
	 * this page and its tab follow without being told.
	 */
	const detail = $derived<ProjectDetail | null>(
		threadId ? (projectStore.getCached(threadId) ?? null) : null
	);
	let loading = $state(false);
	let error = $state<string | null>(null);

	async function load(force = false) {
		const id = threadId;
		if (!id) return;
		loading = true;
		error = null;
		try {
			await projectStore.get(id, { force });
		} catch (e) {
			console.error('[ThreadView] Failed to load thread:', e);
			// A missing thread was most likely deleted; anything else is the server not answering.
			error =
				e instanceof ApiError && e.status === 404
					? "This thread isn't here. If you deleted it, you can restore it from Recently deleted."
					: "Your server couldn't open this thread. Check that it's running, then try again.";
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		if (threadId) load();
	});

	// The tab wears the thread's name and mark, whichever door opened it:
	// a pin, a citation, ⌘K and a reload all arrive with only the id.
	$effect(() => {
		if (!detail) return;
		const icon = detail.icon || THREAD_ICON;
		if (tab.label !== detail.name || tab.icon !== icon) {
			windowShellStore.updateTab(tab.id, { label: detail.name, icon });
		}
	});

	const archived = $derived(!!detail?.archived_at);
	const color = $derived(detail ? projectColor(detail) : 'var(--color-foreground-muted)');

	// Finished tasks, so the line can show what was done in it, and notes
	// about the thread. The task list loads and refreshes the open ones.
	onMount(() => {
		notesStore.start();
		thingsStore.ensure();
		if (!tasksStore.closedLoaded) {
			tasksStore.loadClosed().catch((e) => console.error('[ThreadView] finished tasks failed:', e));
		}
		return () => notesStore.stop();
	});

	const notes = $derived(threadId ? notesStore.about(`/thread/${threadId}`) : []);

	// Who it is with: the people its members name or link, from the thread's
	// graph. Nothing is inferred from free text.
	let people = $state<ProjectGraphNode[]>([]);
	async function loadPeople() {
		const id = threadId;
		if (!id) return;
		try {
			const graph = await getProjectGraph(id);
			people = graph.nodes.filter((n) => n.entity_type === 'person');
		} catch (e) {
			// An aid, not the content: the page stands without it.
			console.error('[ThreadView] Failed to load who it is with:', e);
			people = [];
		}
	}
	$effect(() => {
		void detail?.items.length;
		if (threadId) loadPeople();
	});

	// ── this week ──────────────────────────────────────────────────────────
	let week = $state<Week | null>(null);
	$effect(() => {
		void detail?.items.length;
		if (!threadId) return;
		getThreadWeek()
			.then((w) => (week = w))
			.catch((e) => {
				// An aid, not the content: the page stands without it.
				console.error('[ThreadView] week failed:', e);
				week = null;
			});
	});
	const weekDays = $derived(week?.threads.find((t) => t.id === threadId)?.days ?? null);

	// ── where it stands ────────────────────────────────────────────────────
	let standsDraft = $state('');
	let standsEditing = $state(false);
	$effect(() => {
		if (!standsEditing) standsDraft = detail?.current_status ?? '';
	});

	async function saveStands() {
		standsEditing = false;
		const id = threadId;
		const next = standsDraft.trim();
		if (!id || next === (detail?.current_status ?? '')) return;
		try {
			await projectStore.update(id, { current_status: next || null });
		} catch (e) {
			console.error('[ThreadView] saving where it stands failed:', e);
			toast.error("Your server couldn't save that", { description: 'Nothing changed. Try again' });
		}
	}

	// ── properties ─────────────────────────────────────────────────────────
	async function setProps(patch: { is_backlog?: boolean; is_adding_automatic?: boolean; thing_id?: string | null }) {
		const id = threadId;
		if (!id) return;
		try {
			await projectStore.update(id, patch);
		} catch (e) {
			console.error('[ThreadView] update failed:', e);
			toast.error("Your server couldn't change that", { description: 'Nothing changed. Try again' });
		}
	}

	type Status = 'active' | 'backlog' | 'finished' | 'dropped';
	const status = $derived<Status>(
		!detail ? 'active' : detail.archived_at ? (detail.outcome === 'dropped' ? 'dropped' : 'finished') : detail.is_backlog ? 'backlog' : 'active',
	);
	const STATUS_ICON: Record<Status, string> = {
		active: 'mynaui:circle-half',
		backlog: 'mynaui:archive',
		finished: 'mynaui:check-circle',
		dropped: 'mynaui:x-circle',
	};
	const statusText = $derived.by(() => {
		if (!detail) return '';
		if (detail.archived_at) return `${outcomeLabel(detail)} ${shortDay(localDay(new Date(detail.archived_at)))}`;
		return detail.is_backlog ? 'In your backlog' : 'Active';
	});

	/** Status is one property with four values. Ending and reopening go through it. */
	async function setStatus(next: Status) {
		if (!detail || next === status) return;
		if (next === 'finished' || next === 'dropped') {
			if (archived) await changeOutcome(detail, next);
			else await endProject(detail, next);
			return;
		}
		if (archived) await reopenProject(detail);
		if (!!detail.is_backlog !== (next === 'backlog')) await setProps({ is_backlog: next === 'backlog' });
	}

	/** A property menu closes, then makes its change. */
	function choose(close: () => void, change: () => Promise<void>) {
		close();
		void change();
	}

	let statusOpen = $state(false);
	let addingOpen = $state(false);
	let underOpen = $state(false);

	// ── what was added ─────────────────────────────────────────────────────
	// Chats come from the thread's own detail: every chat whose `thread_id` is
	// this one. The session list holds only the box's most recent chats, so it
	// is only the fallback for a box older than `detail.chats`.
	const chats = $derived.by<ProjectChat[]>(() => {
		if (detail?.chats) return detail.chats;
		return chatSessions.sessions
			.filter((s) => s.thread_id === threadId)
			.map((s) => ({
				id: s.conversation_id,
				title: s.title ?? '',
				icon: s.icon,
				message_count: s.message_count,
				last_message_at: s.last_message_at || s.first_message_at,
			}));
	});

	const memberItems = $derived((detail?.items ?? []).filter((i) => !i.url.startsWith('/chat/')));
	let statusOverride = $state<Record<string, string>>({});

	/** A file's text extraction, in the owner's words, when it needs saying. */
	function fileNote(url: string, role?: string, stored?: string): string | null {
		if (role === 'manuscript') return 'Your draft';
		const s = statusOverride[url] ?? stored;
		if (s === 'pending' || s === 'extracting') return 'Reading…';
		if (s === 'no_text') return 'No text layer';
		if (s === 'failed') return 'failed';
		return null;
	}

	/** Open a member beside the thread, so the thread stays in reach. */
	function openUrl(url: string) {
		if (url.startsWith('http://') || url.startsWith('https://')) {
			void openWebLink(url);
			return;
		}
		windowShellStore.openRouteBeside(url);
	}

	async function setRole(url: string, role: ProjectItemRole) {
		const id = threadId;
		if (!id) return;
		try {
			await projectStore.setItemRole(id, url, role);
		} catch (e) {
			console.error('[ThreadView] role change failed:', e);
			toast.error("Your server couldn't change that", { description: 'Nothing changed. Try again' });
		}
	}

	async function retryExtraction(url: string) {
		const fileId = url.split('/')[2];
		if (!fileId) return;
		try {
			await reextractDriveFile(fileId);
			statusOverride = { ...statusOverride, [url]: 'pending' };
		} catch {
			/* the note stays; the next open refreshes it */
		}
	}

	function memberMenu(url: string, role: string | undefined, e: MouseEvent) {
		e.preventDefault();
		const items: ContextMenuItem[] = [
			{ id: 'open', label: 'Open', icon: 'mynaui:external-link', action: () => openUrl(url) },
		];
		if (url.startsWith('/page/') && !archived) {
			const draft = role === 'manuscript';
			items.push({
				id: 'draft',
				label: draft ? 'Use as a source' : 'Mark as your draft',
				description: draft ? 'The assistant can draw on it again' : "The assistant won't cite it back to you",
				icon: draft ? 'mynaui:book-open' : 'mynaui:pen',
				dividerBefore: true,
				action: () => setRole(url, draft ? 'library' : 'manuscript'),
			});
		}
		const elsewhere = getProjectMenuItems(url).map((i) => ({
			...i,
			submenu: i.submenu?.filter((s) => s.id !== 'remove-from-project'),
		}));
		items.push(...elsewhere);
		if (!archived && detail) {
			const d = detail;
			items.push({
				id: 'remove',
				label: 'Remove from thread',
				icon: 'mynaui:x',
				dividerBefore: true,
				variant: 'destructive',
				action: () => void removeFromProject(d, url),
			});
		}
		contextMenu.show({ x: e.clientX, y: e.clientY }, items);
	}

	// ── ask about it ───────────────────────────────────────────────────────
	let askDraft = $state('');
	function submitAsk(e: Event) {
		e.preventDefault();
		const text = askDraft.trim();
		const id = threadId;
		if (!text || !id) return;
		// The chat opens in the other pane, so the thread stays beside it.
		askVirtues(text, id, { beside: true });
		askDraft = '';
	}

	// ── dropping on the page: files upload, rows file themselves ──────────
	let dropActive = $state(false);
	function acceptsDrop(data: DataTransfer): boolean {
		return !archived && (hasFiles(data) || data.types.includes(REF_DRAG_TYPE));
	}
	async function handleDrop(e: DragEvent) {
		const id = threadId;
		const ref = droppedRefUrl(e);
		if (ref) {
			if (detail) await fileIntoProject(detail, ref);
			return;
		}
		const dropped = e.dataTransfer?.files;
		if (!id || !dropped || dropped.length === 0) return;
		const files = [...dropped];
		// An upload can take a while and the row only appears at the end, so
		// the drop says it was taken before it says it is done.
		const pending = toast.loading(files.length === 1 ? `Adding ${files[0].name}…` : `Adding ${files.length} files…`);
		let failed = 0;
		for (const f of files) {
			try {
				const uploaded = await uploadDriveFile('uploads', f);
				await addProjectItem(id, `/drive/${uploaded.id}`);
			} catch (err) {
				failed += 1;
				console.error('[ThreadView] drop-add failed:', err);
			}
		}
		toast.dismiss(pending);
		if (failed > 0) {
			toast.error(
				failed === 1 && files.length === 1
					? `Your server couldn't add ${files[0].name}`
					: `Your server couldn't add ${failed} of ${files.length} files`,
				{ description: 'Try dropping them again' },
			);
		}
		await Promise.all([load(true), projectStore.load()]);
	}

	let pickerPos = $state<{ x: number; y: number } | null>(null);
	function openPicker(e: MouseEvent) {
		pickerPos = { x: e.clientX, y: e.clientY };
	}
	async function addMember(entity: { url: string }) {
		pickerPos = null;
		// Only what can be filed: not a thread, not Setup or the interview.
		const url = projectMemberUrl(entity.url);
		if (!url || !detail) return;
		await fileIntoProject(detail, url);
	}

	// ── the thread as a line ───────────────────────────────────────────────
	type EntryKind = 'start' | 'record' | 'task' | 'request';
	/** The line's filters. "What happened" is your records: events, messages, purchases. */
	type LineFilter = 'chats' | 'pages' | 'tasks' | 'files' | 'happened';
	const FILTER_LABELS: Record<LineFilter, string> = {
		chats: 'Chats',
		pages: 'Pages',
		tasks: 'Tasks',
		files: 'Files',
		happened: 'What happened',
	};
	let lineFilter = $state<LineFilter | null>(null);

	function memberGroup(url: string): LineFilter | undefined {
		if (url.startsWith('/record/')) return 'happened';
		if (url.startsWith('/page/')) return 'pages';
		if (url.startsWith('/drive/')) return 'files';
		return undefined;
	}
	interface LineEntry {
		key: string;
		/** Its local day, `YYYY-MM-DD`; null for a task with no date. */
		date: string | null;
		/** Orders entries within a day. */
		at: string;
		label: string;
		kind: EntryKind;
		icon: string;
		/** What it is, in a few words: "Calendar event", "Page", "Done". */
		meta: string;
		/** The filter it answers to; none for the start and for things with no filter of their own. */
		group?: LineFilter;
		task?: Task;
		/** A member's url, for its menu and its reading note. */
		url?: string;
		role?: string;
		fileStatus?: string;
		open?: () => void;
	}

	/** What a member is, from its route. */
	function memberKind(url: string): string {
		if (url.startsWith('/record/calendar_event/')) return 'Calendar event';
		if (url.startsWith('/record/')) return 'Record';
		if (/^https?:\/\//.test(url)) return 'Link';
		const kinds: Record<string, string> = { page: 'Page', drive: 'File', person: 'Person', place: 'Place', org: 'Organization', thing: 'Thing', day: 'Day' };
		return kinds[url.split('/')[1] ?? ''] ?? 'Added';
	}

	function localDay(d: Date): string {
		const m = String(d.getMonth() + 1).padStart(2, '0');
		const day = String(d.getDate()).padStart(2, '0');
		return `${d.getFullYear()}-${m}-${day}`;
	}
	const today = localDay(new Date());

	/** A task opens in a tab, as it does from every list. */
	function openTask(t: Task) {
		windowShellStore.openTabFromRoute(`/task/${t.id}`, { label: t.title, focusExisting: true });
	}

	/** A task homed here, or homed elsewhere and also about this thread. */
	function inThread(t: Task): 'home' | 'also' | null {
		const home = homeOf(t);
		if (home?.kind === 'thread' && home.id === threadId) return 'home';
		if ((t.also_about ?? []).some((l) => l.kind === 'thread' && l.id === threadId)) return 'also';
		return null;
	}

	const doneTasks = $derived(tasksStore.closed.filter((t) => t.status === 'done' && t.closed_at && inThread(t)));

	function taskEntry(t: Task): LineEntry {
		const request = isRequest(t);
		const also = inThread(t) === 'also';
		const done = t.status === 'done';
		let meta = request ? `Asked of the assistant · ${requestState(t)}` : done ? 'Done' : 'Task';
		if (also) meta += ' · Also about this thread';
		const date = done && t.closed_at ? localDay(new Date(t.closed_at)) : t.due_on;
		return {
			key: t.id,
			date,
			at: done ? (t.closed_at ?? '') : (t.due_at ?? `${t.due_on ?? ''}T99`),
			label: t.title,
			kind: request ? 'request' : 'task',
			icon: request ? 'mynaui:sparkles' : 'mynaui:circle',
			meta,
			group: 'tasks',
			task: t,
			open: () => openTask(t),
		};
	}

	const line = $derived.by(() => {
		if (!detail) return { past: [] as LineEntry[], ahead: [] as LineEntry[], filters: [] as LineFilter[] };
		const entries: LineEntry[] = [
			{
				key: 'start',
				date: localDay(new Date(detail.created_at)),
				// First on its day, before anything added that day.
				at: '',
				label: 'You started this thread',
				kind: 'start',
				icon: THREAD_ICON,
				meta: '',
			},
			...memberItems.map((i) => {
				const when = i.occurred_at ?? i.added_at;
				return {
					key: i.url,
					date: localDay(new Date(when)),
					at: when,
					label: memberName(i),
					kind: 'record' as const,
					icon: memberIcon(i.url),
					meta: memberKind(i.url),
					group: memberGroup(i.url),
					url: i.url,
					role: i.role,
					fileStatus: i.status,
					open: () => openUrl(i.url),
				};
			}),
			...chats.map((c) => ({
				key: `/chat/${c.id}`,
				date: localDay(new Date(c.last_message_at)),
				at: c.last_message_at,
				label: c.title || 'Untitled chat',
				kind: 'record' as const,
				icon: c.icon || 'mynaui:message',
				meta: 'Chat',
				group: 'chats' as const,
				url: `/chat/${c.id}`,
				open: () => openUrl(`/chat/${c.id}`),
			})),
			...doneTasks.map(taskEntry),
		];
		entries.sort(byDay);
		const shown = lineFilter ? entries.filter((e) => e.group === lineFilter) : entries;
		return {
			past: shown.filter((e) => (e.date ?? '') <= today),
			ahead: shown.filter((e) => (e.date ?? '') > today),
			// Only the kinds this thread has get a filter.
			filters: (Object.keys(FILTER_LABELS) as LineFilter[]).filter((f) => entries.some((e) => e.group === f)),
		};
	});

	function byDay(a: LineEntry, b: LineEntry): number {
		return (a.date ?? '').localeCompare(b.date ?? '') || a.at.localeCompare(b.at);
	}

	/** A long past folds to its latest entries until you ask for the rest. */
	const PAST_SHOWN = 8;
	let showAllPast = $state(false);
	const pastShown = $derived(showAllPast ? line.past : line.past.slice(-PAST_SHOWN));
	const pastHidden = $derived(line.past.length > pastShown.length);
	const lineEmpty = $derived(!lineFilter && line.past.length <= 1 && line.ahead.length === 0);

	function shortDay(iso: string | null): string {
		if (!iso) return '';
		const d = new Date(`${iso}T12:00:00`);
		const sameYear = d.getFullYear() === new Date().getFullYear();
		return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: sameYear ? undefined : 'numeric' });
	}

	function longDay(iso: string): string {
		return shortDay(localDay(new Date(iso)));
	}
</script>

<div
	class="thread-page"
	use:dropZone={{
		accepts: acceptsDrop,
		onActive: (on) => (dropActive = on),
		onDrop: handleDrop,
		// A sidebar row allows only a link, and a drop whose effect it does
		// not allow never fires.
		effect: (data) => (data.types.includes(REF_DRAG_TYPE) ? 'link' : 'copy'),
	}}
>
	{#if loading && !detail}
		<div class="state"><Icon icon="mynaui:spinner" width="18" class="spin" /> Loading…</div>
	{:else if error}
		<div class="state">{error} <TextAction inline onclick={() => load(true)}>Try again</TextAction></div>
	{:else if detail}
		<DetailFrame>
			<main class="main">
				<ThreadHeader thread={detail} />

				<section class="stands-block" aria-label="Where it stands">
					{#if standsEditing}
						<!-- svelte-ignore a11y_autofocus -->
						<textarea
							class="stands stands-input"
							bind:value={standsDraft}
							rows="1"
							autofocus
							placeholder="Where is this now? One sentence is enough."
							aria-label="Where it stands"
							onblur={saveStands}
							onkeydown={(e) => {
								if (e.key === 'Enter' && !e.shiftKey) {
									e.preventDefault();
									(e.currentTarget as HTMLTextAreaElement).blur();
								} else if (e.key === 'Escape') {
									standsDraft = detail?.current_status ?? '';
									standsEditing = false;
								}
							}}
						></textarea>
					{:else if detail.current_status}
						<p class="stands">{detail.current_status}</p>
						<p class="stands-meta">
							{detail.current_status_at ? `Written ${longDay(detail.current_status_at)}` : 'Written by you'}
							{#if !archived}· <TextAction inline onclick={() => (standsEditing = true)}>Edit</TextAction>{/if}
						</p>
					{:else if !archived}
						<TextAction onclick={() => (standsEditing = true)}>Say where this stands</TextAction>
					{/if}
				</section>

				{#each notes as n (n.key)}
					<NoteBox note={n} here={`/thread/${detail.id}`} />
				{/each}

				{#if !archived}
					<!-- The way into a new chat about this thread, shaped like the
					     composer it leads to so it reads as a place to type. -->
					<form class="ask" onsubmit={submitAsk}>
						<input
							class="ask-input"
							bind:value={askDraft}
							placeholder={`Ask about ${detail.name}…`}
							aria-label={`Ask about ${detail.name}`}
						/>
						<span class="ask-send" class:idle={!askDraft.trim()}>
							<IconButton icon="mynaui:arrow-up" label="Ask" type="submit" disabled={!askDraft.trim()} />
						</span>
					</form>
					<div class="ask-more">
						<TextAction onclick={() => newChatInProject(detail, { beside: true })}>New chat here</TextAction>
					</div>
				{/if}

				<section aria-labelledby="tasks-h">
					<div class="block-head">
						<h2 id="tasks-h" class="block-title">Tasks</h2>
						{#if !archived}
							<TextAction onclick={() => newTaskDialog.open({ home: { kind: 'thread', id: detail.id } })}>New task</TextAction>
						{/if}
					</div>
					<div class="tasks">
						<TaskList
							{active}
							route={tab.route}
							projectId={detail.id}
							readonly={archived}
							hideNew
							emptyText={archived ? 'No open tasks were left.' : 'No open tasks. New task adds one.'}
						/>
					</div>
				</section>

				<section aria-labelledby="line-h">
					<div class="block-head">
						<h2 id="line-h" class="block-title">The thread</h2>
						{#if !archived}
							<TextAction onclick={openPicker}>Add</TextAction>
						{/if}
					</div>

					{#if line.filters.length > 1}
						<div class="filters" role="group" aria-label="Show on the line">
							<button type="button" class="filter" aria-pressed={lineFilter === null} onclick={() => (lineFilter = null)}>Everything</button>
							{#each line.filters as f (f)}
								<button type="button" class="filter" aria-pressed={lineFilter === f} onclick={() => (lineFilter = f)}>{FILTER_LABELS[f]}</button>
							{/each}
						</div>
					{/if}

					<ol class="line">
						{#if pastHidden}
							<li class="entry fold">
								<span class="date"></span>
								<span class="node quiet"></span>
								<TextAction onclick={() => (showAllPast = true)}>Show earlier</TextAction>
							</li>
						{/if}
						{#each pastShown as e, i (e.key)}
							{@render entry(e, false, i === 0 || pastShown[i - 1].date !== e.date)}
						{/each}
						<li class="entry today" aria-label="Today">
							<span class="date">Today</span>
							<span class="today-dot"></span>
							<span class="today-rule"></span>
						</li>
						{#each line.ahead as e, i (e.key)}
							{@render entry(e, true, i === 0 || line.ahead[i - 1].date !== e.date)}
						{/each}
					</ol>
					{#if lineEmpty && !archived}
						<p class="quiet">Chats, pages and files you add, and tasks you finish, line up here by day. Drop anything on this page to add it.</p>
					{/if}
				</section>
			</main>

			<aside class="side" aria-label="About this thread">
				<section class="props" aria-label="Properties">
					<div class="prop">
						<Popover bind:open={statusOpen} placement="bottom-start" offset={4}>
							{#snippet trigger({ toggle })}
								<PreviewRow icon={STATUS_ICON[status]} onclick={toggle}>{statusText}</PreviewRow>
							{/snippet}
							{#snippet children({ close })}
								<div class="menu" role="menu" aria-label="Status">
									<MenuItem role="menuitemradio" icon={STATUS_ICON.active} checked={status === 'active'} label="Active"
										description="It shows in your week and on the Overview."
										onclick={() => choose(close, () => setStatus('active'))} />
									<MenuItem role="menuitemradio" icon={STATUS_ICON.backlog} checked={status === 'backlog'} label="Backlog"
										description="It waits until you come back to it."
										onclick={() => choose(close, () => setStatus('backlog'))} />
									<div class="gap"></div>
									<MenuItem role="menuitemradio" icon={STATUS_ICON.finished} checked={status === 'finished'} label="Finished"
										description="You did what it was for."
										onclick={() => choose(close, () => setStatus('finished'))} />
									<MenuItem role="menuitemradio" icon={STATUS_ICON.dropped} checked={status === 'dropped'} label="Dropped"
										description="You stopped. Everything in it stays."
										onclick={() => choose(close, () => setStatus('dropped'))} />
								</div>
							{/snippet}
						</Popover>
					</div>

					<div class="prop">
						<Popover bind:open={addingOpen} placement="bottom-start" offset={4}>
							{#snippet trigger({ toggle })}
								<PreviewRow
									icon={detail.is_adding_automatic ? 'mynaui:sparkles' : 'mynaui:hand'}
									onclick={archived ? undefined : toggle}
								>
									{detail.is_adding_automatic ? 'Adding: Automatic' : 'Adding: Manual'}
								</PreviewRow>
							{/snippet}
							{#snippet children({ close })}
								<div class="menu" role="menu" aria-label="Adding">
									<MenuItem role="menuitemradio" icon="mynaui:hand" checked={!detail.is_adding_automatic} label="Manual"
										description="Only what you add is filed here."
										onclick={() => choose(close, () => setProps({ is_adding_automatic: false }))} />
									<MenuItem role="menuitemradio" icon="mynaui:sparkles" checked={!!detail.is_adding_automatic} label="Automatic"
										description="Your server also files what it is sure belongs here, and asks about the rest."
										onclick={() => choose(close, () => setProps({ is_adding_automatic: true }))} />
								</div>
							{/snippet}
						</Popover>
					</div>

					<div class="prop">
						<Popover bind:open={underOpen} placement="bottom-start" offset={4}>
							{#snippet trigger({ toggle })}
								<PreviewRow
									icon="atlas:things"
									quiet={!detail.thing_id}
									title="A thing goes on and can't be finished, like a garden or a bike. A thread can sit under one."
									onclick={archived ? undefined : toggle}
								>
									{detail.thing_title ? `Under ${detail.thing_title}` : 'Not under a thing'}
								</PreviewRow>
							{/snippet}
							{#snippet children({ close })}
								<div class="menu" role="menu" aria-label="The thing this thread sits under">
									<MenuItem role="menuitemradio" checked={!detail.thing_id} label="Not under a thing"
										onclick={() => choose(close, () => setProps({ thing_id: null }))} />
									{#each thingsStore.things as th (th.id)}
										<MenuItem role="menuitemradio" icon="atlas:things" checked={detail.thing_id === th.id} label={th.title}
											onclick={() => choose(close, () => setProps({ thing_id: th.id }))} />
									{/each}
									{#if thingsStore.loaded && thingsStore.things.length === 0}
										<p class="menu-note">Name a thing in your wiki to put this thread under it.</p>
									{/if}
								</div>
							{/snippet}
						</Popover>
					</div>

					{#if people.length > 0}
						<PreviewRow icon="atlas:people">
							<span class="with">
								{#each people.slice(0, 6) as person, i (person.url)}
									<TextAction inline onclick={() => openUrl(person.url)}>{person.name}</TextAction>{i < Math.min(people.length, 6) - 1 ? ', ' : ''}
								{/each}
							</span>
						</PreviewRow>
					{/if}
				</section>

				{#if weekDays && !archived}
					<ThreadWeekBars days={weekDays} today={week?.today ?? null} {color} />
				{/if}

				{#if !archived}
					<ThreadUpNext threadId={detail.id} {detail} tasks={0} />
				{/if}
			</aside>

			{#if dropActive}
				<div class="drop-over" aria-hidden="true">
					<span class="drop-card"><Icon icon="mynaui:upload" width="16" />Drop to add it to {detail.name}</span>
				</div>
			{/if}
		</DetailFrame>
	{:else}
		<div class="state">Thread not found.</div>
	{/if}
</div>

{#snippet entry(e: LineEntry, ahead: boolean, showDate: boolean)}
	{@const note = e.url ? fileNote(e.url, e.role, e.fileStatus) : null}
	<li class="entry" class:ahead class:start={e.kind === 'start'}>
		<span class="date">{showDate ? shortDay(e.date) : ''}</span>
		{#if e.kind === 'task' && e.task}
			{@const t = e.task}
			<span class="node check">
				<TaskCheck
					done={t.status === 'done'}
					label={t.status === 'done' ? `Mark ${t.title} not done` : `Complete ${t.title}`}
					onclick={() => toggleTaskDone(t)}
				/>
			</span>
		{:else if e.kind === 'start' && detail}
			<span class="node tinted" style={`--tile: ${color}`}><ProjectGlyph project={detail} size={14} inherit /></span>
		{:else}
			<span class="node" class:request={e.kind === 'request'}><Icon icon={e.icon} width="14" /></span>
		{/if}
		{#if e.open}
			<button
				type="button"
				class="entry-label"
				class:done={e.task?.status === 'done'}
				onclick={e.open}
				oncontextmenu={e.url ? (ev) => memberMenu(e.url!, e.role, ev) : undefined}
			>
				<span class="entry-title">{e.label}</span>
				<span class="entry-meta">{e.meta}{note && note !== 'failed' ? ` · ${note}` : ''}</span>
			</button>
		{:else}
			<span class="entry-label static"><span class="entry-title">{e.label}</span></span>
		{/if}
		{#if note === 'failed' && e.url}
			{@const url = e.url}
			<span class="retry"><TextAction inline onclick={() => retryExtraction(url)}>Reading failed. Try again</TextAction></span>
		{/if}
	</li>
{/snippet}

{#if pickerPos}
	<RefPicker
		mode="single"
		position={pickerPos}
		placeholder="Add a chat, page, person, or link…"
		excludeIds={memberItems.map((i) => i.url)}
		filter={(e) => !!projectMemberUrl(e.url)}
		onSelect={addMember}
		onClose={() => (pickerPos = null)}
	/>
{/if}

<style>
	.thread-page {
		width: 100%;
		height: 100%;
		overflow-y: auto;
	}
	.main {
		display: flex;
		flex-direction: column;
		gap: 32px;
		min-width: 0;
	}
	.state {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 48px 32px;
		color: var(--color-foreground-muted);
	}

	/* Where it stands: the box's sentence, in the serif at the lead size. */
	.stands-block {
		margin-top: -8px;
	}
	.stands {
		display: block;
		width: 100%;
		margin: 0;
		padding: 0;
		border: 0;
		background: transparent;
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: var(--text-lead);
		line-height: 28px;
		color: var(--color-foreground);
	}
	.stands-input {
		resize: none;
		outline: none;
		field-sizing: content;
	}
	.stands-input::placeholder {
		color: var(--color-foreground-subtle);
	}
	.stands-meta {
		margin: 4px 0 0;
		font-size: var(--text-small);
		line-height: 20px;
		color: var(--color-foreground-muted);
	}

	/* The ask: shaped like the composer it opens. */
	.ask {
		display: flex;
		align-items: center;
		gap: 8px;
		min-height: 44px;
		padding: 4px 4px 4px 16px;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-xl);
		background: var(--color-surface);
	}
	.ask-input {
		flex: 1;
		min-width: 0;
		border: none;
		background: transparent;
		outline: none;
		font: inherit;
		font-size: var(--text-body);
		color: var(--color-foreground);
	}
	.ask-input::placeholder {
		color: var(--color-foreground-subtle);
	}
	.ask-send {
		display: inline-flex;
		transition: opacity var(--motion-hover-out) var(--ease-premium);
	}
	/* Opacity alone, so it fades out as well as in; the button is disabled
	   while the field is empty, so it takes no click or focus. */
	.ask-send.idle {
		opacity: 0;
		pointer-events: none;
	}

	.ask-more {
		display: flex;
		justify-content: flex-end;
		margin-top: 8px;
	}
	.filters {
		display: flex;
		flex-wrap: wrap;
		gap: 2px;
		margin: 4px 0 12px -10px;
	}
	.filter {
		height: 28px;
		padding: 0 10px;
		border: 0;
		border-radius: var(--radius-md);
		background: transparent;
		color: var(--color-foreground-muted);
		font: inherit;
		font-size: var(--text-sm, 13px);
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.filter:hover {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}
	.filter[aria-pressed='true'] {
		background: var(--active-bg);
		color: var(--color-foreground);
	}
	.block-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		min-height: 32px;
	}
	.block-title {
		margin: 0;
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: var(--text-heading);
		line-height: 32px;
		color: var(--color-foreground);
	}
	.tasks {
		margin-top: 8px;
	}
	.quiet {
		margin: 8px 0 0 96px;
		font-size: var(--text-small);
		line-height: 20px;
		color: var(--color-foreground-muted);
	}

	/* The thread as a line: dates on the left, a typed node for each entry,
	   solid down to today and dashed after it. */
	.line {
		list-style: none;
		margin: 16px 0 0;
		padding: 0;
	}
	.entry {
		position: relative;
		display: grid;
		grid-template-columns: 64px 24px minmax(0, 1fr);
		align-items: center;
		column-gap: 8px;
		min-height: 48px;
	}
	.entry::before {
		content: '';
		position: absolute;
		left: calc(64px + 8px + 12px);
		top: 0;
		bottom: 0;
		border-left: 2px solid var(--color-border);
		transform: translateX(-1px);
	}
	.entry.ahead::before {
		border-left-style: dashed;
	}
	.entry:first-child::before {
		top: 50%;
	}
	.entry:last-child::before {
		bottom: 50%;
	}
	.date {
		font-size: var(--text-small);
		line-height: 20px;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
		text-align: right;
		padding-right: 4px;
	}
	.node {
		position: relative;
		z-index: 1;
		display: inline-grid;
		place-items: center;
		width: 24px;
		height: 24px;
		border-radius: 50%;
		box-sizing: border-box;
		border: 1px solid var(--color-border);
		background: var(--color-page);
		color: var(--color-foreground-muted);
	}
	.node.quiet {
		width: 8px;
		height: 8px;
		justify-self: center;
		border: 0;
		background: var(--color-border);
	}
	.node.check {
		border: 0;
	}
	.node.request {
		color: var(--color-primary);
	}
	.node.tinted {
		border: 0;
		background: color-mix(in srgb, var(--tile) 16%, var(--color-page));
		color: var(--tile);
	}
	.entry.today {
		min-height: 32px;
	}
	.entry.today .date {
		color: var(--color-primary);
		font-weight: 500;
	}
	.today-dot {
		position: relative;
		z-index: 1;
		justify-self: center;
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--color-primary);
	}
	/* Round dots, as the week chart rings today. */
	.today-rule {
		height: 2px;
		background-image: radial-gradient(circle, var(--color-primary) 1px, transparent 1.5px);
		background-size: 8px 2px;
		background-repeat: repeat-x;
		opacity: 0.7;
	}
	.entry-label {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		min-width: 0;
		padding: 4px 8px;
		border: 0;
		border-radius: var(--radius-md);
		background: transparent;
		font: inherit;
		text-align: left;
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.entry-label:hover {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}
	.entry-label.static {
		cursor: default;
	}
	.entry-label.static:hover {
		background: transparent;
	}
	.entry-title {
		max-width: 100%;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: var(--text-label);
		line-height: 20px;
		color: var(--color-foreground);
	}
	.entry.start .entry-title {
		color: var(--color-foreground-muted);
	}
	.entry-label.done .entry-title {
		color: var(--color-foreground-muted);
	}
	.entry-meta {
		font-size: var(--text-caption);
		line-height: 16px;
		color: var(--color-foreground-muted);
	}
	.entry-meta:empty {
		display: none;
	}
	.retry {
		grid-column: 3;
		padding: 0 8px 8px;
		font-size: var(--text-caption);
	}
	.entry.fold {
		min-height: 40px;
	}
	.entry.fold :global(.v-textaction) {
		padding-left: 8px;
	}

	/* The side column: properties as rows, then the week and what is next. */
	.side {
		display: flex;
		flex-direction: column;
		gap: 32px;
		padding-top: 4px;
	}
	.props {
		display: flex;
		flex-direction: column;
	}
	.prop :global(.popover-trigger) {
		display: block;
	}
	.menu {
		display: flex;
		flex-direction: column;
		min-width: 240px;
		max-width: 320px;
		max-height: 360px;
		overflow-y: auto;
		padding: 4px;
	}
	.gap {
		height: 4px;
	}
	.menu-note {
		margin: 0;
		padding: 8px;
		font-size: var(--text-small);
		line-height: 20px;
		color: var(--color-foreground-muted);
	}
	.with {
		display: block;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	/* Anything dragged over the page lands in the thread. */
	.drop-over {
		position: absolute;
		inset: 16px;
		display: grid;
		place-items: center;
		border: 2px dashed var(--drop-line);
		border-radius: var(--radius-xl);
		background: var(--drop-tint);
		pointer-events: none;
		animation: drop-in var(--motion-hover-in) var(--ease-premium) both;
		z-index: 2;
	}
	.drop-card {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		padding: 8px 16px;
		border-radius: var(--radius-full);
		background: var(--color-surface);
		box-shadow: var(--shadow-float);
		font-size: var(--text-label);
		color: var(--color-foreground);
	}
	@keyframes drop-in {
		from {
			opacity: 0;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.ask-send,
		.entry-label {
			transition: none;
		}
		.drop-over {
			animation: none;
		}
	}
</style>
