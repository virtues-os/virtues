import { describe, expect, it, vi, afterEach } from 'vitest';
import { appletDestination, appletGlyph, describeSchedule, errorHeadline, limitsOf, relativeTime } from './palette';

describe('describeSchedule', () => {
	it('reads daily and interval schedules as before', () => {
		expect(describeSchedule('0 */15 * * * *')).toBe('Every 15 min');
		expect(describeSchedule('0 0 7 * * *')).toBe('Daily at 7am');
		expect(describeSchedule('0 30 4 * * *')).toBe('Daily at 4:30am');
		expect(describeSchedule(null)).toBe('On demand');
	});

	it('reads weekly schedules in words', () => {
		expect(describeSchedule('0 0 18 * * 1')).toBe('Mondays at 6pm');
		expect(describeSchedule('0 0 17 * * FRI')).toBe('Fridays at 5pm');
		expect(describeSchedule('0 0 9 * * 1-5')).toBe('Weekdays at 9am');
		expect(describeSchedule('0 30 10 * * 0,6')).toBe('Weekends at 10:30am');
		expect(describeSchedule('0 0 8 * * 1,4')).toBe('Mondays and Thursdays at 8am');
		expect(describeSchedule('0 0 8 * * 1,3,5')).toBe('Mondays, Wednesdays and Fridays at 8am');
	});

	it('returns the cron unchanged for syntax it does not read', () => {
		expect(describeSchedule('0 0 8 * * 1#2')).toBe('0 0 8 * * 1#2');
		expect(describeSchedule('0 0 8 1 * *')).toBe('0 0 8 1 * *');
	});
});

describe('relativeTime', () => {
	afterEach(() => vi.useRealTimers());

	it('reads future times forward instead of as "now"', () => {
		vi.useFakeTimers();
		vi.setSystemTime(new Date('2026-10-06T12:00:00Z'));
		expect(relativeTime('2026-10-06T15:00:00Z')).toBe('in 3h');
		expect(relativeTime('2026-10-06T12:20:00Z')).toBe('in 20m');
		expect(relativeTime('2026-10-07T12:00:00Z')).toBe('tomorrow');
		expect(relativeTime('2026-10-06T09:00:00Z')).toBe('3h ago');
		expect(relativeTime('2026-10-05T12:00:00Z')).toBe('yesterday');
	});
});

describe('limitsOf', () => {
	it('names the unit of every ceiling', () => {
		expect(
			limitsOf({ limits: { max_llm_cost: 0.5, max_llm_cost_per_day: 2, max_runs_per_day: 1, max_runs_per_hour: 6 } })
		).toEqual(['$0.50 a run', '$2.00 a day', '1 run a day', '6 runs an hour']);
	});

	it('is empty when nothing is set', () => {
		expect(limitsOf({})).toEqual([]);
		expect(limitsOf(null)).toEqual([]);
	});
});

describe('appletGlyph', () => {
	const a = (over: Record<string, unknown>) =>
		({ origin: 'user', has_face: false, config: {}, ...over }) as Parameters<typeof appletGlyph>[0];

	it('names where the work goes, as far as the server knows', () => {
		expect(appletGlyph(a({ origin: 'system' }))).toBe('settings');
		expect(appletGlyph(a({ origin: 'source', has_face: true }))).toBe('settings');
		expect(appletGlyph(a({ has_face: true, config: { chat_id: 'chat_1' } }))).toBe('dashboard');
		expect(appletGlyph(a({ config: { chat_id: 'chat_1' } }))).toBe('chats');
		expect(appletGlyph(a({}))).toBe('applets');
	});

	it('takes what the applet says it makes first', () => {
		expect(appletGlyph(a({ config: { delivers: { kind: 'page' }, chat_id: 'chat_1' } }))).toBe('pages');
		expect(appletGlyph(a({ config: { delivers: { kind: 'dashboard' } } }))).toBe('dashboard');
		expect(appletGlyph(a({ config: { delivers: { kind: 'text' } } }))).toBe('applets');
	});
});

describe('appletDestination', () => {
	const a = (over: Record<string, unknown>) =>
		({ origin: 'user', has_face: false, config: {}, ...over }) as Parameters<typeof appletDestination>[0];

	it('says where the work goes in the words the icon means', () => {
		expect(appletDestination(a({ config: { delivers: { kind: 'page', title: 'Morning Examen pages' } } }))).toBe(
			'Morning Examen pages'
		);
		expect(appletDestination(a({ config: { delivers: { kind: 'page' } } }))).toBe('Pages');
		expect(appletDestination(a({ origin: 'system' }))).toBe('This server');
		expect(appletDestination(a({ has_face: true }))).toBe('Its dashboard');
		expect(appletDestination(a({ config: { chat_id: 'c' } }))).toBe('Its conversation');
		expect(appletDestination(a({}))).toBe('-');
	});
});

describe('errorHeadline', () => {
	it('keeps the reason and drops the trace', () => {
		expect(errorHeadline('Gmail is disconnected.\n  at sync (gmail.rs:42)')).toBe('Gmail is disconnected.');
		expect(errorHeadline('x'.repeat(200), 10)).toBe('xxxxxxxxx…');
	});
});
