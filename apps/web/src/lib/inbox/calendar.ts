/**
 * The Inbox as a month: which day each thing falls on, and the weeks a
 * month's grid shows.
 */
import { isoDay } from '$lib/tasks/due';
import type { InboxItem } from './items';

/**
 * The day a thing falls on, YYYY-MM-DD in local time: a task's due day, a
 * request with no due day by when it runs, and an insight by when it stops
 * mattering. An insight that lasts until midnight belongs to the day that
 * ends there. Anything else has no day and stays in the List.
 */
export function itemDay(it: Pick<InboxItem, 'task' | 'note'>): string | null {
	if (it.task?.due_on) return it.task.due_on;
	if (it.task?.next_run_at) return isoDay(new Date(it.task.next_run_at));
	if (it.note?.until) {
		const at = new Date(it.note.until);
		const midnight = at.getHours() === 0 && at.getMinutes() === 0;
		return isoDay(midnight ? new Date(at.getTime() - 1) : at);
	}
	return null;
}

/** The 42 days a month's grid shows, Sunday first: the weeks that hold the month, padded to six. */
export function monthDays(month: Date): string[] {
	const first = new Date(month.getFullYear(), month.getMonth(), 1);
	const start = new Date(first.getFullYear(), first.getMonth(), 1 - first.getDay());
	return Array.from({ length: 42 }, (_, i) => isoDay(new Date(start.getFullYear(), start.getMonth(), start.getDate() + i)));
}

/** The things on each day, in the order given. */
export function byDay(items: InboxItem[]): Map<string, InboxItem[]> {
	const out = new Map<string, InboxItem[]>();
	for (const it of items) {
		const day = itemDay(it);
		if (!day) continue;
		const list = out.get(day);
		if (list) list.push(it);
		else out.set(day, [it]);
	}
	return out;
}
