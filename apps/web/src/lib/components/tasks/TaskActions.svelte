<script lang="ts" module>
	import { getByoKey } from '$lib/api/client';
	import { placeOf } from '$lib/tasks/requests';

	/* Where your own account runs, asked once per load: the preview remounts
	   this for every task you point at. */
	let byoPlaceOnce: Promise<string> | null = null;
	function byoPlace(): Promise<string> {
		byoPlaceOnce ??= getByoKey<{ endpoint_url?: string | null }>()
			.then((s) => placeOf(s.endpoint_url))
			.catch(() => 'on your own account');
		return byoPlaceOnce;
	}
</script>

<script lang="ts">
	/**
	 * What can be done with a task, on its page and in the door's preview.
	 *
	 *   A task that repeats  its newest result, when it runs, and Options.
	 *                     Nothing to accept: opening it is all it asks.
	 *   A ready request   the assistant's result, then Accept and Reply.
	 *   A request at work where it runs and what it has cost, Start or Take it
	 *                     back, and a note for its next run.
	 *   Your own task     Ask the assistant (once, never a recurring one), and
	 *                     Dismiss.
	 *
	 * Each is a real button: the one that moves the task forward is primary,
	 * the rest are secondary, and Dismiss is ghost because it puts the task away.
	 */
	import type { Task, TaskPatch } from '$lib/api/client';
	import { addTaskNote } from '$lib/api/client';
	import { Button } from '$lib';
	import Markdown from '$lib/components/Markdown.svelte';
	import RefCard from '$lib/components/RefCard.svelte';
	import TaskComposer from '$lib/components/tasks/TaskComposer.svelte';
	import { tasksStore } from '$lib/stores/tasks.svelte';
	import { modelSlots } from '$lib/stores/modelSlots.svelte';
	import { pagesStore } from '$lib/stores/pages.svelte';
	import { isClosed } from '$lib/tasks/actions';
	import { appletOf, isRepeating, requestState } from '$lib/tasks/requests';
	import { describeTriggers } from '$lib/applets/palette';
	import { actionOf, problemOf } from '$lib/applets/standing';
	import { optionsOf } from '$lib/applets/kind';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { runApplet } from '$lib/api/client';
	import { appletsStore } from '$lib/stores/applets.svelte';
	import { recurrenceOf } from '$lib/tasks/recur';
	import { formatMicrosUSD } from '$lib/utils/currency';
	import { toast } from 'svelte-sonner';
	import { onMount } from 'svelte';

	let { task, onChanged }: { task: Task; onChanged?: () => void } = $props();

	const request = $derived(task.assignee === 'assistant');

	// A task that repeats is run by its applet, which says when it runs and
	// whether its last run went wrong.
	const repeating = $derived(isRepeating(task));
	const applet = $derived(appletOf(task));
	const trouble = $derived(applet ? problemOf(applet) : null);
	const fix = $derived(applet ? actionOf(applet) : null);
	let starting = $state(false);

	function openOptions() {
		if (applet) windowShellStore.openRouteBeside(optionsOf(applet), applet.name);
	}

	async function actOnTrouble() {
		if (!applet || !fix) return;
		if (fix.kind === 'fix') {
			windowShellStore.openTabFromRoute(fix.href, { focusExisting: true });
			return;
		}
		starting = true;
		try {
			await runApplet(applet.id);
			await appletsStore.load();
		} catch (e) {
			toast.error("Your server didn't start it", { description: e instanceof Error ? e.message : undefined });
		} finally {
			starting = false;
		}
	}
	let replying = $state(false);
	let place = $state('on your own account');

	onMount(() => {
		if (!modelSlots.loaded) void modelSlots.load();
		void byoPlace().then((p) => (place = p));
	});

	const where = $derived.by(() => {
		if (!task.run_model) return null;
		const name = modelSlots.nameOf(task.run_model);
		return task.run_route === 'byo' ? `${name} ${place}` : `${name} through Virtues`;
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
		{#if repeating}
			{#if task.result}
				<div class="result">
					<Markdown content={task.result} />
					{#if task.result_url}
						<RefCard type={task.result_url.split('/')[1] ?? null} label={resultLabel(task.result_url)} url={task.result_url} />
					{/if}
				</div>
			{/if}
			<p class="state">
				{#if !applet}
					It repeats on its own.
				{:else if !applet.enabled}
					It is off. Turn it on in Options.
				{:else if trouble}
					{trouble}.
				{:else}
					{describeTriggers(applet)}.{#if !task.result}{' '}Its first result shows here.{/if}
				{/if}
			</p>
			<div class="buttons">
				{#if applet?.enabled && fix}
					<Button onclick={actOnTrouble} loading={starting}>{fix.label}</Button>
				{/if}
				{#if applet}
					<Button variant="secondary" onclick={openOptions}>Options</Button>
				{/if}
			</div>
		{:else if request && task.status === 'ready'}
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
				<Button onclick={() => save({ status: 'done' }, "Your server couldn't close that request")}>Accept</Button>
				<Button variant="secondary" onclick={() => (replying = !replying)}>{replying ? 'Cancel reply' : 'Reply'}</Button>
			</div>
		{:else if request}
			<p class="state">
				{requestState(task)}.
				{#if where}Runs on {where}.{/if}
				{#if task.spent_micros}{formatMicrosUSD(task.spent_micros)} so far.{/if}
			</p>
			<div class="buttons">
				{#if task.status === 'open' && !task.next_run_at}
					<Button onclick={() => save({ assignee: 'assistant' }, "Your server couldn't start that")}>Start</Button>
				{/if}
				<Button variant="secondary" onclick={() => save({ assignee: 'owner' }, "Your server couldn't take that back")}>Take it back</Button>
				<Button variant="ghost" onclick={dismiss}>Dismiss</Button>
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
					<Button
						variant="secondary"
						icon="mynaui:sparkles"
						onclick={() => save({ assignee: 'assistant' }, "Your server couldn't hand that over")}
					>
						Ask the assistant
					</Button>
				{/if}
				<Button variant="ghost" onclick={dismiss}>Dismiss</Button>
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
		font-size: var(--text-body);
		line-height: 24px;
		color: var(--color-foreground);
	}
	.state {
		margin: 0;
		font-size: var(--text-label);
		line-height: 20px;
		color: var(--color-foreground-muted);
	}
	.buttons {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}
</style>
