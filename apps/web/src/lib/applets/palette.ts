/**
 * Applet schedule and time formatting.
 *
 * This file used to also carry `paletteFor` and six hardcoded gradient
 * palettes that assigned every applet a "time of day" colour from its cron
 * hour. Nothing ever imported it: the card it was designed for reads a status
 * pulse instead, and the palettes were light-only hex stops that would have
 * broken every dark theme the moment anything did. Deleted rather than
 * repaired — the plan's card is the pulse, not a colour scheme.
 */

/**
 * Human-readable schedule label, e.g. "Daily at 7am" or "Every 15 min".
 */
export function describeSchedule(cron: string | null): string {
	if (!cron) return 'On demand';
	const fields = cron.trim().split(/\s+/);
	let min: string, hour: string, day: string, dow: string;
	if (fields.length === 6) {
		[, min, hour, day, , dow] = fields;
	} else if (fields.length === 5) {
		[min, hour, day, , dow] = fields;
	} else {
		return cron;
	}

	// Every N minutes
	if (min.startsWith('*/') && hour === '*' && day === '*' && dow === '*') {
		return `Every ${min.slice(2)} min`;
	}
	// Every N hours
	if (min === '0' && hour.startsWith('*/') && day === '*' && dow === '*') {
		return `Every ${hour.slice(2)}h`;
	}
	// Hourly at :MM
	if (hour === '*' && day === '*' && dow === '*') {
		return min === '0' ? 'Hourly' : `Every hour at :${min.padStart(2, '0')}`;
	}
	// Daily at HH:MM
	if (day === '*' && dow === '*') {
		const h = Number(hour);
		if (Number.isInteger(h)) {
			const ampm = h === 0 ? '12am' : h < 12 ? `${h}am` : h === 12 ? '12pm' : `${h - 12}pm`;
			return min === '0' ? `Daily at ${ampm}` : `Daily at ${ampm.replace(/(am|pm)/, `:${min.padStart(2, '0')}$1`)}`;
		}
	}
	return cron;
}

/**
 * Relative time formatter used in card footers and history lists.
 * "5s ago", "2m ago", "3h ago", "yesterday", "Mar 12".
 */
export function relativeTime(ts: string | null | undefined): string {
	if (!ts) return '—';
	const then = new Date(ts).getTime();
	if (Number.isNaN(then)) return ts;
	const diff = Date.now() - then;
	const sec = Math.max(0, Math.round(diff / 1000));
	if (sec < 5) return 'now';
	if (sec < 60) return `${sec}s ago`;
	const min = Math.round(sec / 60);
	if (min < 60) return `${min}m ago`;
	const hr = Math.round(min / 60);
	if (hr < 24) return `${hr}h ago`;
	const d = Math.round(hr / 24);
	if (d === 1) return 'yesterday';
	if (d < 7) return `${d}d ago`;
	return new Date(ts).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}
