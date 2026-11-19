import { describe, expect, it } from 'vitest';
import { namedDays } from './dates';

// A Tuesday afternoon.
const NOW = new Date(2026, 10, 17, 15, 0);

const dates = (q: string) => namedDays(q, NOW).map((d) => d.date);

describe('namedDays', () => {
	it('reads today and yesterday', () => {
		expect(dates('today')).toEqual(['2026-11-17']);
		expect(dates('Yesterday')).toEqual(['2026-11-16']);
	});

	it('gives a bare weekday as the latest one and the one before', () => {
		expect(dates('friday')).toEqual(['2026-11-13', '2026-11-06']);
		expect(dates('tue')).toEqual(['2026-11-17', '2026-11-10']);
	});

	it('never reads "last" as today', () => {
		expect(dates('last tuesday')).toEqual(['2026-11-10']);
		expect(dates('last monday')).toEqual(['2026-11-16']);
	});

	it('reads month and day both ways, and takes the latest one', () => {
		expect(dates('oct 5')).toEqual(['2026-10-05']);
		expect(dates('5 October')).toEqual(['2026-10-05']);
		expect(dates('dec 24')).toEqual(['2025-12-24']);
		expect(dates('oct 5 2024')).toEqual(['2024-10-05']);
	});

	it('reads an ISO date and offers nothing in the future', () => {
		expect(dates('2026-10-05')).toEqual(['2026-10-05']);
		expect(dates('2026-12-01')).toEqual([]);
	});

	it('names nothing for ordinary words and impossible days', () => {
		expect(dates('garden')).toEqual([]);
		expect(dates('su')).toEqual([]);
		expect(dates('feb 30')).toEqual([]);
		expect(dates('may')).toEqual([]);
	});

	it('titles a day, with the year only when it is not this one', () => {
		expect(namedDays('oct 5', NOW)[0].title).toBe('Monday, 5 October');
		expect(namedDays('dec 24', NOW)[0].title).toBe('Wednesday, 24 December 2025');
	});
});
