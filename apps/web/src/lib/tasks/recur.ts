/**
 * How a recurring task reads and which recurrences the menu offers.
 *
 * A task recurs every N days, weeks, months or years, counted from the day
 * it is finished: finishing one opens the next. So one occurrence is open at
 * a time, and a missed one never piles up.
 */
import type { RecurUnit } from '$lib/api/client';

export interface Recurrence {
	every: number;
	unit: RecurUnit;
}

/** The menu's choices, shortest first. */
export const RECUR_CHOICES: Recurrence[] = [
	{ every: 1, unit: 'day' },
	{ every: 1, unit: 'week' },
	{ every: 2, unit: 'week' },
	{ every: 1, unit: 'month' },
	{ every: 3, unit: 'month' },
	{ every: 6, unit: 'month' },
	{ every: 1, unit: 'year' },
];

/** "Every week", "Every 3 months". */
export function recurLabel(every: number, unit: RecurUnit): string {
	return every === 1 ? `Every ${unit}` : `Every ${every} ${unit}s`;
}

/** A task's recurrence, or null when it happens once. */
export function recurrenceOf(task: { recur_every?: number | null; recur_unit?: RecurUnit | null }): Recurrence | null {
	return task.recur_every && task.recur_unit ? { every: task.recur_every, unit: task.recur_unit } : null;
}
