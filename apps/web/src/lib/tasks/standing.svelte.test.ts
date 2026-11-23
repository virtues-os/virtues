// Where a task stands, as a sentence. Every task is invented.
import { describe, expect, it } from 'vitest';
import type { Task } from '$lib/api/client';
import { dueFact, standing } from './standing';

const task = (over: Partial<Task> = {}): Task =>
	({ id: 'task_1', title: 'Return the two library books', assignee: 'owner', status: 'open', next_run_at: null, applet_id: null, ...over }) as Task;

const NOW = new Date('2026-11-18T12:00:00').getTime();

describe('standing', () => {
	it('names who has the task', () => {
		expect(standing(task(), NOW)).toBe('This is yours.');
		expect(standing(task({ status: 'done' }), NOW)).toBe('You finished this.');
		expect(standing(task({ status: 'dismissed' }), NOW)).toBe('You put this away.');
	});

	it('says what the assistant is doing with a request', () => {
		const asked = (over: Partial<Task>) => standing(task({ assignee: 'assistant', ...over }), NOW);
		expect(asked({ status: 'active' })).toBe('The assistant is working on this.');
		expect(asked({ status: 'ready' })).toMatch(/^The assistant finished this\./);
		expect(asked({})).toBe("The assistant has this but hasn't started it.");
		expect(asked({ next_run_at: new Date(NOW + 20_000).toISOString() })).toBe('The assistant has this and starts in a moment.');
		expect(asked({ next_run_at: new Date('2026-11-18T18:30:00').toISOString() })).toBe('The assistant has this and starts today 6:30 PM.');
	});

	it('says a start that never came instead of calling it queued', () => {
		const late = standing(task({ assignee: 'assistant', next_run_at: new Date('2026-11-18T09:15:00').toISOString() }), NOW);
		expect(late).toBe("The assistant has this. It was set to start at 9:15 AM and hasn't started yet.");
	});

	it('leaves a task that repeats to its applet', () => {
		expect(standing(task({ assignee: 'assistant', applet_id: 'applet_user__review' }), NOW)).toBeNull();
	});
});

describe('dueFact', () => {
	const now = new Date(NOW);
	it('gives a day ahead in words and a day gone by as its date, never as a count of days', () => {
		expect(dueFact('2026-11-18', null, now)).toBe('Due Today');
		expect(dueFact('2026-11-20', null, now)).toBe('Due Friday');
		expect(dueFact('2026-11-16', null, now)).toBe('Was due Mon, Nov 16');
	});
});
