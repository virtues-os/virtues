/**
 * A task's history (`app_task_events`) as the owner reads it: one line per
 * event, signed by who did it. The task list and the task page both show it.
 */
import type { TaskEvent } from '$lib/api/client';

/** The words for each edited field. Every home field reads as "its home". */
const FIELD_NAMES: Record<string, string> = {
	title: 'the title',
	notes: 'the notes',
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

export function eventLine(e: TaskEvent): string {
	const who = e.author === 'assistant' ? 'The assistant' : 'You';
	const body = e.body as Record<string, unknown>;
	switch (e.kind) {
		case 'created':
			return body.recurred_from ? 'It came back after the last one was finished' : `${who} added this`;
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
