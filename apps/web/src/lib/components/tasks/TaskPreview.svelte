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
	import TaskActions from '$lib/components/tasks/TaskActions.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { Button } from '$lib';
	import { tasksStore } from '$lib/stores/tasks.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { identityOf } from '$lib/refs/identity.svelte';
	import { isClosed, toggleTaskDone } from '$lib/tasks/actions';
	import { dueLabel } from '$lib/tasks/due';
	import { recurLabel, recurrenceOf } from '$lib/tasks/recur';
	import { homeOf, subjectIcon, subjectName } from '$lib/tasks/homes';
	import { isRequest } from '$lib/tasks/requests';
	import { toast } from 'svelte-sonner';
	import { notesStore } from '$lib/stores/notes.svelte';
	import TextAction from '$lib/components/TextAction.svelte';

	let { task }: { task: Task } = $props();

	let dueOpen = $state(false);
	const recurs = $derived(recurrenceOf(task));
	const source = $derived(task.source_url ? identityOf(task.source_url) : null);
	const home = $derived(homeOf(task));

	async function save(patch: TaskPatch, failure = "Your server couldn't change that") {
		try {
			await tasksStore.update(task.id, patch);
		} catch (e) {
			toast.error(failure, { description: e instanceof Error ? e.message : undefined });
		}
	}
</script>

<div class="ph">
	<span class="kind">{isRequest(task) ? 'Request' : 'Task'}</span>
	{#if !isRequest(task)}
		<Button variant="secondary" size="sm" onclick={() => toggleTaskDone(task)}>
			{isClosed(task) ? 'Mark not done' : 'Mark done'}
		</Button>
	{/if}
</div>

<h2 class="title" class:closed={isClosed(task)}>{task.title}</h2>
{#if task.notes}
	<p class="notes">{task.notes}</p>
{/if}

{#each notesStore.about(`/task/${task.id}`).slice(0, 2) as n (n.key)}
	<div class="note">
		<p class="note-text"><span class="mark" aria-hidden="true"></span>{n.text}</p>
		<div class="note-acts">
			<Button variant="secondary" size="sm" onclick={() => windowShellStore.openTabFromRoute(n.action_route, { focusExisting: true })}>
				{n.action}
			</Button>
			<TextAction
				onclick={() =>
					windowShellStore.openTabFromRoute(`/insight/${encodeURIComponent(n.key)}`, { label: n.text, focusExisting: true })}
				>Why this?</TextAction
			>
			<TextAction onclick={() => notesStore.dismiss(n)}>Not now</TextAction>
		</div>
	</div>
{/each}

<div class="chips">
	{#if home}
		<span class="chip"><Icon icon={subjectIcon(home.kind)} width="14" />{subjectName(home)}</span>
	{:else}
		<span class="chip quiet"><Icon icon="atlas:projects" width="14" />No home yet</span>
	{/if}
	{#each task.also_about ?? [] as link (link.kind + link.id)}
		<span class="chip"><Icon icon={subjectIcon(link.kind)} width="14" />Also about {subjectName(link)}</span>
	{/each}
	<span class="chip"><Icon icon="mynaui:calendar" width="14" />{task.due_on ? `Due ${dueLabel(task.due_on, task.due_at).toLowerCase()}` : 'No date'}</span>
	{#if recurs}
		<span class="chip"><Icon icon="mynaui:refresh" width="14" />{recurLabel(recurs.every, recurs.unit)}</span>
	{/if}
	{#if source && task.source_url}
		<button type="button" class="chip link" onclick={() => windowShellStore.openRouteBeside(task.source_url!)}>
			<Icon icon={source.icon || 'mynaui:link'} width="14" />{source.title}
		</button>
	{/if}
</div>

{#if !isClosed(task)}
	<div class="buttons">
		<Popover bind:open={dueOpen} placement="bottom-start" offset={4}>
			{#snippet trigger({ toggle })}
				<Button variant="secondary" size="sm" onclick={toggle}>Change date</Button>
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
		<span class="move"><TaskHome {task} onSave={(p) => save(p, "Your server couldn't move that")} label="Move" plain /></span>
	</div>
{/if}

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
		font-size: 18px;
		line-height: 28px;
		font-weight: 500;
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
	.mark {
		flex: none;
		width: 6px;
		height: 6px;
		margin-top: 8px;
		border-radius: 50%;
		background: var(--color-primary);
	}
	.note-acts {
		display: flex;
		align-items: center;
		gap: 16px;
		margin: 12px 0 0 16px;
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
		border: 0;
		border-radius: var(--radius-md);
		background: var(--hover-bg);
		font: inherit;
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
	.chip.quiet {
		color: var(--color-foreground-muted);
	}
	.chip.link {
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.chip.link:hover {
		background: var(--active-bg);
		transition-duration: var(--motion-hover-in);
	}
	.buttons {
		display: flex;
		align-items: center;
		gap: 16px;
		margin-top: 20px;
	}
	.actions {
		margin-top: 24px;
	}
	@media (prefers-reduced-motion: reduce) {
		.chip.link {
			transition: none;
		}
	}
</style>
