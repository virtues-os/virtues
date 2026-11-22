<script lang="ts">
	/**
	 * A task in the Threads door's preview column, in the thread preview's
	 * shape: its home's tile and its title, its description, one note if your
	 * server raised one, then its properties as rows. The home and date rows
	 * change them in place; Open goes to the task's page.
	 */
	import type { Task, TaskPatch } from '$lib/api/client';
	import Popover from '$lib/floating/primitives/Popover.svelte';
	import DuePicker from '$lib/components/tasks/DuePicker.svelte';
	import TaskHome from '$lib/components/tasks/TaskHome.svelte';
	import TaskActions from '$lib/components/tasks/TaskActions.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import ProjectGlyph from '$lib/components/ProjectGlyph.svelte';
	import PreviewHead from '$lib/components/threads/PreviewHead.svelte';
	import PreviewNote from '$lib/components/threads/PreviewNote.svelte';
	import PreviewRow from '$lib/components/threads/PreviewRow.svelte';
	import { Button } from '$lib';
	import { tasksStore } from '$lib/stores/tasks.svelte';
	import { projectStore } from '$lib/stores/project.svelte';
	import { notesStore } from '$lib/stores/notes.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { identityOf } from '$lib/refs/identity.svelte';
	import { projectColor } from '$lib/sidebar/pin-colors';
	import { isClosed, toggleTaskDone } from '$lib/tasks/actions';
	import { dueLabel } from '$lib/tasks/due';
	import { recurLabel, recurrenceOf } from '$lib/tasks/recur';
	import { homeOf, subjectIcon, subjectName } from '$lib/tasks/homes';
	import { isRequest } from '$lib/tasks/requests';
	import { toast } from 'svelte-sonner';

	let { task }: { task: Task } = $props();

	let dueOpen = $state(false);
	const closed = $derived(isClosed(task));
	const recurs = $derived(recurrenceOf(task));
	const source = $derived(task.source_url ? identityOf(task.source_url) : null);
	const home = $derived(homeOf(task));
	const homeThread = $derived(
		home?.kind === 'thread' ? [...projectStore.projects, ...projectStore.archived].find((p) => p.id === home.id) : undefined,
	);
	const homeColor = $derived(homeThread ? projectColor(homeThread) : null);
	const note = $derived(notesStore.about(`/task/${task.id}`)[0] ?? null);
	const also = $derived(task.also_about ?? []);

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

<PreviewHead color={homeColor} title={task.title} {closed}>
	{#snippet glyph()}
		{#if homeThread}
			<ProjectGlyph project={homeThread} size={18} />
		{:else}
			<Icon icon={home ? subjectIcon(home.kind) : 'mynaui:circle'} width="18" />
		{/if}
	{/snippet}
	{#snippet actions()}
		{#if !isRequest(task)}
			<Button variant="secondary" onclick={() => toggleTaskDone(task)}>{closed ? 'Mark not done' : 'Mark done'}</Button>
		{/if}
		<Button variant="ghost" onclick={open}>Open</Button>
	{/snippet}
</PreviewHead>

{#if task.notes}
	<p class="notes">{task.notes}</p>
{/if}

{#if note}
	<div class="block"><PreviewNote {note} /></div>
{/if}

<section class="block props" aria-label="Properties">
	<PreviewRow icon={home ? subjectIcon(home.kind) : 'atlas:projects'} iconColor={homeColor} quiet={!home}>
		{#if closed}
			{home ? subjectName(home) : 'No home'}
		{:else}
			<TaskHome {task} onSave={(p) => save(p, "Your server couldn't move that")} plain linked />
		{/if}
	</PreviewRow>

	<div class="due">
		<Popover bind:open={dueOpen} placement="bottom-start" offset={4}>
			{#snippet trigger({ toggle })}
				<PreviewRow icon="mynaui:calendar" quiet={!task.due_on} onclick={closed ? undefined : toggle}>
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

	{#if recurs}
		<PreviewRow icon="mynaui:refresh">{recurLabel(recurs.every, recurs.unit)}</PreviewRow>
	{/if}

	{#if also.length > 0}
		{@const names = also.map((l) => subjectName(l)).join(', ')}
		<PreviewRow icon="mynaui:link" title={`Also about ${names}`}>Also about {names}</PreviewRow>
	{/if}

	{#if source && task.source_url}
		{@const url = task.source_url}
		<PreviewRow icon={source.icon || 'mynaui:file-text'} title={source.title} onclick={() => windowShellStore.openRouteBeside(url)}>
			{source.title}
		</PreviewRow>
	{/if}
</section>

{#if !closed}
	<div class="block">
		<TaskActions {task} />
	</div>
{/if}

<style>
	.notes {
		display: -webkit-box;
		-webkit-line-clamp: 3;
		line-clamp: 3;
		-webkit-box-orient: vertical;
		overflow: hidden;
		margin: 8px 0 0;
		font-family: var(--font-serif);
		font-size: var(--text-body);
		line-height: 24px;
		color: var(--color-foreground-muted);
		white-space: pre-wrap;
	}
	.block {
		margin-top: 24px;
	}
	.props {
		display: flex;
		flex-direction: column;
	}
	.due :global(.popover-trigger) {
		display: block;
	}
</style>
