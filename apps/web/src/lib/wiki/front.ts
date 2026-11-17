/**
 * front.ts - what the wiki's front page draws, read from endpoints that
 * already exist.
 *
 * The week comes from two chat-sky weeks (`/api/sky/week`), which carry each
 * day's stays, calendar items and workouts at their hours, plus the people a
 * calendar item names. The written sentence for each day comes from
 * `/api/wiki/days`. Nothing here is counted for display: a count only orders
 * the Lately lists.
 */
import { getProfile, getSkyWeek, type SkyDay, type SkyMoment } from '$lib/api/client';
import { listDays } from '$lib/wiki/api';
import { lede } from '$lib/wiki/lede';
import { getLocalDateSlug } from '$lib/utils/dateUtils';

/** One day drawn on a midnight-to-midnight line, in hours. */
export interface DayStrip {
	/** Time away from home, as [start, end] hours. */
	stays: [number, number][];
	/** Calendar items and workouts, at the hour they started. */
	moments: number[];
	/** Where the day stops: the current hour today, 24 otherwise. */
	until: number;
}

export interface WeekDay {
	/** `YYYY-MM-DD`. */
	date: string;
	/** The day article's opening paragraph, when it has been written. */
	lede: string | null;
	/** The places you went, for a day with no article yet. */
	places: string[];
	strip: DayStrip | null;
}

export interface LatelyEntry {
	label: string;
	url: string;
	/** `YYYY-MM-DD` of the last day it came up. */
	last: string;
	/** Minutes there in each hour of the day, for places. */
	hours?: number[];
}

export interface FrontData {
	week: WeekDay[];
	people: LatelyEntry[];
	places: LatelyEntry[];
	/** Your home's place id, so a day's line leaves out time at home. */
	homeId: string | null;
}

const WEEK_DAYS = 7;
const LATELY_MAX = 4;
const UNNAMED = 'An unnamed place';

function addDays(slug: string, n: number): string {
	const d = new Date(`${slug}T12:00:00`);
	d.setDate(d.getDate() + n);
	return getLocalDateSlug(d);
}

function isAway(m: SkyMoment, homeId: string | null): boolean {
	return m.kind === 'stay' && !m.planned && !!m.place_id && m.place_id !== homeId;
}

export function stripOf(day: SkyDay, homeId: string | null): DayStrip {
	const until = day.now_hour ?? 24;
	const stays: [number, number][] = [];
	const moments: number[] = [];
	for (const m of day.moments) {
		if (m.planned) continue;
		if (isAway(m, homeId)) {
			const a = Math.max(0, m.start_hour);
			const b = Math.min(until, m.end_hour);
			if (b > a) stays.push([a, b]);
		} else if ((m.kind === 'event' || m.kind === 'workout') && m.start_hour >= 0 && m.start_hour < until) {
			moments.push(m.start_hour);
		}
	}
	return { stays, moments, until };
}

function placeLabel(m: SkyMoment): string {
	return m.links.find((l) => l.kind === 'place')?.label ?? m.label;
}

function placesOf(day: SkyDay, homeId: string | null): string[] {
	const out: string[] = [];
	for (const m of day.moments) {
		if (!isAway(m, homeId)) continue;
		const label = placeLabel(m);
		if (label !== UNNAMED && !out.includes(label)) out.push(label);
	}
	return out;
}

/** Minutes of [start, end) that fall in each hour of the day. */
function addHours(bins: number[], start: number, end: number) {
	for (let h = Math.max(0, Math.floor(start)); h < Math.min(24, Math.ceil(end)); h++) {
		const overlap = Math.min(end, h + 1) - Math.max(start, h);
		if (overlap > 0) bins[h] += overlap * 60;
	}
}

interface Tally extends LatelyEntry {
	n: number;
}

function ranked(map: Map<string, Tally>): LatelyEntry[] {
	return [...map.values()]
		.sort((a, b) => b.n - a.n || b.last.localeCompare(a.last))
		.slice(0, LATELY_MAX)
		.map(({ n: _n, ...entry }) => entry);
}

export async function loadFront(today = getLocalDateSlug()): Promise<FrontData> {
	const first = addDays(today, -(WEEK_DAYS - 1));
	const [thisWeek, lastWeek, days, profile] = await Promise.all([
		getSkyWeek(today).catch(() => null),
		getSkyWeek(addDays(today, -WEEK_DAYS)).catch(() => null),
		listDays(first, today).catch(() => []),
		getProfile().catch(() => null),
	]);
	const homeId = profile?.home_place_id ?? null;
	const selfUrl = profile?.self_person_id ? `/person/${profile.self_person_id}` : null;

	const sky = new Map<string, SkyDay>();
	for (const d of [...(lastWeek?.days ?? []), ...(thisWeek?.days ?? [])]) {
		if (d.date <= today) sky.set(d.date, d);
	}
	const articles = new Map(days.map((d) => [d.date, lede(d.article)]));

	const week: WeekDay[] = [];
	for (let i = 0; i < WEEK_DAYS; i++) {
		const date = addDays(today, -i);
		const day = sky.get(date);
		week.push({
			date,
			lede: articles.get(date) ?? null,
			places: day ? placesOf(day, homeId) : [],
			strip: day ? stripOf(day, homeId) : null,
		});
	}

	const people = new Map<string, Tally>();
	const places = new Map<string, Tally>();
	for (const day of sky.values()) {
		for (const m of day.moments) {
			if (m.planned) continue;
			for (const l of m.links) {
				if (l.kind !== 'person' || l.url === selfUrl) continue;
				const t = people.get(l.url) ?? { label: l.label, url: l.url, last: day.date, n: 0 };
				t.n += 1;
				if (day.date > t.last) t.last = day.date;
				people.set(l.url, t);
			}
			if (isAway(m, homeId) && m.place_id) {
				const label = placeLabel(m);
				if (label === UNNAMED) continue;
				const t = places.get(m.place_id) ?? {
					label,
					url: `/place/${m.place_id}`,
					last: day.date,
					n: 0,
					hours: new Array(24).fill(0),
				};
				t.n += 1;
				if (day.date > t.last) t.last = day.date;
				addHours(t.hours!, m.start_hour, m.end_hour);
				places.set(m.place_id, t);
			}
		}
	}

	return { week, people: ranked(people), places: ranked(places), homeId };
}

/**
 * The painting's light at an hour of the day: night lake, dawn blue, morning
 * sky, midday sand, afternoon warmth, evening ochre, dusk blue. A place's
 * swatch paints the hours you spend there in these, so color stands only for
 * the hour.
 */
const HOUR_LIGHT: [number, string][] = [
	[5, '#7F9AA8'],
	[8, '#9FB3BF'],
	[11, '#E6D7B5'],
	[14, '#EDD29F'],
	[17, '#DDBF8C'],
	[20, '#A8957A'],
	[22, '#9FB3BF'],
	[24, '#7F9AA8'],
];

export function hourLight(hour: number): string {
	return HOUR_LIGHT.find(([until]) => hour < until)?.[1] ?? HOUR_LIGHT[0][1];
}

/** "Today", "Yesterday", a weekday within the week, else "Nov 3". */
export function whenLabel(slug: string, today = getLocalDateSlug()): string {
	if (slug === today) return 'Today';
	if (slug === addDays(today, -1)) return 'Yesterday';
	const d = new Date(`${slug}T12:00:00`);
	if (slug > addDays(today, -WEEK_DAYS)) return d.toLocaleDateString('en-US', { weekday: 'long' });
	return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
}
