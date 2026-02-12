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
