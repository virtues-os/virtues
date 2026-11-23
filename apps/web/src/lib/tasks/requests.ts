/**
 * A request is a task given to the assistant, which works it in the
 * background. These say where one stands, for its row and its page.
 */
import type { Task } from '$lib/api/client';
import { dueLabel, isoDay } from '$lib/tasks/due';
import { isClosed } from '$lib/tasks/actions';
import { appletsStore } from '$lib/stores/applets.svelte';
import { describeTriggers } from '$lib/applets/palette';
import { problemOf } from '$lib/applets/standing';

export function isRequest(t: Pick<Task, 'assignee' | 'status'>): boolean {
	return t.assignee === 'assistant' && !isClosed(t);
}

/**
 * A task that repeats: one the assistant does over and over, such as a
 * weekly review. Its applet runs it, each run replaces its result, and
 * opening it is all a new result asks.
 */
export function isRepeating(t: Pick<Task, 'applet_id'>): boolean {
	return !!t.applet_id;
}

/** The applet that runs a task that repeats, once the applets have loaded. */
export function appletOf(t: Pick<Task, 'applet_id'>) {
	return t.applet_id ? (appletsStore.list.find((a) => a.id === t.applet_id) ?? null) : null;
}

/** Where a task that repeats stands: a new result, what went wrong, that it is off, or when it runs. */
function repeatingState(t: Task, now: number): string {
	if (t.status === 'ready') return 'New';
	const applet = appletOf(t);
	if (!applet) return 'Repeats';
	if (!applet.enabled) return 'Off';
	return problemOf(applet, now) ?? describeTriggers(applet);
}

/** A request's state in a word or two. */
export function requestState(t: Task, now: number = Date.now()): string {
	if (isRepeating(t)) return repeatingState(t, now);
	if (t.status === 'ready') return 'Ready';
	if (t.status === 'active') return 'Working';
	if (t.status === 'open' && t.next_run_at) {
		const at = new Date(t.next_run_at);
		if (at.getTime() - now > 60_000) return `Starts ${dueLabel(isoDay(at), at.toISOString())}`;
		return 'Queued';
	}
	return 'Not started';
}
