/**
 * front.ts - what the wiki's front page reads, from endpoints that already
 * exist.
 *
 * The week's written days come from `/api/wiki/days`: only a day with an
 * article has a line, so the page never opens on an empty row. When each
 * person and place last came up comes from two chat-sky weeks
 * (`/api/sky/week`), whose moments link the people a calendar item names and
 * the place of each stay.
 */
import { getSkyWeek, type SkyDay } from '$lib/api/client';
import { listDays } from '$lib/wiki/api';
import { lede } from '$lib/wiki/lede';
import { getLocalDateSlug } from '$lib/utils/dateUtils';

export interface WeekDay {
	/** `YYYY-MM-DD`. */
	date: string;
	/** The day article's opening paragraph, as markdown (DayLede draws it). */
	lede: string;
}

export interface FrontData {
	/** The last week's written days, newest first. */
	week: WeekDay[];
	/** `YYYY-MM-DD` each person or place last came up, by its wiki route. */
	lastSeen: Map<string, string>;
}

const WEEK_DAYS = 7;
/** How far back "last came up" looks: the two sky weeks. */
const SEEN_DAYS = 2 * WEEK_DAYS;

function addDays(slug: string, n: number): string {
	const d = new Date(`${slug}T12:00:00`);
	d.setDate(d.getDate() + n);
	return getLocalDateSlug(d);
}

export async function loadFront(today = getLocalDateSlug()): Promise<FrontData> {
	const [thisWeek, lastWeek, days] = await Promise.all([
		getSkyWeek(today).catch(() => null),
		getSkyWeek(addDays(today, -WEEK_DAYS)).catch(() => null),
		listDays(addDays(today, -(WEEK_DAYS - 1)), today).catch(() => []),
	]);

	const week: WeekDay[] = days
		.map((d) => ({ date: d.date, lede: lede(d.article) }))
		.filter((d): d is WeekDay => !!d.lede)
		.sort((a, b) => b.date.localeCompare(a.date));

	const lastSeen = new Map<string, string>();
	const seen = (url: string, date: string) => {
		const at = lastSeen.get(url);
		if (!at || date > at) lastSeen.set(url, date);
	};
	const sky: SkyDay[] = [...(lastWeek?.days ?? []), ...(thisWeek?.days ?? [])];
	for (const day of sky) {
		if (day.date > today || day.date <= addDays(today, -SEEN_DAYS)) continue;
		for (const m of day.moments) {
			if (m.planned) continue;
			for (const l of m.links) if (l.kind === 'person' || l.kind === 'place') seen(l.url, day.date);
			if (m.kind === 'stay' && m.place_id) seen(`/place/${m.place_id}`, day.date);
		}
	}

	return { week, lastSeen };
}

/** "Today", "Yesterday", a weekday within the week, else "Nov 3". */
export function whenLabel(slug: string, today = getLocalDateSlug()): string {
	if (slug === today) return 'Today';
	if (slug === addDays(today, -1)) return 'Yesterday';
	const d = new Date(`${slug}T12:00:00`);
	if (slug > addDays(today, -WEEK_DAYS)) return d.toLocaleDateString('en-US', { weekday: 'long' });
	return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
}
