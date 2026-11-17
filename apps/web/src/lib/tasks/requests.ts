/**
 * A request is a task given to the assistant, which works it in the
 * background. These say where one stands, for its row and its page.
 */
import type { Task } from '$lib/api/client';
import { dueLabel, isoDay } from '$lib/tasks/due';
import { isClosed } from '$lib/tasks/actions';

export function isRequest(t: Pick<Task, 'assignee' | 'status'>): boolean {
	return t.assignee === 'assistant' && !isClosed(t);
}

/** A request's state in a word or two. */
export function requestState(t: Task, now: number = Date.now()): string {
	if (t.status === 'ready') return 'Ready';
	if (t.status === 'active') return 'Working';
	if (t.status === 'open' && t.next_run_at) {
		const at = new Date(t.next_run_at);
		if (at.getTime() - now > 60_000) return `Starts ${dueLabel(isoDay(at), at.toISOString())}`;
		return 'Queued';
	}
	return 'Not started';
}

/** Where a bring-your-own-AI endpoint is, in the owner's words. */
export function placeOf(endpoint: string | null | undefined): string {
	let host = '';
	try {
		host = endpoint ? new URL(endpoint).hostname : '';
	} catch {
		return 'on your own account';
	}
	if (host === 'localhost' || host === '::1' || host === '[::1]' || host.startsWith('127.')) return 'on your server';
	if (/^(10\.|192\.168\.|172\.(1[6-9]|2\d|3[01])\.)/.test(host) || host.endsWith('.local')) return 'on your network';
	return 'on your own account';
}
