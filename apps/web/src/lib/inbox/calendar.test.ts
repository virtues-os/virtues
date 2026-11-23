// The Inbox as a month. Every task and insight is invented.
import { describe, expect, it } from 'vitest';
import type { Note, Task } from '$lib/api/client';
import { byDay, itemDay, monthDays } from './calendar';
import type { InboxItem } from './items';

const item = (over: Partial<InboxItem>): InboxItem => ({ key: 'k', kind: 'task', ring: 2, noise: 0, title: 'Oil the hinges', meta: '', threadId: null, ...over });

describe('itemDay', () => {
	it('puts a task on its due day, and a request with none on the day it runs', () => {
		expect(itemDay(item({ task: { due_on: '2026-11-20' } as Task }))).toBe('2026-11-20');
		expect(itemDay(item({ task: { due_on: null, next_run_at: new Date(2026, 10, 21, 9).toISOString() } as Task }))).toBe('2026-11-21');
	});

	it('puts an insight on the day it stops mattering, and one that lasts until midnight on the day before', () => {
		expect(itemDay(item({ note: { until: new Date(2026, 10, 24, 18, 30).toISOString() } as Note }))).toBe('2026-11-24');
		expect(itemDay(item({ note: { until: new Date(2026, 10, 25, 0, 0).toISOString() } as Note }))).toBe('2026-11-24');
	});

	it('leaves a thing with no day out', () => {
		expect(itemDay(item({}))).toBeNull();
		expect(byDay([item({})]).size).toBe(0);
	});
});

describe('monthDays', () => {
	it('shows six weeks from the Sunday on or before the first', () => {
		const days = monthDays(new Date(2026, 10, 1));
		expect(days).toHaveLength(42);
		expect(days[0]).toBe('2026-11-01');
		expect(monthDays(new Date(2026, 11, 1))[0]).toBe('2026-11-29');
	});
});
