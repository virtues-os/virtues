/**
 * Universal date utilities for Virtues.
 *
 * Problem: JavaScript's Date.toISOString() uses UTC.
 * At 8:44 PM Eastern on Feb 10, toISOString() returns "2026-02-11T01:44:00.000Z"
 * — splitting on 'T' gives "2026-02-11", which is tomorrow.
 *
 * These utilities use the browser's local timezone consistently.
 */

/**
 * Get today's date as a YYYY-MM-DD slug in the user's local timezone.
 *
 * This is the correct replacement for `new Date().toISOString().split('T')[0]`
 * which returns the UTC date (wrong after ~7 PM Eastern, ~4 PM Pacific, etc.)
 */
export function getLocalDateSlug(date: Date = new Date()): string {
	const year = date.getFullYear();
	const month = String(date.getMonth() + 1).padStart(2, '0');
	const day = String(date.getDate()).padStart(2, '0');
	return `${year}-${month}-${day}`;
}

/**
 * Format a date value as a local calendar date. Accepts a string, epoch number,
 * Date, or nullish (returns '' — unlike raw `toLocaleDateString`, which renders
 * "Invalid Date"). Default style: "Feb 10, 2026".
 *
 * This is the canonical replacement for inline `new Date(x).toLocaleDateString()`.
 */
export function formatDate(
	value: string | number | Date | null | undefined,
	opts: Intl.DateTimeFormatOptions = { year: 'numeric', month: 'short', day: 'numeric' },
): string {
	if (value == null) return '';
	const date = value instanceof Date ? value : new Date(value);
	if (isNaN(date.getTime())) return '';
	return date.toLocaleDateString('en-US', opts);
}

/**
 * Format a date value as a local date + time. Null-safe (returns '').
 * Default style: medium date + short time, e.g. "Feb 10, 2026, 8:44 PM".
 *
 * Canonical replacement for inline `new Date(x).toLocaleString()`.
 */
export function formatDateTime(
	value: string | number | Date | null | undefined,
	opts: Intl.DateTimeFormatOptions = { dateStyle: 'medium', timeStyle: 'short' },
): string {
	if (value == null) return '';
	const date = value instanceof Date ? value : new Date(value);
	if (isNaN(date.getTime())) return '';
	return date.toLocaleString('en-US', opts);
}

/**
 * Format a date as a human-readable string in the user's local timezone.
 * e.g. "Tuesday, February 10, 2026"
 */
export function formatLongDate(date: Date): string {
	return date.toLocaleDateString('en-US', {
		weekday: 'long',
		year: 'numeric',
		month: 'long',
		day: 'numeric',
	});
}

/**
 * Format a UTC timestamp string for relative display.
 * Shows time for today, "Yesterday" for yesterday, weekday name for last 7 days,
 * and short date for older.
 */
export function formatRelativeTimestamp(dateStr: string): string {
	const date = new Date(dateStr);
	const now = new Date();

	// Compare using local dates (not UTC)
	const dateLocal = new Date(date.getFullYear(), date.getMonth(), date.getDate());
	const nowLocal = new Date(now.getFullYear(), now.getMonth(), now.getDate());
	const diffDays = Math.round((nowLocal.getTime() - dateLocal.getTime()) / (1000 * 60 * 60 * 24));

	if (diffDays === 0) {
		return date.toLocaleTimeString('en-US', {
			hour: 'numeric',
			minute: '2-digit',
		});
	} else if (diffDays === 1) {
		return 'Yesterday';
	} else if (diffDays < 7) {
		return date.toLocaleDateString('en-US', { weekday: 'long' });
	} else {
		return date.toLocaleDateString('en-US', {
			month: 'short',
			day: 'numeric',
			year: now.getFullYear() !== date.getFullYear() ? 'numeric' : undefined,
		});
	}
}

/**
 * Format a timestamp as a compact "time ago" string.
 * "Just now", "5m ago", "3h ago", "2d ago", then short date.
 *
 * Use for system activity (sync times, job times, version history).
 * For content listing (chats, pages), use formatRelativeTimestamp() instead.
 */
export function formatTimeAgo(timestamp: string | null): string {
	if (!timestamp) return 'Never';

	const date = new Date(timestamp);
	const now = new Date();
	const diffMs = now.getTime() - date.getTime();
	const diffMins = Math.floor(diffMs / 1000 / 60);
	const diffHours = Math.floor(diffMins / 60);
	const diffDays = Math.floor(diffHours / 24);

	if (diffMins < 1) return 'Just now';
	if (diffMins < 60) return `${diffMins}m ago`;
	if (diffHours < 24) return `${diffHours}h ago`;
	if (diffDays < 7) return `${diffDays}d ago`;

	return date.toLocaleDateString('en-US', {
		month: 'short',
		day: 'numeric',
		year: now.getFullYear() !== date.getFullYear() ? 'numeric' : undefined,
	});
}

/**
 * When something happened, or will, worded to sit inside a sentence: "just
 * now", "4 minutes ago", "in 20 minutes", "today at 7:00 AM", "yesterday at
 * 11:00 PM", "tomorrow at 9:00 AM", "on Monday at 9:00 AM", "on November 12".
 *
 * Use where a page writes a full sentence ("Ran today at 7:00 AM."). The
 * compact forms above are for columns and lists.
 */
export function whenInWords(timestamp: string, now: number = Date.now()): string {
	const then = new Date(timestamp);
	const t = then.getTime();
	if (Number.isNaN(t)) return '';
	const mins = Math.round(Math.abs(now - t) / 60_000);
	if (mins < 1) return 'just now';
	if (mins < 60) {
		const n = `${mins} minute${mins === 1 ? '' : 's'}`;
		return t <= now ? `${n} ago` : `in ${n}`;
	}
	const midnight = (ms: number) => new Date(ms).setHours(0, 0, 0, 0);
	// Rounded, so a day that gains or loses an hour still counts as one day.
	const days = Math.round((midnight(now) - midnight(t)) / 86_400_000);
	const clock = then.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
	if (days === 0) return `today at ${clock}`;
	if (days === 1) return `yesterday at ${clock}`;
	if (days === -1) return `tomorrow at ${clock}`;
	if (Math.abs(days) < 7) return `on ${then.toLocaleDateString(undefined, { weekday: 'long' })} at ${clock}`;
	return `on ${then.toLocaleDateString(undefined, { month: 'long', day: 'numeric' })}`;
}

/**
 * Parse a date slug (YYYY-MM-DD) into a Date object.
 * Treats the slug as a local date (midnight in the user's timezone).
 *
 * IMPORTANT: Do NOT use `new Date("2026-02-10")` — that parses as UTC midnight,
 * which becomes Feb 9 at 7 PM Eastern. Instead, parse components manually.
 */
export function parseDateSlug(slug: string): Date {
	const [year, month, day] = slug.split('-').map(Number);
	return new Date(year, month - 1, day);
}

/**
 * The day and its part, as home's headline says it: "Friday afternoon".
 * Home shows it for now; a new chat in the essay layout carries it as its
 * name until a real one is written.
 */
export function dayPartHeadline(date: Date): string {
	const day = date.toLocaleDateString(undefined, { weekday: 'long' });
	const h = date.getHours();
	const part = h >= 5 && h < 12 ? 'morning' : h >= 12 && h < 17 ? 'afternoon' : h >= 17 && h < 22 ? 'evening' : 'night';
	return `${day} ${part}`;
}
