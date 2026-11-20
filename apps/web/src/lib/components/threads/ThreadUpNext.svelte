<script lang="ts">
	/**
	 * What is next in a thread. In the preview column (`tasks` above zero):
	 * its soonest open tasks, the ones also about it among them, which you
	 * can tick off here, with New task to add one to it, then the soonest
	 * planned record. On the thread's page, which lists its tasks in full
	 * (`tasks` at zero): the planned records coming up. Renders nothing when
	 * there is nothing to show and no way to add.
	 */
	import type { ProjectDetail, Task } from '$lib/api/client';
	import Icon from '$lib/components/Icon.svelte';
	import TextAction from '$lib/components/TextAction.svelte';
	import TaskCheck from '$lib/components/tasks/TaskCheck.svelte';
	import PreviewRow from '$lib/components/threads/PreviewRow.svelte';
	import { newTaskDialog } from '$lib/stores/newTask.svelte';
	import { tasksStore } from '$lib/stores/tasks.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { toggleTaskDone } from '$lib/tasks/actions';
	import { dueLabel } from '$lib/tasks/due';
	import { homeOf } from '$lib/tasks/homes';
	import { isRequest, requestState } from '$lib/tasks/requests';

	let {
		threadId,
		detail,
		tasks = 3,
		canAdd = true,
	}: {
		threadId: string;
		detail: ProjectDetail | null;
		/** How many open tasks to list. Zero where the page lists them itself. */
		tasks?: number;
		/** New task, left out for an ended thread. */
		canAdd?: boolean;
	} = $props();

	/** Its open tasks, soonest first and undated last. It reads the task store, so a tick leaves the row at once. */
	const open = $derived.by<Task[]>(() => {
		if (tasks <= 0) return [];
		return tasksStore.open
			.filter((t) => {
				const home = homeOf(t);
				return (
					(home?.kind === 'thread' && home.id === threadId) ||
					(t.also_about ?? []).some((l) => l.kind === 'thread' && l.id === threadId)
				);
			})
			.sort((a, b) => (a.due_on ?? '~').localeCompare(b.due_on ?? '~') || a.sort_order - b.sort_order)
			.slice(0, tasks);
	});

	/** Planned records still to come: one beside the tasks, three on their own. */
	const ahead = $derived.by(() => {
		if (!detail) return [];
		const now = Date.now();
		return detail.items
			.filter((i) => i.occurred_at && new Date(i.occurred_at).getTime() > now)
			.sort((a, b) => (a.occurred_at ?? '').localeCompare(b.occurred_at ?? ''))
			.slice(0, tasks > 0 ? 1 : 3);
	});

	const heading = $derived(tasks > 0 ? 'Up next' : 'Coming up');
	const show = $derived(open.length > 0 || ahead.length > 0 || (tasks > 0 && canAdd));

	/** "Thu 1 PM" within the coming week, else "Dec 3". */
	function whenAhead(iso: string): string {
		const d = new Date(iso);
		if (d.getTime() - Date.now() > 6 * 86_400_000) {
			return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
		}
		const time = d.toLocaleTimeString('en-US', { hour: 'numeric', minute: d.getMinutes() ? '2-digit' : undefined });
		return `${d.toLocaleDateString('en-US', { weekday: 'short' })} ${time}`;
	}

	function openTask(t: Task) {
		windowShellStore.openTabFromRoute(`/task/${t.id}`, { label: t.title, focusExisting: true });
	}
</script>

{#if show}
	<section aria-label={heading}>
		<div class="head">
			<h3 class="label">{heading}</h3>
			{#if tasks > 0 && canAdd}
				<TextAction quiet onclick={() => newTaskDialog.open({ home: { kind: 'thread', id: threadId } })}>New task</TextAction>
			{/if}
		</div>
		{#each open as t (t.id)}
			<div class="next">
				{#if isRequest(t)}
					<span class="req" aria-hidden="true"><Icon icon="mynaui:sparkles" width="14" /></span>
				{:else}
					<TaskCheck done={false} label={`Complete ${t.title}`} onclick={() => toggleTaskDone(t)} />
				{/if}
				<button type="button" class="next-title" onclick={() => openTask(t)}>{t.title}</button>
				{#if isRequest(t)}
					<span class="when">{requestState(t)}</span>
				{:else if t.due_on}
					<span class="when">{dueLabel(t.due_on, t.due_at)}</span>
				{/if}
			</div>
		{/each}
		{#each ahead as item (item.url)}
			<PreviewRow icon="mynaui:calendar" trailing={whenAhead(item.occurred_at!)} onclick={() => windowShellStore.openRouteBeside(item.url)}>
				{item.title ?? 'Something planned'}
			</PreviewRow>
		{/each}
		{#if open.length === 0 && ahead.length === 0}
			<p class="none">Nothing open here.</p>
		{/if}
	</section>
{/if}

<style>
	.head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		margin-bottom: 8px;
	}
	.label {
		margin: 0;
		font-size: var(--text-label);
		font-weight: 500;
		line-height: 20px;
		color: var(--color-foreground-muted);
	}
	.next {
		display: flex;
		align-items: center;
		gap: 12px;
		min-height: 32px;
		margin: 0 -8px;
		padding: 0 8px;
		border-radius: var(--radius-md);
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.next:hover {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}
	.req {
		display: grid;
		place-items: center;
		flex: none;
		width: 18px;
		color: var(--color-primary);
	}
	.next-title {
		min-width: 0;
		flex: 1 1 auto;
		padding: 0;
		border: 0;
		background: transparent;
		font: inherit;
		font-size: var(--text-label);
		line-height: 20px;
		text-align: left;
		color: var(--color-foreground);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		cursor: pointer;
	}
	.when {
		flex: none;
		font-size: var(--text-small);
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
	}
	.none {
		margin: 0;
		font-size: var(--text-small);
		line-height: 20px;
		color: var(--color-foreground-muted);
	}
	@media (prefers-reduced-motion: reduce) {
		.next {
			transition: none;
		}
	}
</style>
