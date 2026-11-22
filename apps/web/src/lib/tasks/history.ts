/**
 * A task's history (`app_task_events`) as the owner reads it: one line per
 * event, signed by who did it, for the activity in the task view.
 */
import type { TaskEvent } from '$lib/api/client';

/** The words for each edited field. Every home field reads as "its home". */
const FIELD_NAMES: Record<string, string> = {
	title: 'the title',
	notes: 'the description',
	assignee: 'who it is for',
	due_on: 'the date',
	due_at: 'the time',
	thread_id: 'its home',
	thing_id: 'its home',
	person_id: 'its home',
	place_id: 'its home',
	org_id: 'its home',
	also_about: 'what it is also about',
	recur_every: 'how often it repeats',
	recur_unit: 'how often it repeats',
	result_url: 'the result',
};

/** One event as a line. `source` names the record the task was made from, when it has one. */
export function eventLine(e: TaskEvent, source?: string): string {
	const who = e.author === 'assistant' ? 'The assistant' : 'You';
	const body = e.body as Record<string, unknown>;
	switch (e.kind) {
		case 'created':
			if (body.recurred_from) return 'It came back after the last one was finished';
			return source ? `${who} made this from ${source}` : `${who} added this`;
		case 'status': {
			const to = String(body.to ?? '');
			const said: Record<string, string> = {
				ready: 'finished a run and left a result',
				done: e.author === 'owner' ? 'closed it' : 'finished it',
				dismissed: 'dismissed it',
				active: 'started working on it',
				open: e.author === 'owner' ? 'sent it back' : 'put it back in the queue',
			};
			return `${who} ${said[to] ?? 'changed its status'}`;
		}
		case 'edited': {
			const fields = Object.keys(body);
			if (fields.length === 1 && fields[0] === 'assignee') {
				return body.assignee === 'assistant' ? `${who} handed it to the assistant` : `${who} took it back`;
			}
			const changed = [...new Set(fields.map((f) => FIELD_NAMES[f] ?? f))];
			return `${who} changed ${changed.join(' and ')}`;
		}
		default:
			return String(body.text ?? '');
	}
}

export function eventTime(e: TaskEvent): string {
	return new Date(e.created_at).toLocaleString('en-US', {
		month: 'short',
		day: 'numeric',
		hour: 'numeric',
		minute: '2-digit',
	});
}
