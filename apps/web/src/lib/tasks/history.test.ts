import { describe, expect, it } from 'vitest';
import type { TaskEvent } from '$lib/api/client';
import { eventLine } from './history';

const event = (
	kind: TaskEvent['kind'],
	body: Record<string, unknown>,
	author: TaskEvent['author'] = 'owner',
): TaskEvent => ({
	id: 1,
	task_id: 'task_1',
	author,
	kind,
	body,
	chat_id: null,
	created_at: '2030-01-02T09:00:00Z',
});

describe('eventLine', () => {
	it('names each edited field once, with its own words', () => {
		expect(eventLine(event('edited', { person_id: null, thread_id: 'nb_1', also_about: [] }))).toBe(
			'You changed its home and what it is also about',
		);
		expect(eventLine(event('edited', { due_on: '2030-01-03', due_at: null }))).toBe('You changed the date and the time');
	});

	it('says who handed a task over', () => {
		expect(eventLine(event('edited', { assignee: 'assistant' }))).toBe('You handed it to the assistant');
		expect(eventLine(event('status', { to: 'done' }, 'assistant'))).toBe('The assistant finished it');
	});
});
