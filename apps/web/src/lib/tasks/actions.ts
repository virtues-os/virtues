/**
 * What a to-do's check does, for every view that draws one.
 */
import { toast } from 'svelte-sonner';
import type { Task } from '$lib/api/client';
import { tasksStore } from '$lib/stores/tasks.svelte';
import { dueLabel } from '$lib/tasks/due';

export function isClosed(task: Pick<Task, 'status'>): boolean {
	return task.status === 'done' || task.status === 'dismissed';
}

/**
 * Finish an open to-do, or reopen a closed one. Finishing one that recurs
 * opens the next occurrence on the server, so this says when that one is due
 * and offers the way back.
 */
export async function toggleTaskDone(task: Task): Promise<void> {
	const finishing = !isClosed(task);
	try {
		await tasksStore.update(task.id, { status: finishing ? 'done' : 'open' });
	} catch (e) {
		toast.error("Your server couldn't update that task", {
			description: e instanceof Error ? e.message : undefined,
		});
		return;
	}
	if (!finishing || !task.recur_every) return;
	const next = tasksStore.open.find((t) => t.recurred_from_id === task.id);
	if (!next?.due_on) return;
	toast(`Next one due ${dueLabel(next.due_on, null)}`, {
		action: { label: 'Undo', onClick: () => void toggleTaskDone({ ...task, status: 'done' }) },
	});
}
