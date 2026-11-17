<script lang="ts">
	/**
	 * A task in the Threads door's preview column: its title and notes, its
	 * home and what else it is about, its due day and where it came from, and
	 * what can be done with it. "Mark done" is the column's one button; Open
	 * goes to the task's page.
	 */
	import type { Task, TaskPatch } from '$lib/api/client';
	import Popover from '$lib/floating/primitives/Popover.svelte';
	import DuePicker from '$lib/components/tasks/DuePicker.svelte';
	import TaskHome from '$lib/components/tasks/TaskHome.svelte';
	import TaskAlsoAbout from '$lib/components/tasks/TaskAlsoAbout.svelte';
	import TaskActions from '$lib/components/tasks/TaskActions.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { Button } from '$lib';
	import { tasksStore } from '$lib/stores/tasks.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { identityOf } from '$lib/refs/identity.svelte';
	import { isClosed, toggleTaskDone } from '$lib/tasks/actions';
	import { dueLabel } from '$lib/tasks/due';
	import { recurLabel, recurrenceOf } from '$lib/tasks/recur';
	import { isRequest } from '$lib/tasks/requests';
	import { toast } from 'svelte-sonner';

	let { task }: { task: Task } = $props();

	let dueOpen = $state(false);
	const recurs = $derived(recurrenceOf(task));
	const source = $derived(task.source_url ? identityOf(task.source_url) : null);

	async function save(patch: TaskPatch, failure = "Your server couldn't change that") {
		try {
			await tasksStore.update(task.id, patch);
		} catch (e) {
			toast.error(failure, { description: e instanceof Error ? e.message : undefined });
		}
	}

	function open() {
		windowShellStore.openTabFromRoute(`/task/${task.id}`, { label: task.title, focusExisting: true });
	}
</script>

<div class="ph">
	<span class="kind">{isRequest(task) ? 'Request' : 'Task'}</span>
	<span class="buttons">
		{#if !isRequest(task)}
			<Button variant="secondary" size="sm" onclick={() => toggleTaskDone(task)}>
				{isClosed(task) ? 'Reopen' : 'Mark done'}
			</Button>
		{/if}
		<Button variant="secondary" size="sm" onclick={open}>Open</Button>
	</span>
</div>

<h2 class="title" class:closed={isClosed(task)}>{task.title}</h2>
{#if task.notes}
	<p class="notes">{task.notes}</p>
{/if}

<dl class="props">
	<dt>Home</dt>
	<dd><TaskHome {task} onSave={(p) => save(p, "Your server couldn't move that")} showIcon plain /></dd>
	<dt>Also about</dt>
	<dd><TaskAlsoAbout {task} onSave={(p) => save(p)} readonly={isClosed(task)} plain /></dd>
	<dt>Due</dt>
	<dd>
		<Popover bind:open={dueOpen} placement="bottom-start" offset={4}>
			{#snippet trigger({ toggle })}
				<button type="button" class="fact" onclick={toggle} aria-haspopup="dialog">
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
	{#if recurs}
		<dt>Repeats</dt>
		<dd class="plain">{recurLabel(recurs.every, recurs.unit)}</dd>
	{/if}
	{#if source && task.source_url}
		<dt>From</dt>
		<dd>
			<button type="button" class="fact source" onclick={() => windowShellStore.openRouteBeside(task.source_url!)}>
				<Icon icon={source.icon || 'mynaui:link'} width="14" />
				<span>{source.title}</span>
			</button>
		</dd>
	{/if}
</dl>

<div class="actions">
	<TaskActions {task} />
</div>

<style>
	.ph {
		display: flex;
		align-items: center;
		justify-content: space-between;
		height: 48px;
		gap: 8px;
	}
	.kind {
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	.buttons {
		display: inline-flex;
		gap: 8px;
	}
	.title {
		margin: 16px 0 0;
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: var(--md-h2-size);
		line-height: 28px;
		color: var(--color-foreground);
	}
	.title.closed {
		color: var(--color-foreground-muted);
		text-decoration: line-through;
	}
	.notes {
		margin: 8px 0 0;
		font-family: var(--font-serif);
		font-size: 16px;
		line-height: 24px;
		color: var(--color-foreground);
		white-space: pre-wrap;
	}
	.props {
		display: grid;
		grid-template-columns: 80px minmax(0, 1fr);
		gap: 8px 12px;
		margin: 24px 0 0;
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
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 0 4px;
		margin: 0 -4px;
		border: 0;
		border-radius: var(--radius-sm);
		background: transparent;
		font: inherit;
		font-size: 14px;
		color: var(--color-foreground);
		cursor: pointer;
		text-align: left;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.fact:hover {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}
	.source :global(svg) {
		flex: none;
		color: var(--color-foreground-muted);
	}
	.actions {
		margin-top: 24px;
	}
	@media (prefers-reduced-motion: reduce) {
		.fact {
			transition: none;
		}
	}
</style>
