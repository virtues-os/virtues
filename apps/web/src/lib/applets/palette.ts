/**
 * Applet schedule and time formatting, and the glyph for an applet's kind.
 *
 * This file used to also carry `paletteFor` and six hardcoded gradient
 * palettes that assigned every applet a "time of day" colour from its cron
 * hour. Nothing ever imported it: the card it was designed for reads a status
 * pulse instead, and the palettes were light-only hex stops that would have
 * broken every dark theme the moment anything did. Deleted rather than
 * repaired — the plan's card is the pulse, not a colour scheme.
 */

import type { Applet } from '$lib/api/client';

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
	const at = clockLabel(hour, min);
	// Daily at HH:MM
	if (day === '*' && dow === '*' && at) {
		return `Daily at ${at}`;
	}
	// Weekly: one or more days of the week at HH:MM
	if (day === '*' && dow !== '*' && at) {
		const days = weekdaysLabel(dow);
		if (days) return `${days} at ${at}`;
	}
	return cron;
}

/** "7am", "4:30am", "6pm"; null when the fields aren't a single fixed time. */
function clockLabel(hour: string, min: string): string | null {
	const h = Number(hour);
	const m = Number(min);
	if (!/^\d+$/.test(hour) || !/^\d+$/.test(min) || h > 23 || m > 59) return null;
	const suffix = h < 12 ? 'am' : 'pm';
	const h12 = h % 12 === 0 ? 12 : h % 12;
	return m === 0 ? `${h12}${suffix}` : `${h12}:${String(m).padStart(2, '0')}${suffix}`;
}

const DAY_NAMES = ['Sundays', 'Mondays', 'Tuesdays', 'Wednesdays', 'Thursdays', 'Fridays', 'Saturdays'];
const DAY_ALIASES: Record<string, number> = { SUN: 0, MON: 1, TUE: 2, WED: 3, THU: 4, FRI: 5, SAT: 6 };

/** Cron's day-of-week field in words: "Mondays", "Weekdays", "Mondays and
 *  Thursdays". Null when it uses syntax this doesn't read (steps, `L`, `#`). */
function weekdaysLabel(dow: string): string | null {
	const days = new Set<number>();
	for (const part of dow.toUpperCase().split(',')) {
		const range = part.split('-');
		const nums = range.map((d) => (d in DAY_ALIASES ? DAY_ALIASES[d] : /^\d$/.test(d) ? Number(d) % 7 : NaN));
		if (nums.some(Number.isNaN) || nums.length > 2) return null;
		if (nums.length === 1) days.add(nums[0]);
		else for (let d = nums[0]; d !== (nums[1] + 1) % 7; d = (d + 1) % 7) days.add(d);
	}
	const sorted = [...days].sort((a, b) => a - b);
	if (sorted.join() === '1,2,3,4,5') return 'Weekdays';
	if (sorted.join() === '0,6') return 'Weekends';
	if (sorted.length === 7) return 'Every day';
	const names = sorted.map((d) => DAY_NAMES[d]);
	return names.length <= 2 ? names.join(' and ') : `${names.slice(0, -1).join(', ')} and ${names.at(-1)}`;
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
	// Future times (a next run) read forward. Every one of them used to read
	// "now", because the seconds were clamped at zero.
	const future = diff < 0;
	const sec = Math.round(Math.abs(diff) / 1000);
	if (sec < 5) return 'now';
	const say = (n: string) => (future ? `in ${n}` : `${n} ago`);
	if (sec < 60) return say(`${sec}s`);
	const min = Math.round(sec / 60);
	if (min < 60) return say(`${min}m`);
	const hr = Math.round(min / 60);
	if (hr < 24) return say(`${hr}h`);
	const d = Math.round(hr / 24);
	if (d === 1) return future ? 'tomorrow' : 'yesterday';
	if (d < 7) return say(`${d}d`);
	return new Date(ts).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}

/**
 * The ceilings an applet declares in `config.limits`, each with its unit, so
 * "$0.50 a run" and "$2.00 a day" can't be confused.
 */
export function limitsOf(config: Record<string, unknown> | null | undefined): string[] {
	const l = (config?.limits ?? {}) as Record<string, unknown>;
	const num = (k: string) => (typeof l[k] === 'number' ? (l[k] as number) : null);
	const out: string[] = [];
	const perRun = num('max_llm_cost');
	const perDay = num('max_llm_cost_per_day');
	const runsDay = num('max_runs_per_day');
	const runsHour = num('max_runs_per_hour');
	if (perRun !== null) out.push(`$${perRun.toFixed(2)} a run`);
	if (perDay !== null) out.push(`$${perDay.toFixed(2)} a day`);
	if (runsDay !== null) out.push(`${runsDay} ${runsDay === 1 ? 'run' : 'runs'} a day`);
	if (runsHour !== null) out.push(`${runsHour} ${runsHour === 1 ? 'run' : 'runs'} an hour`);
	return out;
}

/**
 * The Atlas glyph for where an applet's work goes. What the applet declares
 * (`config.delivers.kind`) comes first; otherwise what the server can see: a
 * live view (its face), its conversation, or the server itself for built-in
 * and source applets. Anything else draws the applets star.
 */
const DELIVERS_GLYPH: Record<string, string> = { page: 'pages', chat: 'chats', dashboard: 'dashboard' };

export function appletGlyph(a: Pick<Applet, 'origin' | 'has_face' | 'config'>): string {
	if (a.origin === 'system' || a.origin === 'source') return 'settings';
	const delivers = (a.config?.delivers as { kind?: unknown } | undefined)?.kind;
	if (typeof delivers === 'string' && DELIVERS_GLYPH[delivers]) return DELIVERS_GLYPH[delivers];
	if (a.has_face) return 'dashboard';
	if (typeof a.config?.chat_id === 'string' && a.config.chat_id) return 'chats';
	return 'applets';
}

/**
 * Where an applet's work goes, in words, for the table's "Goes to" column.
 * The same reading as `appletGlyph`, so the icon and the words agree.
 */
export function appletDestination(a: Pick<Applet, 'origin' | 'has_face' | 'config'>): string {
	const glyph = appletGlyph(a);
	const title = (a.config?.delivers as { title?: unknown } | undefined)?.title;
	if (glyph === 'pages') return typeof title === 'string' && title ? title : 'Pages';
	return (
		{ settings: 'This server', dashboard: 'Its dashboard', chats: 'Its conversation' } as Record<string, string>
	)[glyph] ?? '-';
}

/** A run error's first line: the reason, without the trace under it. */
export function errorHeadline(error: string, max = 160): string {
	const line = error.split('\n')[0].trim();
	return line.length > max ? `${line.slice(0, max - 1)}…` : line;
}
