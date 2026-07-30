/**
 * Applet card palette + glyph derivation.
 *
 * Each action's hero card gets a color palette and a time-of-day glyph
 * derived from its schedule. System pipelines that fire hourly but are
 * conceptually morning (e.g. day_summary_eod) can override via
 * `config.category` — if set, it wins over the derived palette.
 */

import type { Applet } from '$lib/api/client';

export type PaletteKey =
	| 'morning'
	| 'midday'
	| 'evening'
	| 'night'
	| 'continuous'
	| 'ondemand';

export interface Palette {
	key: PaletteKey;
	label: string;
	glyph: string; // iconify name
	/** Background: subtle linear gradient stops (CSS) */
	gradient: string;
	/** Accent color (border + badge text) */
	accent: string;
	/** Foreground tint for subtitles on the card */
	foreground: string;
}

const PALETTES: Record<PaletteKey, Palette> = {
	morning: {
		key: 'morning',
		label: 'Morning',
		glyph: 'ri:sun-line',
		gradient: 'linear-gradient(135deg, #fff7ed 0%, #fed7aa 100%)',
		accent: '#c2410c',
		foreground: '#7c2d12'
	},
	midday: {
		key: 'midday',
		label: 'Midday',
		glyph: 'ri:sun-foggy-line',
		gradient: 'linear-gradient(135deg, #f8fafc 0%, #cbd5e1 100%)',
		accent: '#334155',
		foreground: '#0f172a'
	},
	evening: {
		key: 'evening',
		label: 'Evening',
		glyph: 'ri:contrast-2-line',
		gradient: 'linear-gradient(135deg, #fdf4ff 0%, #e9d5ff 100%)',
		accent: '#7e22ce',
		foreground: '#581c87'
	},
	night: {
		key: 'night',
		label: 'Night',
		glyph: 'ri:moon-line',
		gradient: 'linear-gradient(135deg, #eef2ff 0%, #c7d2fe 100%)',
		accent: '#4338ca',
		foreground: '#312e81'
	},
	continuous: {
		key: 'continuous',
		label: 'Continuous',
		glyph: 'ri:loop-left-line',
		gradient: 'linear-gradient(135deg, #ecfeff 0%, #a5f3fc 100%)',
		accent: '#0e7490',
		foreground: '#164e63'
	},
	ondemand: {
		key: 'ondemand',
		label: 'On demand',
		glyph: 'ri:flashlight-line',
		gradient: 'linear-gradient(135deg, #f9fafb 0%, #e5e7eb 100%)',
		accent: '#4b5563',
		foreground: '#1f2937'
	}
};

/**
 * Parse the "hour" field from a cron schedule. Supports both 5-field
 * (minute hour day month dow) and 6-field (sec minute hour day month dow)
 * expressions. Returns null if the hour is a wildcard/list/range (i.e.
 * the action isn't tied to a specific hour).
 */
export function parseCronHour(cron: string | null): number | null {
	if (!cron) return null;
	const fields = cron.trim().split(/\s+/);
	let hourField: string;
	if (fields.length === 6) hourField = fields[2];
	else if (fields.length === 5) hourField = fields[1];
	else return null;
	if (hourField === '*' || hourField.includes('/') || hourField.includes(',') || hourField.includes('-')) {
		return null;
	}
	const n = Number(hourField);
	return Number.isInteger(n) && n >= 0 && n < 24 ? n : null;
}

// Detect "continuous" cadence — e.g. "0 *\/2 * * * *" (every 2 minutes),
// "0 0 *\/1 * * *" (every hour). These are pipelines that tick frequently
// and aren't tied to a time of day.
function isContinuous(cron: string | null): boolean {
	if (!cron) return false;
	const fields = cron.trim().split(/\s+/);
	const slice = fields.length === 6 ? fields.slice(0, 3) : fields.slice(0, 2);
	return slice.some((f) => f.includes('/'));
}

export function paletteFor(action: Applet): Palette {
	// Explicit override via config.category wins over cron-based derivation.
	const override = (action.config?.category as string | undefined) || undefined;
	if (override && override in PALETTES) {
		return PALETTES[override as PaletteKey];
	}

	if (!action.cron_schedule) {
		return PALETTES.ondemand;
	}

	if (isContinuous(action.cron_schedule)) {
		return PALETTES.continuous;
	}

	const hour = parseCronHour(action.cron_schedule);
	if (hour == null) return PALETTES.continuous;
	if (hour >= 5 && hour < 12) return PALETTES.morning;
	if (hour >= 12 && hour < 17) return PALETTES.midday;
	if (hour >= 17 && hour < 22) return PALETTES.evening;
	return PALETTES.night;
}

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
