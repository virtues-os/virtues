/**
 * A life as the lifeline plate draws it: one origin, one now, chapters as
 * spans on the wire. Two sources feed the same plate — the repo's fictional
 * example (the opening: what a partition of a life LOOKS like) and the
 * person's own chapters (the close: the partition they just made).
 */
import type { ChapterApi } from "$lib/wiki/api";

export const YR = 365.25 * 24 * 3600e3;

export interface LifeChapter {
	t0: number;
	/** null = still going */
	t1: number | null;
	/** null = a stretch the person declined to name (kind 'unknown') */
	label: string | null;
	/** The readout on hover, after the name. */
	ep: string;
}

export interface PlannedChapter {
	t0: number;
	t1: number;
	label: string;
}

export interface Life {
	/** Birth, when known: the α end of the wire and the age ruler's zero.
	 *  Unknown (the profile has no birth date) hides the age row; the
	 *  scale then starts at the first chapter. */
	birth: number | null;
	now: number;
	/** When the record begins — draws coverage strips inside the boxes,
	 *  dense after it, sparse before. Only the fictional life claims one:
	 *  for a real person the strips would be invented, so none are drawn. */
	box: number | null;
	chapters: LifeChapter[];
	planned: PlannedChapter | null;
	/** The caption when nothing is hovered. */
	caption: string;
	ariaLabel: string;
}

// ── the fictional life, verbatim from the prototype ──
// The same made-up one the opening's table lists (interview.ts). The
// repo's reserved fictional life; nothing here is a real person's.
const F_BIRTH = new Date(1997, 3, 2).getTime();
const F_NOW = new Date(2026, 7, 17).getTime();

export const FICTIONAL_LIFE: Life = {
	birth: F_BIRTH,
	now: F_NOW,
	box: new Date(2025, 1, 9).getTime(),
	chapters: [
		{ t0: F_BIRTH, t1: new Date(2003, 7, 20).getTime(), label: "Childhood on the coast", ep: "three towns before the first classroom" },
		{ t0: new Date(2003, 7, 20).getTime(), t1: new Date(2009, 5, 10).getTime(), label: "Grade school, inland", ep: "snow days and the lake" },
		{ t0: new Date(2009, 5, 10).getTime(), t1: new Date(2016, 7, 20).getTime(), label: "The band years", ep: "the garage after hours" },
		{ t0: new Date(2016, 7, 20).getTime(), t1: new Date(2021, 8, 1).getTime(), label: "College", ep: "everything new at once, then a year at a desk" },
		{ t0: new Date(2021, 8, 1).getTime(), t1: new Date(2023, 6, 1).getTime(), label: "The first shop", ep: "the first real build" },
		{ t0: new Date(2023, 6, 1).getTime(), t1: new Date(2025, 5, 1).getTime(), label: "The workshop", ep: "two years of hard problems" },
		{ t0: new Date(2025, 5, 1).getTime(), t1: null, label: "Out on my own", ep: "the shop with my name on it" },
	],
	planned: { t0: F_NOW + 10 * (YR / 12), t1: F_NOW + 4.2 * YR, label: "The shop, grown" },
	caption: "One life on one wire: every day falls inside exactly one chapter.",
	ariaLabel:
		"One fictional life drawn on one wire: seven chapters as spans from birth toward now, named above, aged below. The table that follows lists the same chapters.",
};

/** A `YYYY-MM-DD` from the API, read as a local calendar day. */
function dayOf(date: string): number {
	const [y, m, d] = date.slice(0, 10).split("-").map(Number);
	return new Date(y, m - 1, d).getTime();
}

/** The person's own life, from what the interview wrote: their chapters
 *  (wiki_chapters, in order) and the birth date on their profile, if any.
 *  Returns null when there are no chapters to draw. */
export function lifeFromRecord(chapters: ChapterApi[], birthDate: string | null | undefined): Life | null {
	if (chapters.length === 0) return null;
	const now = Date.now();
	const spans: LifeChapter[] = chapters.map((ch) => {
		const t0 = dayOf(ch.started_at);
		const t1 = ch.ended_at ? dayOf(ch.ended_at) : null;
		const y0 = new Date(t0).getFullYear();
		const y1 = t1 === null ? "now" : new Date(t1).getFullYear();
		return {
			t0,
			t1,
			label: ch.title,
			ep: ch.summary?.trim() || `${y0} – ${y1}`,
		};
	});
	const n = spans.length;
	const from = new Date(spans[0].t0).getFullYear();
	return {
		birth: birthDate ? dayOf(birthDate) : null,
		now,
		box: null,
		chapters: spans,
		planned: null,
		caption: `Your life on one wire: ${n} ${n === 1 ? "chapter" : "chapters"}, from ${from} to now.`,
		ariaLabel: `Your life drawn on one wire: ${n} chapters as spans from ${from} toward now, named above.`,
	};
}
