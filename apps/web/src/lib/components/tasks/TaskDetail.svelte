<script lang="ts">
	/**
	 * One task, whole: on its own page (TaskView) and in the Threads door's
	 * Tasks view beside the list, as a mail app shows a message. The main
	 * column is the task: its check and title, its description, any note on
	 * it, where it came from, a request's result, then its activity: its
	 * history from `app_task_events`, each line signed by who did it, with
	 * your own notes among it and the note field under it. The side column
	 * holds its home, what else it is about, its due day, how often it
	 * repeats, and who it is for, as icon rows. A closed task is read-only
	 * until it is reopened, its check aside.
	 *
	 * `layout="page"` sits in the page frame with the side column.
	 *
	 * `"pane"` fits the column beside a list (the Inbox) in two columns, as
	 * an issue tracker lays out an issue. The wide one is the story: the
	 * title, where the assistant has got to when it has the task, the
	 * description, then what happened, oldest first, ending in the box for
	 * writing more. The narrow one holds every property, grouped and named:
	 * its status, who does it, its day, how it repeats, the model and cost of
	 * a request, its thread and what else it is about. Handing a task to the
	 * assistant is choosing it as who does it. In a narrow pane the
	 * properties sit under the title.
	 */
	import type { Task, TaskEvent, TaskPatch } from '$lib/api/client';
	import { ApiError, addTaskNote, getTask, listTaskEvents, updateTask } from '$lib/api/client';
	import { Button } from '$lib';
	import RefCard from '$lib/components/RefCard.svelte';
	import { modelSlots } from '$lib/stores/modelSlots.svelte';
	import { pagesStore } from '$lib/stores/pages.svelte';
	import { formatMicrosUSD } from '$lib/utils/currency';
	import { chatPrompt, dueFact, standing } from '$lib/tasks/standing';
	import { requestState } from '$lib/tasks/requests';
	import ReaderChat from '$lib/components/inbox/ReaderChat.svelte';
	import PaneColumns from '$lib/components/PaneColumns.svelte';
	import Folded from '$lib/components/Folded.svelte';
	import SideGroup from '$lib/components/SideGroup.svelte';
	import TextAction from '$lib/components/TextAction.svelte';
	import DetailFrame from '$lib/components/DetailFrame.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import Markdown from '$lib/components/Markdown.svelte';
	import Popover from '$lib/floating/primitives/Popover.svelte';
	import TaskCheck from '$lib/components/tasks/TaskCheck.svelte';
	import TaskComposer from '$lib/components/tasks/TaskComposer.svelte';
	import DuePicker from '$lib/components/tasks/DuePicker.svelte';
	import TaskHome from '$lib/components/tasks/TaskHome.svelte';
	import TaskAlsoAbout from '$lib/components/tasks/TaskAlsoAbout.svelte';
	import TaskActions from '$lib/components/tasks/TaskActions.svelte';
	import MenuItem from '$lib/components/MenuItem.svelte';
	import PreviewRow from '$lib/components/threads/PreviewRow.svelte';
	import { projectColor } from '$lib/sidebar/pin-colors';
	import { homeOf, subjectIcon, subjectName } from '$lib/tasks/homes';
	import { THREAD_ICON } from '$lib/utils/iconHelpers';
	import TaskSource from '$lib/components/tasks/TaskSource.svelte';
	import NoteBox from '$lib/components/threads/NoteBox.svelte';
	import { notesStore } from '$lib/stores/notes.svelte';
	import { tasksStore } from '$lib/stores/tasks.svelte';
	import { projectStore } from '$lib/stores/project.svelte';
	import { identityOf } from '$lib/refs/identity.svelte';
	import { isClosed, toggleTaskDone } from '$lib/tasks/actions';
	import { dueLabel } from '$lib/tasks/due';
	import { RECUR_CHOICES, recurLabel, recurrenceOf, type Recurrence } from '$lib/tasks/recur';
	import { isRepeating, isRequest } from '$lib/tasks/requests';
	import { eventLine, eventTime } from '$lib/tasks/history';
	import { toast } from 'svelte-sonner';
	import { untrack } from 'svelte';

	let {
		taskId,
		layout = 'page',
		current = $bindable(null),
	}: {
		taskId: string | null;
		layout?: 'page' | 'pane';
		/** The task as shown, for a host that names its tab or path after it. */
		current?: Task | null;
	} = $props();

	// The store's copy when the list has it, so an edit here and in the list
	// stay one task; else this view's own read.
	let fetched = $state<Task | null>(null);
	const task = $derived((taskId ? tasksStore.find(taskId) : undefined) ?? (fetched?.id === taskId ? fetched : null));

	// Showing a task that repeats is opening it: its new result stops waiting.
	$effect(() => {
		if (task) void tasksStore.seen(task);
	});
	$effect(() => {
		current = task;
	});
	let error = $state<string | null>(null);
	let history = $state<TaskEvent[]>([]);

	let historyFailed = $state(false);

	/**
	 * The task, then its history. A failed history leaves the task on show
	 * and says so where the history goes. Beside a list the task changes as
	 * you move down it, so a late answer for the last one is dropped.
	 */
	let loadedId: string | null = null;
	async function load(id: string) {
		error = null;
		// The same task again (after an edit) keeps its activity on show while it reloads.
		if (id !== loadedId) {
			history = [];
			historyFailed = false;
			loadedId = id;
		}
		try {
			if (!untrack(() => tasksStore.find(id))) {
				const read = await getTask(id);
				if (id !== taskId) return;
				fetched = read;
			}
		} catch (e) {
			if (id !== taskId) return;
			console.error('[TaskDetail] Failed to load task:', e);
			error =
				e instanceof ApiError && e.status === 404
					? "This task isn't here. It may have been deleted."
					: "Your server couldn't open this task. Check that it's running, then try again.";
			return;
		}
		await reloadHistory(id);
	}

	async function reloadHistory(id = task?.id) {
		if (!id) return;
		try {
			const events = await listTaskEvents(id);
			if (id !== taskId) return;
			history = events;
			historyFailed = false;
		} catch (e) {
			if (id !== taskId) return;
			console.error('[TaskDetail] history failed:', e);
			historyFailed = true;
		}
	}

	$effect(() => {
		if (taskId) void load(taskId);
	});

	$effect(() => {
		notesStore.start();
		// The thread's name and color, for the path and the home.
		untrack(() => {
			if (!projectStore.loaded) void projectStore.load();
		});
		return () => notesStore.stop();
	});

	async function save(patch: TaskPatch, failure = "Your server couldn't change that") {
		if (!task) return;
		try {
			if (tasksStore.find(task.id)) await tasksStore.update(task.id, patch);
			else fetched = await updateTask(task.id, patch);
		} catch (e) {
			toast.error(failure, { description: e instanceof Error ? e.message : undefined });
			return;
		}
		// The change is made; a history that fails to reload says so in its own place.
		await reloadHistory();
	}

	async function toggle() {
		if (!task) return;
		if (tasksStore.find(task.id)) await toggleTaskDone(task);
		else await save({ status: isClosed(task) ? 'open' : 'done' });
		await reloadHistory();
	}

	function saveTitle(value: string) {
		const title = value.trim();
		if (task && title && title !== task.title) void save({ title }, "Your server couldn't rename that task");
	}

	function saveDescription(value: string) {
		const notes = value.trim() || null;
		if (task && notes !== (task.notes ?? null)) void save({ notes }, "Your server couldn't save that description");
	}

	/** Add one of the owner's own notes. Resolves true so the field clears. */
	async function addNote(value: string): Promise<boolean> {
		const text = value.trim();
		if (!task || !text) return false;
		try {
			await addTaskNote(task.id, text);
		} catch (e) {
			toast.error("Your server couldn't save that note", { description: e instanceof Error ? e.message : undefined });
			return false;
		}
		await reloadHistory();
		return true;
	}

	const notes = $derived(task ? notesStore.about(`/task/${task.id}`).slice(0, 2) : []);

	let dueOpen = $state(false);
	let recurOpen = $state(false);

	function saveRecurrence(recurs: Recurrence | null) {
		recurOpen = false;
		void save(
			{ recur_every: recurs?.every ?? null, recur_unit: recurs?.unit ?? null },
			"Your server couldn't change how that repeats",
		);
	}

	const sourceTitle = $derived(task?.source_url ? identityOf(task.source_url)?.title : undefined);
	const recurs = $derived(task ? recurrenceOf(task) : null);

	// ── beside a list ──────────────────────────────────────────────────────

	let statusOpen = $state(false);
	let whoOpen = $state(false);

	/** Where a task stands, in a word, for its Status property. */
	function statusLabel(t: Task): string {
		if (t.status === 'done') return 'Done';
		if (t.status === 'dismissed') return 'Put away';
		if (isRequest(t)) {
			const said = requestState(t);
			return said === 'Ready' ? 'Result ready' : said;
		}
		return 'Open';
	}
	function statusIcon(t: Task): string {
		if (t.status === 'done') return 'mynaui:check-circle';
		if (t.status === 'dismissed') return 'mynaui:archive';
		if (t.status === 'active') return 'mynaui:spinner';
		return 'mynaui:circle';
	}

	$effect(() => {
		if (layout === 'pane' && !modelSlots.loaded) untrack(() => void modelSlots.load());
	});
	// The model a request runs on, by name. Where that model runs is not
	// said here: the manual and Settings say it.
	const model = $derived(task?.run_model ? modelSlots.nameOf(task.run_model) : null);

	function dismiss() {
		void save({ status: 'dismissed' }, "Your server couldn't put that away");
		toast('Put away', {
			action: { label: 'Undo', onClick: () => void save({ status: 'open' }, "Your server couldn't bring that back") },
		});
	}

	function resultLabel(url: string): string {
		const id = url.slice(url.lastIndexOf('/') + 1);
		return pagesStore.pages.find((p) => p.id === id)?.title || 'The page it made';
	}

	/**
	 * What you write in the box that ends the task's history. To a result
	 * the assistant left, it is a reply, and the assistant works on it
	 * again. Otherwise it is a note in the history, which the assistant
	 * reads when it next runs. Resolves true so the box clears.
	 */
	async function send(text: string): Promise<boolean> {
		const said = text.trim();
		if (!task || !said) return false;
		if (task.assignee === 'assistant' && task.status === 'ready') {
			try {
				await tasksStore.reply(task.id, said);
			} catch (e) {
				toast.error("Your server couldn't send that reply", { description: e instanceof Error ? e.message : undefined });
				return false;
			}
			await reloadHistory();
			return true;
		}
		return addNote(said);
	}
