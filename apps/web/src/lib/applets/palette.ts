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

import type { Applet, AppletLogEntry, AppletWatch } from '$lib/api/client';

/** A data trigger in words: "On new workouts", "30 min before calendar events". */
export function describeWatch(w: AppletWatch): string {
	const offset = w.minutes % 60 === 0 ? `${w.minutes / 60}h` : `${w.minutes} min`;
	switch (w.kind) {
		case 'new':
			return `On new ${w.label}`;
		case 'before':
			return `${offset} before ${w.label}`;
		case 'after_end':
			return `${offset} after ${w.label} end`;
	}
}

/** When an applet starts on its own: its schedule, its data trigger, or both. */
export function describeTriggers(a: Pick<Applet, 'schedule' | 'watch'>): string {
	const parts = [a.schedule ? describeSchedule(a.schedule) : null, a.watch ? describeWatch(a.watch) : null];
	const said = parts.filter((p): p is string => p !== null);
	return said.length ? said.join(' · ') : 'On demand';
}

/**
 * Human-readable schedule label, e.g. "Daily at 7am" or "Every 15 min".
 */
export function describeSchedule(cron: string | null): string {
	if (!cron) return 'On demand';
	const fields = cron.trim().split(/\s+/);
	let min: string, hour: string, day: string, month: string, dow: string;
	if (fields.length === 6) {
		[, min, hour, day, month, dow] = fields;
	} else if (fields.length === 5) {
		[min, hour, day, month, dow] = fields;
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
	// Monthly: one day of the month at HH:MM
	if (/^\d+$/.test(day) && Number(day) >= 1 && Number(day) <= 31 && month === '*' && dow === '*' && at) {
		return `Monthly on the ${ordinal(Number(day))} at ${at}`;
	}
	return cron;
}

/** "1st", "2nd", "3rd", "11th", "22nd". */
function ordinal(n: number): string {
	const teen = n % 100 >= 11 && n % 100 <= 13;
	const suffix = teen ? 'th' : ({ 1: 'st', 2: 'nd', 3: 'rd' } as Record<number, string>)[n % 10] ?? 'th';
	return `${n}${suffix}`;
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
		{ dashboard: 'Its dashboard', chats: 'Its conversation' } as Record<string, string>
	)[glyph] ?? '-';
}

/**
 * The fix a failed run needs when running it again can't help: an account
 * whose sign-in lapsed has to be reconnected first. Null means Retry is the
 * fix. "Gmail is disconnected" names the account, so the button can too.
 */
export function runFix(error: string | null): { label: string; href: string } | null {
	if (!error) return null;
	const named = error.match(/^([A-Z][\w.]*(?: [A-Z][\w.]*)*) is disconnected/);
	const lapsed = /sign-in expired|invalid_grant|reauthori[sz]e|token (?:has )?expired|401 unauthorized/i.test(error);
	if (!named && !lapsed) return null;
	return { label: named ? `Reconnect ${named[1]}` : 'Reconnect', href: '/sources' };
}

/** A run error's first line: the reason, without the trace under it. */
export function errorHeadline(error: string, max = 160): string {
	const line = error.split('\n')[0].trim();
	return line.length > max ? `${line.slice(0, max - 1)}…` : line;
}

/**
 * The kinds of failure a run's error text names, each with the sentence a
 * person reads. The patterns are the runner's own strings
 * (`applet_runner/mod.rs`) and the usual shapes of an HTTP or OS error. The
 * first match wins, so a narrow kind sits above a broad one: an expired
 * sign-in is also a failed request.
 */
const PLAIN_REASONS: { pattern: RegExp; text: string }[] = [
	{
		pattern: /sign-in expired|invalid_grant|reauthori[sz]e|token (?:has )?expired|401 unauthorized/i,
		text: 'An account it uses needs you to sign in again',
	},
	{
		pattern: /failed to load credential|credential refresh failed|could not route credential/i,
		text: "It couldn't use the account it's connected to",
	},
	{ pattern: /timed out|timeout|deadline exceeded/i, text: 'It took too long and was stopped' },
	{ pattern: /rate limit|too many requests|\b429\b/i, text: 'A service it uses asked it to slow down' },
	{
		pattern: /overloaded|bad gateway|service unavailable|gateway time-?out|internal server error|\b50[0234]\b/i,
		text: 'A service it uses had a problem',
	},
	{
		pattern:
			/error sending request|connection refused|connection reset|dns error|failed to lookup address|network is unreachable|no route to host|^Network error/i,
		text: "It couldn't reach a service it needs",
	},
	{ pattern: /failed to spawn action command|action command is empty/i, text: "Its program couldn't start" },
	{ pattern: /permission denied|os error 13\b|operation not permitted/i, text: "It wasn't allowed to open a file it needs" },
	{
		pattern: /condition sql failed|^SQL error|^Database error|error returned from database/i,
		text: 'A query it runs has an error',
	},
	{
		pattern: /subprocess exited with status|panicked at|Traceback \(most recent call last\)|failed to wait for action subprocess/i,
		text: 'Its program stopped with an error',
	},
];

/**
 * Why a run failed, as a sentence without a final period, or null when the
 * error is none of the kinds above. The run log keeps the error's own text.
 */
export function plainReason(error: string): string | null {
	return PLAIN_REASONS.find((r) => r.pattern.test(error))?.text ?? null;
}

/**
 * What a row or a page says about a failed run: the plain sentence for a
 * known kind of failure, and otherwise the error's first line, which is
 * already a sentence when the server wrote it for a person ("Add
 * OPENAI_API_KEY in Settings, under Keys"). No final period either way.
 */
export function failureReason(error: string, max = 160): string {
	return plainReason(error) ?? errorHeadline(error, max).replace(/\.$/, '');
}

const STATUS_WORD: Record<AppletLogEntry['status'], string> = {
	success: 'Ran',
	error: 'Failed',
	running: 'Running',
	skipped: 'Skipped',
	cancelled: 'Cancelled',
	budget_exceeded: 'Stopped at its spending limit',
};

/** One line of a run log in words: what the run said, or why it failed. */
export function runLine(e: AppletLogEntry): string {
	const what = e.status === 'error' ? `Failed${e.error ? `: ${errorHeadline(e.error, 120)}` : ''}` : (e.summary ?? STATUS_WORD[e.status]);
	return e.occurrences > 1 ? `${what} (${e.occurrences} times)` : what;
}

/** The hosts a job's logged runs reached, each once, in the order first seen. */
export function hostsReached(log: AppletLogEntry[]): string[] {
	const hosts: string[] = [];
	for (const e of log) {
		for (const h of e.requests ?? []) {
			if (h.requests > h.refused && !hosts.includes(h.host)) hosts.push(h.host);
		}
	}
	return hosts;
}
