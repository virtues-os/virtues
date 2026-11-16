import { describe, expect, it } from 'vitest';
import { RECUR_CHOICES, recurLabel, recurrenceOf } from './recur';

describe('recurLabel', () => {
	it('reads one unit without a count and several with one', () => {
		expect(recurLabel(1, 'week')).toBe('Every week');
		expect(recurLabel(3, 'month')).toBe('Every 3 months');
		expect(recurLabel(2, 'year')).toBe('Every 2 years');
	});

	it('labels every choice the menu offers differently', () => {
		const labels = RECUR_CHOICES.map((c) => recurLabel(c.every, c.unit));
		expect(new Set(labels).size).toBe(labels.length);
	});
});

describe('recurrenceOf', () => {
	it('is null for a task that happens once', () => {
		expect(recurrenceOf({ recur_every: null, recur_unit: null })).toBeNull();
		// A box older than recurrence sends neither field.
		expect(recurrenceOf({})).toBeNull();
	});

	it('reads the pair off a task that recurs', () => {
		expect(recurrenceOf({ recur_every: 6, recur_unit: 'month' })).toEqual({ every: 6, unit: 'month' });
	});
});
