<script lang="ts">
	/**
	 * One task's page. The main column is the task: its check and title, the
	 * owner's notes, where it came from, a request's result, and its history
	 * from `app_task_events`, each line signed by who did it. The side column
	 * holds its home with Move, what else it is about, its due day, how often
	 * it repeats, and who it is for.
	 */
	import type { Tab } from '$lib/tabs/types';
	import type { Task, TaskEvent, TaskPatch } from '$lib/api/client';
	import { getTask, listTaskEvents, updateTask } from '$lib/api/client';
	import Icon from '$lib/components/Icon.svelte';
	import Markdown from '$lib/components/Markdown.svelte';
	import Popover from '$lib/floating/primitives/Popover.svelte';
	import TaskCheck from '$lib/components/tasks/TaskCheck.svelte';
	import TaskComposer from '$lib/components/tasks/TaskComposer.svelte';
	import DuePicker from '$lib/components/tasks/DuePicker.svelte';
	import TaskHome from '$lib/components/tasks/TaskHome.svelte';
	import TaskAlsoAbout from '$lib/components/tasks/TaskAlsoAbout.svelte';
	import { Button } from '$lib';
	import { tasksStore } from '$lib/stores/tasks.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { identityOf } from '$lib/refs/identity.svelte';
	import { isClosed, toggleTaskDone } from '$lib/tasks/actions';
	import { dueLabel } from '$lib/tasks/due';
	import { recurLabel, recurrenceOf } from '$lib/tasks/recur';
	import { eventLine, eventTime } from '$lib/tasks/history';
	import { toast } from 'svelte-sonner';

	let { tab }: { tab: Tab; active?: boolean } = $props();

	const taskId = $derived(tab.route.match(/^\/task\/([^/?#]+)/)?.[1] ?? null);

	// The store's copy when the list has it, so an edit here and in the list
	// stay one task; else this page's own read.
	let fetched = $state<Task | null>(null);
	const task = $derived((taskId ? tasksStore.find(taskId) : undefined) ?? fetched);
	let error = $state<string | null>(null);
	let history = $state<TaskEvent[]>([]);

	async function load(id: string) {
		error = null;
		try {
			if (!tasksStore.find(id)) fetched = await getTask(id);
			history = await listTaskEvents(id);
		} catch (e) {
			console.error('[TaskView] Failed to load task:', e);
			error = "Your server couldn't open this task. It may have been removed.";
		}
	}

	$effect(() => {
		if (taskId) void load(taskId);
	});

	$effect(() => {
		if (task && tab.label !== task.title) windowShellStore.updateTab(tab.id, { label: task.title });
	});

	async function save(patch: TaskPatch, failure = "Your server couldn't change that") {
		if (!task) return;
		try {
			if (tasksStore.find(task.id)) await tasksStore.update(task.id, patch);
			else fetched = await updateTask(task.id, patch);
			history = await listTaskEvents(task.id);
		} catch (e) {
			toast.error(failure, { description: e instanceof Error ? e.message : undefined });
		}
	}

	async function toggle() {
		if (!task) return;
		if (tasksStore.find(task.id)) await toggleTaskDone(task);
		else await save({ status: isClosed(task) ? 'open' : 'done' });
		if (task) history = await listTaskEvents(task.id);
	}

	function saveTitle(value: string) {
		const title = value.trim();
		if (task && title && title !== task.title) void save({ title }, "Your server couldn't rename that task");
	}

	function saveNotes(value: string) {
		const notes = value.trim() || null;
		if (task && notes !== (task.notes ?? null)) void save({ notes }, "Your server couldn't save those notes");
	}

	let dueOpen = $state(false);

	const source = $derived(task?.source_url ? identityOf(task.source_url) : null);
	const recurs = $derived(task ? recurrenceOf(task) : null);
</script>

<div class="task-page">
	{#if error}
		<div class="state">{error}</div>
	{:else if !task}
		<div class="state"><Icon icon="mynaui:spinner" width="18" class="spin" /> Loading…</div>
	{:else}
		<div class="frame">
			<main class="main">
				<div class="head">
					<span class="check">
						<TaskCheck
							done={task.status === 'done'}
							label={isClosed(task) ? `Reopen ${task.title}` : `Complete ${task.title}`}
							onclick={toggle}
						/>
					</span>
					{#key task.id}
						<input
							class="title"
							class:closed={isClosed(task)}
							value={task.title}
							aria-label="Title"
							readonly={isClosed(task)}
							onblur={(e) => saveTitle(e.currentTarget.value)}
							onkeydown={(e) => {
								if (e.key === 'Enter') e.currentTarget.blur();
							}}
						/>
					{/key}
					<Button variant="secondary" size="sm" onclick={toggle}>{isClosed(task) ? 'Reopen' : 'Mark done'}</Button>
				</div>

				<section class="block">
					<h2 class="block-title">Notes</h2>
					{#key task.id}
						<TaskComposer
							value={task.notes ?? ''}
							label="Notes"
							placeholder="Add a note for yourself"
							commitOnBlur
							onCommit={saveNotes}
						/>
					{/key}
				</section>

				{#if task.source_url && source}
					<section class="block">
						<h2 class="block-title">Where it came from</h2>
						<button
							type="button"
							class="source"
							onclick={() => windowShellStore.openRouteBeside(task.source_url!)}
						>
							<Icon icon={source.icon || 'mynaui:link'} width="16" />
							<span>{source.title}</span>
						</button>
					</section>
				{/if}

				{#if task.result}
					<section class="block">
						<h2 class="block-title">What the assistant left</h2>
						<div class="result"><Markdown content={task.result} /></div>
					</section>
				{/if}

				<section class="block">
					<h2 class="block-title">History</h2>
					{#if history.length === 0}
						<p class="quiet">Nothing has happened to it yet.</p>
					{:else}
						<ol class="history">
							{#each history as e (e.id)}
								<li>
									<span class="dot" aria-hidden="true"></span>
									<span class="line">{eventLine(e)}</span>
									<span class="when">{eventTime(e)}</span>
								</li>
							{/each}
						</ol>
					{/if}
				</section>
			</main>

			<aside class="side" aria-label="About this task">
				<dl class="props">
					<dt>Home</dt>
					<dd><TaskHome {task} onSave={(p) => save(p, "Your server couldn't move that")} showIcon /></dd>
					<dt>Also about</dt>
					<dd><TaskAlsoAbout {task} onSave={(p) => save(p)} readonly={isClosed(task)} /></dd>
					<dt>Due</dt>
					<dd>
						<Popover bind:open={dueOpen} placement="bottom-start" offset={4}>
							{#snippet trigger({ toggle: toggleDue })}
								<button type="button" class="fact" onclick={toggleDue} aria-haspopup="dialog">
									{task.due_on ? dueLabel(task.due_on, task.due_at) : 'No date'}
								</button>
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
					</dd>
					<dt>Repeats</dt>
					<dd class="plain">{recurs ? recurLabel(recurs.every, recurs.unit) : 'Never'}</dd>
					<dt>For</dt>
					<dd class="plain">{task.assignee === 'assistant' ? 'The assistant' : 'You'}</dd>
				</dl>
			</aside>
		</div>
	{/if}
</div>

<style>
	.task-page {
		width: 100%;
		height: 100%;
		overflow-y: auto;
	}
	.frame {
		display: grid;
		grid-template-columns: minmax(0, 1fr) 280px;
		gap: 48px;
		max-width: 1080px;
		margin: 0 auto;
		padding: 48px 32px 96px;
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
		align-items: center;
		gap: 12px;
	}
	.check {
		display: inline-flex;
		flex: none;
	}
	.title {
		flex: 1 1 auto;
		min-width: 0;
		padding: 0;
		border: 0;
		background: transparent;
		font-family: var(--font-serif);
		font-size: 36px;
		line-height: 44px;
		color: var(--color-foreground);
	}
	.title:focus {
		outline: none;
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
		font-size: 15px;
		line-height: 24px;
		color: var(--color-foreground-muted);
	}
	.source {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		padding: 8px 12px;
		margin: 0 -12px;
		border: 0;
		border-radius: var(--radius-md);
		background: transparent;
		font: inherit;
		font-size: 15px;
		color: var(--color-foreground);
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.source:hover {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}
	.source :global(svg) {
		color: var(--color-foreground-muted);
	}
	.result {
		font-size: 15px;
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
		gap: 8px;
		padding: 4px 0;
		font-size: 14px;
		line-height: 20px;
		color: var(--color-foreground);
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
		font-size: 13px;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}
	.side {
		padding-top: 12px;
	}
	.props {
		display: grid;
		grid-template-columns: 80px minmax(0, 1fr);
		gap: 12px;
		margin: 0;
		align-items: baseline;
	}
	.props dt {
		font-size: 13px;
		line-height: 24px;
		color: var(--color-foreground-muted);
	}
	.props dd {
		margin: 0;
		min-width: 0;
		line-height: 24px;
	}
	.plain {
		font-size: 14px;
		color: var(--color-foreground);
	}
	.fact {
		padding: 0 4px;
		margin: 0 -4px;
		border: 0;
		border-radius: var(--radius-sm);
		background: transparent;
		font: inherit;
		font-size: 14px;
		color: var(--color-foreground);
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.fact:hover {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}
	@media (max-width: 900px) {
		.frame {
			grid-template-columns: minmax(0, 1fr);
			padding: 24px 16px 64px;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.source,
		.fact {
			transition: none;
		}
	}
	:global(.spin) {
		animation: spin 0.8s linear infinite;
	}
	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}
</style>