</script>

{#if error}
	<div class="state">{error}</div>
{:else if !task}
	<div class="state"><Icon icon="mynaui:spinner" width="18" class="spin" /> Loading…</div>
{:else if layout === 'page'}
	<DetailFrame>{@render body(task)}</DetailFrame>
{:else}
	<div class="pane-grid">{@render paneBody(task)}</div>
{/if}

{#snippet titleRow(task: Task)}
	<div class="head">
		<!-- A task that repeats is never finished, so it has no box to tick. -->
		{#if !isRepeating(task)}
			<span class="check">
				<TaskCheck
					done={task.status === 'done'}
					label={isClosed(task) ? `Mark ${task.title} not done` : `Complete ${task.title}`}
					onclick={toggle}
				/>
			</span>
		{/if}
		{#key task.id}
			<textarea
				class="title"
				class:closed={isClosed(task)}
				value={task.title}
				rows="1"
				aria-label="Title"
				readonly={isClosed(task) || isRepeating(task)}
				onblur={(e) => saveTitle(e.currentTarget.value)}
				onkeydown={(e) => {
					if (e.key === 'Enter') {
						e.preventDefault();
						e.currentTarget.blur();
					}
				}}
			></textarea>
		{/key}
	</div>
{/snippet}

{#snippet historyList()}
	{#if historyFailed}
		<p class="quiet">
			Your server couldn't load its activity. <TextAction inline onclick={() => reloadHistory()}>Try again</TextAction>
		</p>
	{:else if history.length > 0}
		<ol class="history">
			{#each history as e (e.id)}
				{#if e.kind === 'note' && e.author === 'owner'}
					<li class="own">
						<span class="dot" aria-hidden="true"></span>
						<span class="line">You wrote</span>
						<span class="when">{eventTime(e)}</span>
						<p class="own-text">{String(e.body.text ?? '')}</p>
					</li>
				{:else}
					<li>
						<span class="dot" aria-hidden="true"></span>
						<span class="line">{eventLine(e, sourceTitle)}</span>
						<span class="when">{eventTime(e)}</span>
					</li>
				{/if}
			{/each}
		</ol>
	{/if}
{/snippet}

{#snippet paneBody(task: Task)}
	{@const closed = isClosed(task)}
	{@const asked = isRequest(task)}
	{@const sentence = asked ? standing(task) : null}
	<PaneColumns sideLabel="About this task">
		{#snippet head()}{@render titleRow(task)}{/snippet}

		<!-- The story: where the assistant has got to, the description, what
		     happened, and the box for writing more. -->
		{#snippet main()}
			{#if sentence || isRepeating(task)}
				<section class="standing" aria-label="Where it stands">
					{#if sentence}<p class="sentence">{sentence}</p>{/if}
					{#if isRepeating(task)}
						<TaskActions {task} onChanged={reloadHistory} />
					{:else if asked && task.status === 'ready'}
						<div class="result">
							<Folded><Markdown content={task.result ?? ''} /></Folded>
							{#if task.result_url}
								<RefCard type={task.result_url.split('/')[1] ?? null} label={resultLabel(task.result_url)} url={task.result_url} />
							{/if}
						</div>
						<div class="moves">
							<Button onclick={() => save({ status: 'done' }, "Your server couldn't close that request")}>Accept</Button>
						</div>
					{:else if asked && task.status === 'open' && !task.next_run_at}
						<div class="moves">
							<Button onclick={() => save({ assignee: 'assistant' }, "Your server couldn't start that")}>Start</Button>
						</div>
					{/if}
				</section>
			{/if}

			<div class="described">
				{#if closed}
					{#if task.notes}<Markdown content={task.notes} />{/if}
				{:else}
					{#key task.id}
						<TaskComposer value={task.notes ?? ''} label="Description" placeholder="Add a description" commitOnBlur onCommit={saveDescription} />
					{/key}
				{/if}
			</div>

			{#each notes as n (n.key)}
				<NoteBox note={n} here={`/task/${task.id}`} />
			{/each}

			{#if task.source_url}
				<section class="block">
					<h2 class="block-title">Where it came from</h2>
					<TaskSource url={task.source_url} />
				</section>
			{/if}

			{#if task.result && closed}
				<section class="block">
					<h2 class="block-title">What the assistant left</h2>
					<div class="result"><Folded><Markdown content={task.result} /></Folded></div>
				</section>
			{/if}

			<section class="block">
				<h2 class="block-title">What happened</h2>
				{@render historyList()}
				<div class="write">
					<ReaderChat about={task.id} placeholder={chatPrompt(task)} onSend={send} />
				</div>
			</section>
		{/snippet}

		<!-- The properties, all in one place. One with no value says so quietly. -->
		{#snippet side()}
			<SideGroup label="Properties">
				<Popover bind:open={statusOpen} placement="bottom-start" offset={4}>
					{#snippet trigger({ toggle: toggleStatus })}
						<PreviewRow icon={statusIcon(task)} onclick={toggleStatus}>{statusLabel(task)}</PreviewRow>
					{/snippet}
					{#snippet children({ close })}
						<div class="menu" role="menu" aria-label="Where this task stands">
							<MenuItem role="menuitemradio" checked={!closed} label="Open" onclick={() => { close(); if (closed) void save({ status: 'open' }, "Your server couldn't reopen that"); }} />
							<MenuItem role="menuitemradio" checked={task.status === 'done'} label="Done" onclick={() => { close(); if (task.status !== 'done') void toggle(); }} />
							<MenuItem role="menuitemradio" checked={task.status === 'dismissed'} label="Put away" onclick={() => { close(); if (task.status !== 'dismissed') dismiss(); }} />
						</div>
					{/snippet}
				</Popover>
				<Popover bind:open={whoOpen} placement="bottom-start" offset={4}>
					{#snippet trigger({ toggle: toggleWho })}
						<PreviewRow icon={task.assignee === 'assistant' ? 'mynaui:sparkles' : 'mynaui:user'} onclick={closed ? undefined : toggleWho}>
							{task.assignee === 'assistant' ? 'The assistant' : 'You'}
						</PreviewRow>
					{/snippet}
					{#snippet children({ close })}
						<div class="menu" role="menu" aria-label="Who does this task">
							<MenuItem role="menuitemradio" checked={task.assignee !== 'assistant'} label="You" onclick={() => { close(); if (task.assignee === 'assistant') void save({ assignee: 'owner' }, "Your server couldn't take that back"); }} />
							<MenuItem
								role="menuitemradio"
								checked={task.assignee === 'assistant'}
								label="The assistant"
								disabled={!!recurs}
								onclick={() => { close(); if (task.assignee !== 'assistant') void save({ assignee: 'assistant' }, "Your server couldn't hand that over"); }}
							/>
							{#if recurs}<p class="menu-note">A task that repeats stays yours.</p>{/if}
						</div>
					{/snippet}
				</Popover>
				{#if !isRepeating(task)}
					<Popover bind:open={dueOpen} placement="bottom-start" offset={4}>
						{#snippet trigger({ toggle: toggleDue })}
							<PreviewRow icon="mynaui:calendar" quiet={!task.due_on} onclick={closed ? undefined : toggleDue}>
								{task.due_on ? dueFact(task.due_on, task.due_at) : 'Set a day'}
							</PreviewRow>
						{/snippet}
						{#snippet children({ close })}
							<DuePicker
								due_on={task.due_on}
								due_at={task.due_at}
								onPick={(on, at) => {
									void save({ due_on: on, due_at: at }, "Your server couldn't change that date");
									close();
								}}
							/>
						{/snippet}
					</Popover>
				{/if}
				{#if !asked && !isRepeating(task)}
					<Popover bind:open={recurOpen} placement="bottom-start" offset={4}>
						{#snippet trigger({ toggle: toggleRecur })}
							<PreviewRow icon="mynaui:refresh" quiet={!recurs} onclick={closed ? undefined : toggleRecur}>
								{recurs ? recurLabel(recurs.every, recurs.unit) : 'Does not repeat'}
							</PreviewRow>
						{/snippet}
						{#snippet children()}{@render recurMenu()}{/snippet}
					</Popover>
				{/if}
				{#if asked && model}
					<PreviewRow icon="mynaui:microchip">{model}</PreviewRow>
				{/if}
				{#if asked && task.spent_micros}
					<PreviewRow icon="mynaui:dollar">{formatMicrosUSD(task.spent_micros)} so far</PreviewRow>
				{/if}
			</SideGroup>
			<SideGroup label="Thread">{@render homeRow(task, closed)}</SideGroup>
			<SideGroup label="Linked">
				<PreviewRow icon="mynaui:link" quiet={(task.also_about ?? []).length === 0}>
					<TaskAlsoAbout {task} onSave={(p) => save(p)} readonly={closed} plain />
				</PreviewRow>
			</SideGroup>
		{/snippet}
	</PaneColumns>
{/snippet}

{#snippet homeRow(task: Task, closed: boolean)}
	{@const home = homeOf(task)}
	{@const homeThread = home?.kind === 'thread' ? [...projectStore.projects, ...projectStore.archived].find((p) => p.id === home.id) : undefined}
	<PreviewRow
		icon={homeThread ? homeThread.icon || THREAD_ICON : home ? subjectIcon(home.kind) : 'atlas:projects'}
		iconColor={homeThread ? projectColor(homeThread) : null}
		quiet={!home}
	>
		{#if closed}
			{home ? subjectName(home) : 'No thread'}
		{:else}
			<TaskHome {task} onSave={(p) => save(p, "Your server couldn't move that")} plain label={home ? undefined : 'Put it in a thread'} />
		{/if}
	</PreviewRow>
{/snippet}

{#snippet recurMenu()}
	<div class="menu" role="menu" aria-label="How often this task repeats">
		<MenuItem role="menuitemradio" checked={!recurs} label="Does not repeat" onclick={() => saveRecurrence(null)} />
		{#each RECUR_CHOICES as choice (choice.every + choice.unit)}
			<MenuItem
				role="menuitemradio"
				checked={recurs?.every === choice.every && recurs?.unit === choice.unit}
				label={recurLabel(choice.every, choice.unit)}
				onclick={() => saveRecurrence(choice)}
			/>
		{/each}
		<p class="menu-note">Counted from the day you finish it.</p>
	</div>
{/snippet}

{#snippet body(task: Task)}
			<main class="main">
				{@render titleRow(task)}

				<div class="lede">
					<!-- A closed task is read until it is reopened. -->
					{#if isClosed(task)}
						{#if task.notes}<div class="description"><Markdown content={task.notes} /></div>{/if}
					{:else}
						{#key task.id}
							<TaskComposer
								value={task.notes ?? ''}
								label="Description"
								placeholder="Add a description"
								commitOnBlur
								onCommit={saveDescription}
							/>
						{/key}
					{/if}
					<TaskActions {task} onChanged={reloadHistory} />
				</div>

				{#if layout === 'pane'}
					<section class="pane-props" aria-label="About this task">{@render properties(task)}</section>
				{/if}

				{#each notes as n (n.key)}
					<NoteBox note={n} here={`/task/${task.id}`} />
				{/each}

				{#if task.source_url}
					<section class="block">
						<h2 class="block-title">Where it came from</h2>
						<TaskSource url={task.source_url} />
					</section>
				{/if}

				{#if task.result && isClosed(task)}
					<section class="block">
						<h2 class="block-title">What the assistant left</h2>
						<div class="result"><Folded><Markdown content={task.result} /></Folded></div>
					</section>
				{/if}

				<!-- One feed: what happened to the task and your own notes, in the
				     order they came, with the note field under them. -->
				<section class="block">
					<h2 class="block-title">Activity</h2>
					{@render historyList()}
					<div class="composer">
						{#key task.id}
							<TaskComposer label="Add a note" placeholder="Add a note for yourself" onCommit={addNote} />
						{/key}
					</div>
				</section>
			</main>

			{#if layout === 'page'}
				<aside class="side" aria-label="About this task">
					{@render properties(task)}
				</aside>
			{/if}
{/snippet}

{#snippet properties(task: Task)}
	{@const closed = isClosed(task)}
	{@const home = homeOf(task)}
	{@const homeThread = home?.kind === 'thread' ? [...projectStore.projects, ...projectStore.archived].find((p) => p.id === home.id) : undefined}
	<!-- Each property is an icon and its value, as on a thread's page. A
	     closed task's are read until it is reopened. -->
	<div class="props">
		<PreviewRow
			icon={homeThread ? homeThread.icon || THREAD_ICON : home ? subjectIcon(home.kind) : 'atlas:projects'}
			iconColor={homeThread ? projectColor(homeThread) : null}
			quiet={!home}
		>
			{#if closed}
				{home ? subjectName(home) : 'No home'}
			{:else}
				<TaskHome {task} onSave={(p) => save(p, "Your server couldn't move that")} plain linked />
			{/if}
		</PreviewRow>
		<PreviewRow icon="mynaui:link" quiet={(task.also_about ?? []).length === 0}>
			<TaskAlsoAbout {task} onSave={(p) => save(p)} readonly={closed} plain />
		</PreviewRow>
		<!-- Its applet says when a task that repeats runs, so it has no due date. -->
		{#if !isRepeating(task)}
		<div class="prop">
			<Popover bind:open={dueOpen} placement="bottom-start" offset={4}>
				{#snippet trigger({ toggle: toggleDue })}
					<PreviewRow icon="mynaui:calendar" quiet={!task.due_on} onclick={closed ? undefined : toggleDue}>
						{task.due_on ? `Due ${dueLabel(task.due_on, task.due_at)}` : 'No date'}
					</PreviewRow>
				{/snippet}
				{#snippet children({ close })}
					<DuePicker
						due_on={task.due_on}
						due_at={task.due_at}
						onPick={(on, at) => {
							void save({ due_on: on, due_at: at }, "Your server couldn't change that date");
							close();
						}}
					/>
				{/snippet}
			</Popover>
		</div>
		{/if}
		{#if !isRequest(task)}
			<div class="prop">
				<Popover bind:open={recurOpen} placement="bottom-start" offset={4}>
					{#snippet trigger({ toggle: toggleRecur })}
						<PreviewRow icon="mynaui:refresh" quiet={!recurs} onclick={closed ? undefined : toggleRecur}>
							{recurs ? recurLabel(recurs.every, recurs.unit) : 'Does not repeat'}
						</PreviewRow>
					{/snippet}
					{#snippet children()}
						<div class="menu" role="menu" aria-label="How often this task repeats">
							<MenuItem role="menuitemradio" checked={!recurs} label="Does not repeat" onclick={() => saveRecurrence(null)} />
							{#each RECUR_CHOICES as choice (choice.every + choice.unit)}
								<MenuItem
									role="menuitemradio"
									checked={recurs?.every === choice.every && recurs?.unit === choice.unit}
									label={recurLabel(choice.every, choice.unit)}
									onclick={() => saveRecurrence(choice)}
								/>
							{/each}
							<p class="menu-note">Counted from the day you finish it.</p>
						</div>
					{/snippet}
				</Popover>
			</div>
		{/if}
		<PreviewRow icon={task.assignee === 'assistant' ? 'mynaui:sparkles' : 'mynaui:user'}>
			{task.assignee === 'assistant' ? 'For the assistant' : 'For you'}
		</PreviewRow>
	</div>
{/snippet}

<style>
	.pane-grid {
		padding: 24px 40px 64px;
	}
	/* Beside a list there is no room for a side column, so the properties
	   sit under the title and description, set apart by space. */
	.pane-props {
		max-width: 480px;
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
	.head {
		display: flex;
		align-items: flex-start;
		gap: 12px;
	}
	/* The check sits on the title's first line, which is 44px tall. */
	.check {
		display: inline-flex;
		flex: none;
		margin-top: 12px;
	}
	.title {
		flex: 1 1 auto;
		min-width: 0;
		padding: 0;
		border: 0;
		background: transparent;
		resize: none;
		overflow: hidden;
		outline: none;
		field-sizing: content;
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: var(--text-title);
		line-height: 44px;
		color: var(--color-foreground);
	}
	.title.closed {
		color: var(--color-foreground-muted);
		text-decoration: line-through;
	}
	.block-title {
		margin: 0 0 8px;
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: var(--md-h2-size);
		line-height: 28px;
		color: var(--color-foreground);
	}
	.quiet {
		margin: 0;
		font-size: var(--text-label);
		line-height: 20px;
		color: var(--color-foreground-muted);
	}
	.lede {
		display: flex;
		flex-direction: column;
		gap: 12px;
		margin-top: -16px;
	}
	.description,
	.result {
		font-size: var(--text-body);
		line-height: 24px;
	}
	.history {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	.history li {
		display: grid;
		grid-template-columns: 16px minmax(0, 1fr) auto;
		align-items: baseline;
		column-gap: 8px;
		padding: 4px 0;
		font-size: var(--text-label);
		line-height: 20px;
		color: var(--color-foreground);
	}
	/* Your own note: who and when on the line, the words under it. */
	.own-text {
		grid-column: 2 / -1;
		margin: 4px 0 4px;
		font-family: var(--font-serif);
		font-size: var(--text-body);
		line-height: 24px;
		color: var(--color-foreground);
		white-space: pre-wrap;
	}
	.history li.own .line {
		color: var(--color-foreground-muted);
	}
	.composer {
		margin-top: 8px;
	}
	.dot {
		justify-self: center;
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--color-foreground-subtle);
		transform: translateY(-2px);
	}
	.when {
		font-size: var(--text-small);
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}
	.side {
		padding-top: 8px;
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
		min-width: 12rem;
		padding: 4px;
	}
	.standing {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}
	.sentence {
		margin: 0;
		font-family: var(--font-serif);
		font-size: var(--text-lead);
		line-height: 26px;
		color: var(--color-foreground);
	}
	.moves {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
	}
	.described {
		margin-top: -16px;
		font-size: var(--text-body);
		line-height: 24px;
	}
	.write {
		margin-top: 16px;
	}
	.menu-note {
		margin: 0;
		padding: 8px 8px 4px;
		font-size: var(--text-caption);
		color: var(--color-foreground-subtle);
	}
</style>
