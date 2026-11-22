// Where a request stands, in a word or two. Every task and applet is invented.
import { beforeEach, describe, expect, it } from 'vitest';
import type { Applet, Task } from '$lib/api/client';
import { appletsStore } from '$lib/stores/applets.svelte';
import { describeTriggers } from '$lib/applets/palette';
import { appletOf, isRepeating, requestState } from './requests';

const task = (over: Partial<Task> = {}): Task =>
	({ id: 'task_1', title: 'Week in review', assignee: 'assistant', status: 'open', next_run_at: null, applet_id: null, ...over }) as Task;

const applet = (over: Partial<Applet> = {}): Applet =>
	({
		id: 'applet_user__week_in_review',
		name: 'Week in review',
		origin: 'ai',
		schedule: '0 0 18 * * 0',
		watch: null,
		enabled: true,
		archived_at: null,
		next_due_at: null,
		triggers: ['cron', 'manual'],
		last_run: null,
		...over,
	}) as Applet;

const NOW = new Date('2026-11-18T12:00:00').getTime();

describe('a task that repeats', () => {
	beforeEach(() => {
		appletsStore.list = [];
	});

	it('is told from a one-time request by its applet', () => {
		expect(isRepeating(task())).toBe(false);
		expect(isRepeating(task({ applet_id: 'applet_user__week_in_review' }))).toBe(true);
		expect(requestState(task({ status: 'ready' }), NOW)).toBe('Ready');
	});

	it('says a new result is there, and otherwise when it runs', () => {
		const review = task({ applet_id: 'applet_user__week_in_review' });
		expect(requestState(review, NOW)).toBe('Repeats');

		appletsStore.list = [applet()];
		expect(appletOf(review)?.name).toBe('Week in review');
		expect(requestState(review, NOW)).toBe(describeTriggers(applet()));
		expect(requestState({ ...review, status: 'ready' }, NOW)).toBe('New');
	});

	it('says it is off, or what went wrong, in place of when it runs', () => {
		const review = task({ applet_id: 'applet_user__week_in_review' });
		appletsStore.list = [applet({ enabled: false })];
		expect(requestState(review, NOW)).toBe('Off');

		const failed = { status: 'error', started_at: '2026-11-15T18:00:00', records_processed: null, error: 'timed out' };
		appletsStore.list = [applet({ last_run: failed as Applet['last_run'] })];
		expect(requestState(review, NOW)).toBe('It took too long and was stopped');
	});
});
