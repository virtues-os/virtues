<script lang="ts">
	/**
	 * New task: a title, notes, and the facts a task carries (its home, what
	 * else it is about, its due day, who it is for), then Create. A date at
	 * the end of the title, like "fri" or "tomorrow 5pm", sets the due day.
	 * ⌘Return creates; "Create more" keeps the dialog open for the next one.
	 */
	import type { Task, TaskPatch } from '$lib/api/client';
	import Modal from '$lib/components/Modal.svelte';
	import Popover from '$lib/floating/primitives/Popover.svelte';
	import DuePicker from '$lib/components/tasks/DuePicker.svelte';
	import TaskHome from '$lib/components/tasks/TaskHome.svelte';
	import TaskAlsoAbout from '$lib/components/tasks/TaskAlsoAbout.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { Button } from '$lib';
	import { newTaskDialog } from '$lib/stores/newTask.svelte';
	import { tasksStore } from '$lib/stores/tasks.svelte';
	import { dueLabel, parseDue } from '$lib/tasks/due';
	import { homePatch } from '$lib/tasks/homes';
	import { isAppleKeyboard } from '$lib/utils/platform';
	import { toast } from 'svelte-sonner';

	/** The task being written, shaped like a task so the pickers can edit it. */
	function blank(): Task {
		return {
			...(homePatch(newTaskDialog.home) as Partial<Task>),
			id: 'draft',
			title: '',
			notes: null,
			assignee: 'owner',
			status: 'open',
			due_on: null,
			due_at: null,
			thread_id: newTaskDialog.home?.kind === 'thread' ? newTaskDialog.home.id : null,
			also_about: [],
			origin: 'owner',
			source_url: null,
			sort_order: 0,
			closed_at: null,
			created_at: '',
			updated_at: '',
			result: null,
			result_url: null,
			chat_id: null,
			next_run_at: null,
			attempts: 0,
		} as Task;
	}

	let draft = $state<Task>(blank());
	let saving = $state(false);
	let more = $state(false);
	let dueOpen = $state(false);
	let titleInput: HTMLInputElement | undefined = $state();

	// A fresh task each time the dialog opens, with the opener's home.
	$effect(() => {
		if (newTaskDialog.isOpen) {
			draft = blank();
			requestAnimationFrame(() => titleInput?.focus());
		}
	});

	/** The date typed at the end of the title, when there is one. */
	const typed = $derived(parseDue(draft.title));
	const due = $derived(
		draft.due_on ? { on: draft.due_on, at: draft.due_at } : typed.due_on ? { on: typed.due_on, at: typed.due_at } : null,
	);
	const createKeys = isAppleKeyboard ? '⌘Return' : 'Ctrl+Enter';

	function edit(patch: TaskPatch) {
		draft = { ...draft, ...patch } as Task;
	}

	async function create() {
		const title = (draft.due_on ? draft.title.trim() : typed.title).trim();
		if (!title || saving) return;
		saving = true;
		try {
			await tasksStore.create({
				title,
				notes: draft.notes?.trim() || null,
				assignee: draft.assignee,
				due_on: due?.on ?? null,
				due_at: due?.at ?? null,
				thread_id: draft.thread_id,
				thing_id: draft.thing_id ?? null,
				person_id: draft.person_id ?? null,
				place_id: draft.place_id ?? null,
				org_id: draft.org_id ?? null,
				also_about: draft.also_about ?? [],
			});
			if (more) {
				const keep = { home: newTaskDialog.home };
				draft = blank();
				newTaskDialog.home = keep.home;
				titleInput?.focus();
			} else {
				newTaskDialog.close();
			}
		} catch (e) {
			toast.error("Your server couldn't add that task", {
				description: e instanceof Error ? e.message : 'Try again',
			});
		} finally {
			saving = false;
		}
	}

	function onKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
			e.preventDefault();
			void create();
		}
	}
</script>

