/**
 * Due dates for tasks, in the words people type: "fri", "tomorrow 5pm",
 * "in 3 days", "jan 5". The add row reads a date off the END of what was
 * typed and shows it as a chip before saving, so a wrong guess is seen and
 * undone rather than saved. Nothing here is clever on purpose: a phrase it
 * does not know stays in the title.
 *
 * Days are the owner's local days. `due_on` is a `YYYY-MM-DD` string and is
 * always sent with `due_at`, so the server never has to guess a day from a
 * UTC instant.
 */

export interface ParsedDue {
	title: string;
	due_on: string | null;
	due_at: string | null;
}

const WEEKDAYS = ['sunday', 'monday', 'tuesday', 'wednesday', 'thursday', 'friday', 'saturday'];

/** `YYYY-MM-DD` for a local date. */
export function isoDay(d: Date): string {
	const m = String(d.getMonth() + 1).padStart(2, '0');
	const day = String(d.getDate()).padStart(2, '0');
	return `${d.getFullYear()}-${m}-${day}`;
}

/** A `YYYY-MM-DD` string as a local midnight. */
export function fromIsoDay(s: string): Date {
	const [y, m, d] = s.split('-').map(Number);
	return new Date(y, m - 1, d);
}

function addDays(d: Date, n: number): Date {
	const out = new Date(d.getFullYear(), d.getMonth(), d.getDate());
	out.setDate(out.getDate() + n);
	return out;
}

const MONTH_NAMES = [
	'january', 'february', 'march', 'april', 'may', 'june',
	'july', 'august', 'september', 'october', 'november', 'december',
];

/** A full name or any prefix of it at least three letters long; "tues", "thurs" and "sept" too. */
function nameIndex(names: string[], word: string): number {
	const w = word.toLowerCase();
	if (w.length < 3) return -1;
	return names.findIndex((n) => n.startsWith(w));
}

const weekdayIndex = (word: string) => nameIndex(WEEKDAYS, word);
const monthIndex = (word: string) => nameIndex(MONTH_NAMES, word);

type DayMatch = { day: Date; rest: string };
type TimeMatch = { hours: number; minutes: number; rest: string };

/** A day phrase at the end of `text`, with an optional "on", "by" or "due" before it. */
function matchDay(text: string, today: Date): DayMatch | null {
	const lead = String.raw`(?:^|\s)(?:(?:on|by|due)\s+)?`;
	const tryRe = (re: RegExp, make: (m: RegExpMatchArray) => Date | null): DayMatch | null => {
		const m = text.match(re);
		if (!m || m.index === undefined) return null;
		const day = make(m);
		return day ? { day, rest: text.slice(0, m.index) } : null;
	};

	return (
		tryRe(new RegExp(`${lead}(today|tonight)$`, 'i'), () => today) ??
		tryRe(new RegExp(`${lead}(tomorrow|tmrw|tmr)$`, 'i'), () => addDays(today, 1)) ??
		tryRe(new RegExp(`${lead}in\\s+(\\d{1,3})\\s+(day|days|week|weeks)$`, 'i'), (m) =>
			addDays(today, Number(m[1]) * (m[2].toLowerCase().startsWith('week') ? 7 : 1)),
		) ??
		tryRe(new RegExp(`${lead}(next\\s+)?([a-z]+)$`, 'i'), (m) => {
			const i = weekdayIndex(m[2]);
			if (i < 0) return null;
			// "fri" is the coming Friday, today included; "next fri" is a week on.
			const ahead = (i - today.getDay() + 7) % 7;
			return addDays(today, m[1] ? ahead + 7 : ahead);
		}) ??
		tryRe(new RegExp(`${lead}([a-z]+)\\.?\\s+(\\d{1,2})(?:st|nd|rd|th)?$`, 'i'), (m) => monthDay(monthIndex(m[1]), Number(m[2]), today)) ??
		tryRe(new RegExp(`${lead}(\\d{1,2})(?:st|nd|rd|th)?\\s+([a-z]+)$`, 'i'), (m) => monthDay(monthIndex(m[2]), Number(m[1]), today)) ??
		tryRe(new RegExp(`${lead}(\\d{4})-(\\d{2})-(\\d{2})$`), (m) => {
			const d = new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3]));
			return isNaN(d.getTime()) || d.getDate() !== Number(m[3]) ? null : d;
		})
	);
}

/** The next time this month and day come round, today included. */
function monthDay(month: number, day: number, today: Date): Date | null {
	if (month < 0 || day < 1 || day > 31) return null;
	let d = new Date(today.getFullYear(), month, day);
	if (d.getMonth() !== month) return null;
	if (d < today) d = new Date(today.getFullYear() + 1, month, day);
	return d;
}

