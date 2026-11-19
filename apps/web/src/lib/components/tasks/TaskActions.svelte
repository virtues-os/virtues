<script lang="ts">
	/**
	 * What can be done with a task, on its page and in the door's preview.
	 *
	 *   A ready request   the assistant's result, then Accept and Reply.
	 *   A request at work where it runs and what it has cost, Start or Take it
	 *                     back, and a note for its next run.
	 *   Your own task     Ask the assistant to do it (once, never a recurring
	 *                     one), and Dismiss.
	 */
	import type { Task, TaskPatch } from '$lib/api/client';
	import { addTaskNote, getByoKey } from '$lib/api/client';
	import Markdown from '$lib/components/Markdown.svelte';
	import RefCard from '$lib/components/RefCard.svelte';
	import TaskComposer from '$lib/components/tasks/TaskComposer.svelte';
	import { tasksStore } from '$lib/stores/tasks.svelte';
	import { modelSlots } from '$lib/stores/modelSlots.svelte';
	import { pagesStore } from '$lib/stores/pages.svelte';
	import { isClosed } from '$lib/tasks/actions';
	import { placeOf, requestState } from '$lib/tasks/requests';
	import { recurrenceOf } from '$lib/tasks/recur';
	import { formatMicrosUSD } from '$lib/utils/currency';
	import { toast } from 'svelte-sonner';
	import { onMount } from 'svelte';

	let { task, onChanged }: { task: Task; onChanged?: () => void } = $props();

	const request = $derived(task.assignee === 'assistant');
	let replying = $state(false);
	let byoPlace = $state('on your own account');

	onMount(() => {
		if (!modelSlots.loaded) void modelSlots.load();
		void getByoKey<{ endpoint_url?: string | null }>()
			.then((s) => (byoPlace = placeOf(s.endpoint_url)))
			.catch(() => {
				// The line keeps its general wording.
			});
	});

	const where = $derived.by(() => {
		if (!task.run_model) return null;
		const name = modelSlots.nameOf(task.run_model);
		return task.run_route === 'byo' ? `${name} ${byoPlace}` : `${name} through Virtues`;
	});

	async function save(patch: TaskPatch, failure: string) {
		try {
			await tasksStore.update(task.id, patch);
			onChanged?.();
		} catch (e) {
			toast.error(failure, { description: e instanceof Error ? e.message : undefined });
		}
	}

	function dismiss() {
		void save({ status: 'dismissed' }, "Your server couldn't dismiss that");
		toast('Dismissed', {
			action: { label: 'Undo', onClick: () => void save({ status: 'open' }, "Your server couldn't restore that") },
		});
	}

	async function reply(text: string): Promise<boolean> {
		const said = text.trim();
		if (!said) return false;
		try {
			await tasksStore.reply(task.id, said);
			replying = false;
			onChanged?.();
			return true;
		} catch (e) {
			toast.error("Your server couldn't send that reply", { description: e instanceof Error ? e.message : undefined });
			return false;
		}
	}

	async function note(text: string): Promise<boolean> {
		const said = text.trim();
		if (!said) return false;
		try {
			await addTaskNote(task.id, said);
			onChanged?.();
			return true;
		} catch (e) {
			toast.error("Your server couldn't add that note", { description: e instanceof Error ? e.message : undefined });
			return false;
		}
	}

	function resultLabel(url: string): string {
		const id = url.slice(url.lastIndexOf('/') + 1);
		return pagesStore.pages.find((p) => p.id === id)?.title || 'The page it made';
	}
</script>

{#if !isClosed(task)}
	<div class="task-actions">
		{#if request && task.status === 'ready'}
			<div class="result">
				<Markdown content={task.result ?? ''} />
				{#if task.result_url}
					<RefCard type={task.result_url.split('/')[1] ?? null} label={resultLabel(task.result_url)} url={task.result_url} />
				{/if}
			</div>
			{#if replying}
				<TaskComposer
					label="Reply"
					placeholder="What should it change or try next?"
					sendLabel="Send"
					autofocus
					onCommit={reply}
				/>
			{/if}
			<div class="buttons">
				<button type="button" class="action strong" onclick={() => save({ status: 'done' }, "Your server couldn't close that request")}>Accept</button>
				<button type="button" class="action" onclick={() => (replying = !replying)}>{replying ? 'Cancel reply' : 'Reply'}</button>
			</div>
		{:else if request}
			<p class="state">
				{requestState(task)}.
				{#if where}Runs on {where}.{/if}
				{#if task.spent_micros}{formatMicrosUSD(task.spent_micros)} so far.{/if}
			</p>
			<div class="buttons">
				{#if task.status === 'open' && !task.next_run_at}
					<button type="button" class="action strong" onclick={() => save({ assignee: 'assistant' }, "Your server couldn't start that")}>Start</button>
				{/if}
				<button type="button" class="action" onclick={() => save({ assignee: 'owner' }, "Your server couldn't take that back")}>Take it back</button>
				<button type="button" class="action" onclick={dismiss}>Dismiss</button>
			</div>
			<TaskComposer
				label="Add a note for the assistant"
				placeholder="Add a note for the assistant's next run"
				sendLabel="Add note"
				onCommit={note}
			/>
		{:else}
			<div class="buttons">
				{#if !recurrenceOf(task)}
					<button type="button" class="action" onclick={() => save({ assignee: 'assistant' }, "Your server couldn't hand that over")}>Ask the assistant to do it</button>
				{/if}
				<button type="button" class="action" onclick={dismiss}>Dismiss</button>
			</div>
		{/if}
	</div>
{/if}

<style>
	.task-actions {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}
	.result {
		font-size: 15px;
		line-height: 24px;
		color: var(--color-foreground);
	}
	.state {
		margin: 0;
		font-size: 14px;
		line-height: 20px;
		color: var(--color-foreground-muted);
	}
	.buttons {
		display: flex;
		flex-wrap: wrap;
		gap: 16px;
	}
	.action {
		padding: 0;
		border: 0;
		background: none;
		font: inherit;
		font-size: 14px;
		font-weight: 500;
		color: var(--color-primary);
		cursor: pointer;
	}
	.action:not(.strong) {
		color: var(--color-foreground-muted);
		font-weight: 400;
	}
	.action:hover {
		color: var(--color-foreground);
	}
	.action:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
		border-radius: var(--radius-sm);
	}
</style>
