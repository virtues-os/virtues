/**
 * The days a search query names, so search can go straight to a day the way
 * it goes straight to an address. "yesterday", "tuesday", "last friday",
 * "oct 5", "5 october" and "2026-10-05" each name one; a bare weekday names
 * two (the latest and the one before), since "tuesday" is often last week's.
 *
 * Only days that have happened are offered: a day page for tomorrow has
 * nothing on it. A month and day with no year means the latest one, so in
 * January "dec 24" is last month.
 */
import { getLocalDateSlug } from '$lib/utils/dateUtils';

export interface NamedDay {
	/** `YYYY-MM-DD`, local. */
	date: string;
	/** "Tuesday, 11 November". The year is added when it is not this year. */
	title: string;
	/** How the query named it, when that is not obvious from the title: "yesterday", "last Tuesday". */
	hint: string | null;
}

const WEEKDAYS = ['sunday', 'monday', 'tuesday', 'wednesday', 'thursday', 'friday', 'saturday'];
const MONTHS = ['january', 'february', 'march', 'april', 'may', 'june', 'july', 'august', 'september', 'october', 'november', 'december'];

/** "tue", "tues", "tuesday" → 2. Three letters at least, so "su" names nothing. */
function weekday(word: string): number | null {
	if (word.length < 3) return null;
	const i = WEEKDAYS.findIndex((d) => d.startsWith(word));
	return i < 0 ? null : i;
}

function month(word: string): number | null {
	if (word.length < 3) return null;
	const i = MONTHS.findIndex((m) => m.startsWith(word));
	return i < 0 ? null : i;
}

function startOfDay(d: Date): Date {
	return new Date(d.getFullYear(), d.getMonth(), d.getDate());
}

function addDays(d: Date, n: number): Date {
	return new Date(d.getFullYear(), d.getMonth(), d.getDate() + n);
}

/** Spelled out by hand: `toLocaleDateString` puts the comma in different places on different engines. */
export function dayTitle(d: Date, now: Date = new Date()): string {
	const cap = (s: string) => s[0].toUpperCase() + s.slice(1);
	const year = d.getFullYear() === now.getFullYear() ? '' : ` ${d.getFullYear()}`;
	return `${cap(WEEKDAYS[d.getDay()])}, ${d.getDate()} ${cap(MONTHS[d.getMonth()])}${year}`;
}

function named(d: Date, now: Date, hint: string | null): NamedDay {
	return { date: getLocalDateSlug(d), title: dayTitle(d, now), hint };
}

/** A real calendar day: `new Date(2026, 1, 30)` rolls into March, which is not what "feb 30" asked for. */
function calendarDay(y: number, m: number, d: number): Date | null {
	const date = new Date(y, m, d);
	return date.getMonth() === m && date.getDate() === d ? date : null;
}

/** The days `text` names, latest first. Empty when it names none. */
export function namedDays(text: string, now: Date = new Date()): NamedDay[] {
	const today = startOfDay(now);
	const q = text.trim().toLowerCase().replace(/[,.]/g, ' ').replace(/\s+/g, ' ');
	if (!q) return [];

	if (q === 'today') return [named(today, now, 'today')];
	if (q === 'yesterday') return [named(addDays(today, -1), now, 'yesterday')];

	// 2026-10-05
	const iso = q.match(/^(\d{4})-(\d{1,2})-(\d{1,2})$/);
	if (iso) {
		const d = calendarDay(+iso[1], +iso[2] - 1, +iso[3]);
		return d && d <= today ? [named(d, now, null)] : [];
	}

	// "tuesday", "last tuesday"
	const wd = q.match(/^(last )?([a-z]+)$/);
	if (wd) {
		const target = weekday(wd[2]);
		if (target !== null) {
			const back = (today.getDay() - target + 7) % 7;
			const name = WEEKDAYS[target][0].toUpperCase() + WEEKDAYS[target].slice(1);
			if (wd[1]) {
				// "last tuesday" is never today.
				const d = addDays(today, -(back || 7));
				return [named(d, now, `last ${name}`)];
			}
			const latest = addDays(today, -back);
			return [
				named(latest, now, back === 0 ? 'today' : back === 1 ? 'yesterday' : null),
				named(addDays(latest, -7), now, null),
			];
		}
	}

	// "oct 5", "october 5 2025", "5 oct", "5 october 2025"
	const md = q.match(/^([a-z]+) (\d{1,2})(?: (\d{4}))?$/) ?? null;
	const dm = q.match(/^(\d{1,2}) ([a-z]+)(?: (\d{4}))?$/) ?? null;
	const parts = md ? { m: md[1], d: md[2], y: md[3] } : dm ? { m: dm[2], d: dm[1], y: dm[3] } : null;
	if (parts) {
		const m = month(parts.m);
		if (m === null) return [];
		const day = +parts.d;
		if (parts.y) {
			const d = calendarDay(+parts.y, m, day);
			return d && d <= today ? [named(d, now, null)] : [];
		}
		let d = calendarDay(today.getFullYear(), m, day);
		if (d && d > today) d = calendarDay(today.getFullYear() - 1, m, day);
		return d ? [named(d, now, null)] : [];
	}

	return [];
}