/** A clock time at the end of `text`: "5pm", "5:30 pm", "at 17:00". */
function matchTime(text: string): TimeMatch | null {
	const twelve = text.match(/(?:^|\s)(?:at\s+)?(\d{1,2})(?::(\d{2}))?\s*(am|pm)$/i);
	if (twelve && twelve.index !== undefined) {
		const h = Number(twelve[1]);
		const min = Number(twelve[2] ?? 0);
		if (h < 1 || h > 12 || min > 59) return null;
		const pm = twelve[3].toLowerCase() === 'pm';
		return { hours: (h % 12) + (pm ? 12 : 0), minutes: min, rest: text.slice(0, twelve.index) };
	}
	const day24 = text.match(/(?:^|\s)(?:at\s+)?([01]?\d|2[0-3]):([0-5]\d)$/);
	if (day24 && day24.index !== undefined) {
		return { hours: Number(day24[1]), minutes: Number(day24[2]), rest: text.slice(0, day24.index) };
	}
	return null;
}

/**
 * Read a due date off the end of `input`. The day and the time may come in
 * either order ("tomorrow 5pm", "5pm tomorrow"). A time with no day is today
 * if it is still ahead, else tomorrow. When nothing would be left of the
 * title, nothing is read.
 */
export function parseDue(input: string, now: Date = new Date()): ParsedDue {
	const original = input.trim();
	const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
	let text = original;
	let day: Date | null = null;
	let time: { hours: number; minutes: number } | null = null;

	for (let pass = 0; pass < 2; pass++) {
		if (!time) {
			const t = matchTime(text);
			if (t && t.rest.trim()) {
				time = { hours: t.hours, minutes: t.minutes };
				text = t.rest.trimEnd();
				continue;
			}
		}
		if (!day) {
			const d = matchDay(text, today);
			if (d && d.rest.trim()) {
				day = d.day;
				text = d.rest.trimEnd();
				continue;
			}
		}
		break;
	}

	if (!day && !time) return { title: original, due_on: null, due_at: null };

	if (!day) {
		const at = new Date(today.getFullYear(), today.getMonth(), today.getDate(), time!.hours, time!.minutes);
		day = at > now ? today : addDays(today, 1);
	}
	let due_at: string | null = null;
	if (time) {
		due_at = new Date(day.getFullYear(), day.getMonth(), day.getDate(), time.hours, time.minutes).toISOString();
	}
	return { title: text.trim(), due_on: isoDay(day), due_at };
}

/**
 * A date phrase on its own ("fri", "tomorrow 5pm"), as the date picker
 * reads it. `null` when the whole phrase is not a date.
 */
export function readDue(input: string, now: Date = new Date()): { due_on: string; due_at: string | null } | null {
	const phrase = input.trim();
	if (!phrase) return null;
	// The parser keeps a title back, so a placeholder stands in for one.
	const parsed = parseDue(`x ${phrase}`, now);
	if (parsed.title !== 'x' || !parsed.due_on) return null;
	return { due_on: parsed.due_on, due_at: parsed.due_at };
}

/**
 * How a due date reads on a row: "Today", "Tomorrow", "Yesterday", "3 days
 * ago" inside the past week, a weekday inside the coming week, else a short
 * date, with the time when there is one.
 */
export function dueLabel(due_on: string, due_at: string | null, now: Date = new Date()): string {
	const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
	const day = fromIsoDay(due_on);
	const diff = Math.round((day.getTime() - today.getTime()) / 86_400_000);
	let label: string;
	if (diff === 0) label = 'Today';
	else if (diff === 1) label = 'Tomorrow';
	else if (diff === -1) label = 'Yesterday';
	else if (diff < -1 && diff > -7) label = `${-diff} days ago`;
	else if (diff > 1 && diff < 7) label = day.toLocaleDateString('en-US', { weekday: 'long' });
	else
		label = day.toLocaleDateString('en-US', {
			weekday: 'short',
			month: 'short',
			day: 'numeric',
			year: day.getFullYear() !== today.getFullYear() ? 'numeric' : undefined,
		});
	if (due_at) {
		label += ` ${new Date(due_at).toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit' })}`;
	}
	return label;
}

export type DueGroup = 'overdue' | 'today' | 'upcoming' | 'none';

/** Which group an open task sits in. A task due earlier today is still Today. */
export function dueGroup(due_on: string | null, now: Date = new Date()): DueGroup {
	if (!due_on) return 'none';
	const today = isoDay(now);
	if (due_on < today) return 'overdue';
	if (due_on === today) return 'today';
	return 'upcoming';
}
