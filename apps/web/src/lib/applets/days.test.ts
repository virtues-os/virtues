import { describe, expect, it } from 'vitest';
import type { Applet, RunDay } from '$lib/api/client';
import {
	appletDays,
	busiestDay,
	countRuns,
	indexRunDays,
	lastDays,
	needsYou,
	recentSummary,
	reliabilityScore,
	stateOf,
	type DayState
} from './days';

const applet = (over: Partial<Applet> = {}): Applet =>
	({ id: 'applet_a', enabled: true, archived_at: null, next_due_at: null, ...over }) as Applet;

describe('stateOf', () => {
	it('takes the worst outcome of the day', () => {
		expect(stateOf(new Map([['success', 23], ['error', 1]]))).toBe('failed');
		expect(stateOf(new Map([['success', 5], ['budget_exceeded', 1]]))).toBe('stopped');
		expect(stateOf(new Map([['success', 24]]))).toBe('ran');
		expect(stateOf(new Map([['skipped', 3]]))).toBe('quiet');
		expect(stateOf(undefined)).toBe('none');
	});
});

describe('appletDays', () => {
	const today = new Date('2026-10-06T12:00:00');
	const days = lastDays(3, today);

	it('gives one state per day, oldest first', () => {
		const rows: RunDay[] = [
			{ applet_id: 'applet_a', day: days[0], status: 'success', runs: 1 },
			{ applet_id: 'applet_a', day: days[2], status: 'error', runs: 1 }
		];
		expect(appletDays(applet(), indexRunDays(rows), days, today.getTime())).toEqual(['ran', 'none', 'failed']);
	});

	it('marks today missed only when a due run is more than an hour late', () => {
		const late = applet({ next_due_at: '2026-10-06T10:00:00' });
		const onTime = applet({ next_due_at: '2026-10-06T11:30:00' });
		const index = indexRunDays([]);
		expect(appletDays(late, index, days, today.getTime()).at(-1)).toBe('missed');
		expect(appletDays(onTime, index, days, today.getTime()).at(-1)).toBe('none');
		expect(appletDays({ ...late, enabled: false }, index, days, today.getTime()).at(-1)).toBe('none');
	});
});

describe('reliabilityScore', () => {
	it('ranks failures and misses above stops', () => {
		expect(reliabilityScore(['failed', 'ran'])).toBeGreaterThan(reliabilityScore(['stopped', 'ran']));
		expect(reliabilityScore(['ran', 'quiet', 'none'])).toBe(0);
	});
});

describe('recentSummary', () => {
	const days = lastDays(7, new Date('2026-10-06T12:00:00'));
	const rows = (status: RunDay['status'], runs: number): RunDay[] => [
		{ applet_id: 'applet_a', day: days[6], status, runs }
	];
	const summary = (r: RunDay[], states: DayState[] = days.map(() => 'ran')) =>
		recentSummary(states, countRuns(indexRunDays(r), 'applet_a', days));

	it('leads with failures, counted in runs', () => {
		expect(summary([...rows('success', 20), ...rows('error', 2)])).toBe('2 failed in the last 7 days');
	});

	it('says when today was missed', () => {
		const states = [...days.slice(1).map(() => 'ran' as const), 'missed' as const];
		expect(summary(rows('success', 6), states)).toBe("Didn't run today when it was due");
	});

	it('counts stops at the spending limit', () => {
		expect(summary(rows('budget_exceeded', 1))).toBe('Stopped at its spending limit once in the last 7 days');
	});

	it('tells a quiet week from a clean one', () => {
		expect(summary(rows('success', 3))).toBe('No problems in the last 7 days');
		expect(summary([], days.map(() => 'none' as const))).toBe('No runs in the last 7 days');
	});
});

describe('busiestDay', () => {
	it('finds the day with the most runs', () => {
		const index = indexRunDays([
			{ applet_id: 'applet_a', day: '2026-10-05', status: 'success', runs: 23 },
			{ applet_id: 'applet_a', day: '2026-10-05', status: 'error', runs: 1 },
			{ applet_id: 'applet_a', day: '2026-10-06', status: 'success', runs: 1 }
		]);
		expect(busiestDay(index, 'applet_a')).toBe(24);
		expect(busiestDay(index, 'applet_b')).toBe(0);
	});
});

describe('needsYou', () => {
	const now = new Date('2026-10-06T12:00:00').getTime();
	const failed = { status: 'error', started_at: null, records_processed: null, error: 'x' };

	it('flags a failed last run and an overdue one', () => {
		expect(needsYou(applet({ last_run: failed }), now)).toBe(true);
		expect(needsYou(applet({ next_due_at: '2026-10-06T10:00:00' }), now)).toBe(true);
		expect(needsYou(applet({ next_due_at: '2026-10-06T11:30:00' }), now)).toBe(false);
	});

	it('leaves applets that are off or finished alone', () => {
		expect(needsYou(applet({ last_run: failed, enabled: false }), now)).toBe(false);
		expect(needsYou(applet({ last_run: failed, archived_at: '2026-10-01T00:00:00' }), now)).toBe(false);
	});
});