<Modal open={newTaskDialog.isOpen} onClose={() => newTaskDialog.close()} title="New task" width="lg">
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="body" onkeydown={onKeydown}>
		<input
			bind:this={titleInput}
			class="title"
			placeholder="What needs doing?"
			aria-label="Title"
			bind:value={draft.title}
			onkeydown={(e) => {
				if (e.key === 'Enter' && !e.metaKey && !e.ctrlKey) {
					e.preventDefault();
					void create();
				}
			}}
		/>
		<textarea
			class="notes"
			placeholder="Description"
			aria-label="Description"
			rows="3"
			value={draft.notes ?? ''}
			oninput={(e) => edit({ notes: e.currentTarget.value })}
		></textarea>

		<div class="facts">
			<span class="fact-cell">
				<Icon icon="atlas:projects" width="14" />
				<TaskHome task={draft} onSave={edit} plain />
			</span>
			<span class="fact-cell">
				<Icon icon="mynaui:calendar" width="14" />
				<Popover bind:open={dueOpen} placement="bottom-start" offset={4}>
					{#snippet trigger({ toggle })}
						<button type="button" class="fact" onclick={toggle} aria-haspopup="dialog">
							{due ? dueLabel(due.on, due.at) : 'No date'}
						</button>
					{/snippet}
					{#snippet children({ close })}
						<DuePicker
							due_on={due?.on ?? null}
							due_at={due?.at ?? null}
							onPick={(on, at) => {
								edit({ due_on: on, due_at: at });
								close();
							}}
						/>
					{/snippet}
				</Popover>
			</span>
			<span class="fact-cell">
				<Icon icon="mynaui:user" width="14" />
				<button
					type="button"
					class="fact"
					aria-pressed={draft.assignee === 'assistant'}
					onclick={() => edit({ assignee: draft.assignee === 'assistant' ? 'owner' : 'assistant' })}
					title="The assistant works a task it is given in the background"
				>
					{draft.assignee === 'assistant' ? 'For the assistant' : 'For you'}
				</button>
			</span>
		</div>
		<div class="also">
			<span class="also-label">Also about</span>
			<TaskAlsoAbout task={draft} onSave={edit} plain />
		</div>
	</div>

	{#snippet footer()}
		<div class="foot">
			<label class="more">
				<input type="checkbox" bind:checked={more} />
				Create more
			</label>
			<span class="hint">{createKeys}</span>
			<Button variant="ghost" size="sm" onclick={() => newTaskDialog.close()}>Cancel</Button>
			<Button variant="primary" size="sm" loading={saving} disabled={!draft.title.trim()} onclick={create}>
				Create task
			</Button>
		</div>
	{/snippet}
</Modal>

<style>
	.body {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}
	.title {
		width: 100%;
		padding: 0;
		border: 0;
		background: transparent;
		font-family: var(--font-serif);
		font-size: var(--md-h2-size);
		line-height: 32px;
		color: var(--color-foreground);
	}
	.title:focus {
		outline: none;
	}
	.title::placeholder,
	.notes::placeholder {
		color: var(--color-foreground-subtle);
	}
	.notes {
		width: 100%;
		min-height: 72px;
		padding: 0;
		border: 0;
		background: transparent;
		font: inherit;
		font-size: 15px;
		line-height: 24px;
		color: var(--color-foreground);
		resize: vertical;
	}
	.notes:focus {
		outline: none;
	}
	.facts {
		display: flex;
		flex-wrap: wrap;
		gap: 8px 24px;
		padding-top: 12px;
		border-top: 1px solid var(--color-border);
	}
	.fact-cell {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		color: var(--color-foreground-muted);
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
	.also {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 8px 12px;
	}
	.also-label {
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	.foot {
		display: flex;
		align-items: center;
		gap: 12px;
		width: 100%;
	}
	.more {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		margin-right: auto;
		font-size: 13px;
		color: var(--color-foreground-muted);
		cursor: pointer;
	}
	.more input {
		margin: 0;
		accent-color: var(--color-primary);
	}
	.hint {
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}
	@media (prefers-reduced-motion: reduce) {
		.fact {
			transition: none;
		}
	}
</style>
