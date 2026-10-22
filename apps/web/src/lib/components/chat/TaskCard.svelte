<script lang="ts">
	/**
	 * Tasks the assistant added or changed (`create_task`, `update_task`), in
	 * the reply where it did so. Each row is the task itself, as the list
	 * draws it, with a checkbox that works: finishing a task from the chat
	 * is the same act as finishing it from Tasks. The rows read the shared
	 * store once it holds the task, so a tick in either place shows in both.
	 */
	import type { Task } from "$lib/api/client";
	import { tasksStore } from "$lib/stores/tasks.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { dueGroup, dueLabel } from "$lib/tasks/due";
	import { toast } from "svelte-sonner";

	let { tasks, verb }: { tasks: Task[]; verb: "added" | "updated" } = $props();

	const rows = $derived(tasks.map((t) => tasksStore.find(t.id) ?? t));
	const heading = $derived(
		verb === "added"
			? tasks.length === 1
				? "Added to Tasks"
				: `Added ${tasks.length} tasks`
			: "Updated in Tasks",
	);

	async function toggle(t: Task) {
		const closing = t.status !== "done" && t.status !== "dismissed";
		try {
			// The store may not hold this task yet (Tasks never opened): seed it.
			if (!tasksStore.find(t.id)) tasksStore.open = [...tasksStore.open, t];
			await tasksStore.update(t.id, { status: closing ? "done" : "open" });
		} catch (e) {
			toast.error("Your server couldn't update that task", {
				description: e instanceof Error ? e.message : undefined,
			});
		}
	}

	function openTasks() {
		windowShellStore.openTabFromRoute("/tasks", { label: "Tasks", focusExisting: true });
	}
</script>

<figure class="task-card">
	<figcaption>
		<button type="button" class="heading" onclick={openTasks}>{heading}</button>
	</figcaption>
	<ul>
		{#each rows as t (t.id)}
			{@const closed = t.status === "done" || t.status === "dismissed"}
			<li class:closed class:dismissed={t.status === "dismissed"}>
				<button
					type="button"
					class="check"
					class:checked={t.status === "done"}
					role="checkbox"
					aria-checked={t.status === "done"}
					aria-label={closed ? `Reopen ${t.title}` : `Complete ${t.title}`}
					onclick={() => toggle(t)}
				>
					{#if t.status === "done"}
						<svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true"
							><path d="M4 8.3 6.7 11 12 5.2" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" /></svg
						>
					{/if}
				</button>
				<span class="title">{t.title}</span>
				{#if t.status === "dismissed"}
					<span class="chip">Dismissed</span>
				{:else if t.status === "active"}
					<span class="chip">Working on it</span>
				{:else if t.status === "ready"}
					<span class="chip">Ready</span>
				{:else if t.assignee === "assistant" && t.next_run_at}
					<span class="chip">Queued</span>
				{/if}
				{#if t.due_on && !closed}
					<span class="chip" class:overdue={dueGroup(t.due_on) === "overdue"}>{dueLabel(t.due_on, t.due_at)}</span>
				{/if}
				{#if t.assignee === "assistant"}
					<span class="chip">Assistant</span>
				{/if}
			</li>
		{/each}
	</ul>
</figure>

<style>
	.task-card {
		margin: 0.75rem 0;
		max-width: 40em;
		padding: 0.625rem 0.875rem 0.5rem;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-xl);
		background: var(--color-surface);
	}
	figcaption {
		font-family: var(--font-sans);
		font-size: 13px;
		color: var(--color-foreground-muted);
		margin-bottom: 0.25rem;
	}
	.heading {
		background: none;
		border: none;
		padding: 0;
		font: inherit;
		color: inherit;
		cursor: pointer;
	}
	.heading:hover {
		color: var(--color-foreground);
	}
	ul {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	li {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		min-height: 2rem;
		font-family: var(--font-sans);
		font-size: 0.9375rem;
		color: var(--color-foreground);
	}
	.title {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.closed .title {
		color: var(--color-foreground-muted);
	}
	li.closed:not(.dismissed) .title {
		text-decoration: line-through;
		text-decoration-color: var(--color-foreground-subtle);
	}
	.check {
		flex-shrink: 0;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 18px;
		height: 18px;
		border-radius: 50%;
		border: 1.5px solid var(--color-foreground-subtle);
		background: transparent;
		color: var(--color-background);
		cursor: pointer;
	}
	.check.checked {
		background: var(--color-foreground-muted);
		border-color: var(--color-foreground-muted);
	}
	.chip {
		flex-shrink: 0;
		padding: 0.0625rem 0.5rem;
		font-size: 0.75rem;
		border-radius: var(--radius-full);
		background: color-mix(in srgb, var(--wash-ink) 7%, transparent);
		color: var(--color-foreground-muted);
		white-space: nowrap;
	}
	.chip.overdue {
		color: var(--color-error);
	}
</style>
