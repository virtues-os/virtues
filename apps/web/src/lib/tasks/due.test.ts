import { describe, expect, it } from 'vitest';
import { dueGroup, dueLabel, parseDue } from './due';

// A Wednesday morning, local time.
const NOW = new Date(2030, 0, 2, 9, 0);

describe('parseDue', () => {
	it('leaves a title with no date alone', () => {
		expect(parseDue('Renew passport', NOW)).toEqual({ title: 'Renew passport', due_on: null, due_at: null });
	});

	it('reads day words', () => {
		expect(parseDue('Call Nick today', NOW).due_on).toBe('2030-01-02');
		expect(parseDue('Laundry tonight', NOW)).toMatchObject({ title: 'Laundry', due_on: '2030-01-02' });
		expect(parseDue('Book the dentist tomorrow', NOW)).toMatchObject({ title: 'Book the dentist', due_on: '2030-01-03' });
		expect(parseDue('Reply in 3 days', NOW).due_on).toBe('2030-01-05');
		expect(parseDue('Reply in 2 weeks', NOW).due_on).toBe('2030-01-16');
	});

	it('reads weekdays, today included, and "next" a week on', () => {
		expect(parseDue('Pay rent fri', NOW).due_on).toBe('2030-01-04');
		expect(parseDue('Pay rent wednesday', NOW).due_on).toBe('2030-01-02');
		expect(parseDue('Pay rent next fri', NOW).due_on).toBe('2030-01-11');
		expect(parseDue('Pay rent by thurs', NOW)).toMatchObject({ title: 'Pay rent', due_on: '2030-01-03' });
	});

	it('reads month and day, rolling past dates to next year', () => {
		expect(parseDue('Taxes apr 15', NOW).due_on).toBe('2030-04-15');
		expect(parseDue('Taxes 15th april', NOW).due_on).toBe('2030-04-15');
		expect(parseDue('Card for David jan 1', NOW).due_on).toBe('2031-01-01');
	});

	it('reads times, in either order with a day', () => {
		const a = parseDue('Call David Okafor tomorrow 5pm', NOW);
		const b = parseDue('Call David Okafor 5pm tomorrow', NOW);
		expect(a.title).toBe('Call David Okafor');
		expect(a).toEqual(b);
		expect(new Date(a.due_at!).getHours()).toBe(17);
		expect(parseDue('Standup at 17:30', NOW)).toMatchObject({ title: 'Standup', due_on: '2030-01-02' });
	});

	it('puts a time already past tomorrow', () => {
		expect(parseDue('Water plants 8am', NOW).due_on).toBe('2030-01-03');
	});

	it('never eats the whole title', () => {
		expect(parseDue('tomorrow', NOW)).toEqual({ title: 'tomorrow', due_on: null, due_at: null });
		expect(parseDue('Read March', NOW).due_on).toBeNull();
		expect(parseDue('Watch Friday Night Lights', NOW).due_on).toBeNull();
	});
});

describe('dueLabel and dueGroup', () => {
	it('names nearby days and dates beyond', () => {
		expect(dueLabel('2030-01-02', null, NOW)).toBe('Today');
		expect(dueLabel('2030-01-03', null, NOW)).toBe('Tomorrow');
		expect(dueLabel('2030-01-01', null, NOW)).toBe('Yesterday');
		expect(dueLabel('2030-01-04', null, NOW)).toBe('Friday');
	});

	it('groups by day', () => {
		expect(dueGroup('2030-01-01', NOW)).toBe('overdue');
		expect(dueGroup('2030-01-02', NOW)).toBe('today');
		expect(dueGroup('2030-01-09', NOW)).toBe('upcoming');
		expect(dueGroup(null, NOW)).toBe('none');
	});
});
