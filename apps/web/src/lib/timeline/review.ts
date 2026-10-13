/**
 * review.ts - "Help Virtues out": the places the box's map can't name, asked
 * one at a time, the place with the most time first (dayback/src/main.js:
 * 2113-2201), after "Is this home?" and "Is this where you work?", which are
 * asked, never assumed. A clear match is accepted without asking and listed
 * for review. Every answer is written to the place's `wiki_places`
 * row (`POST /api/timeline/places/:id/answer`), so a place is asked once.
 */
import { apiGet, apiSend } from "$lib/api/client";

/** A named place the map found near a place being asked about. */
export interface PlaceCandidate {
	name: string;
	meters: number;
	kind: string;
	category: string;
	lat: number;
	lon: number;
}

/** What the box's map says a place is (`maps::naming`). */
export interface PlaceNaming {
	/** `confident` is named without asking; `ambiguous` (several close, or a
	 *  dense spot) and `none` are asked about. */
	tier: "confident" | "ambiguous" | "none";
	name: string | null;
	category: string | null;
	/** Named places within 60 m, nearest first. */
	candidates: PlaceCandidate[];
	/** With none within 60 m: the nearest named place within 250 m. */
	hint: PlaceCandidate | null;
	/** The nearest hotel within 250 m. */
	lodging: PlaceCandidate | null;
	/** Named hotels within 1 km, nearest first: "Which hotel?". */
	hotels: PlaceCandidate[];
	/** Named places within 300 m, nearest first: the typed answer's
	 *  suggestions, labelled on the map while the place is asked about. */
	nearby: PlaceCandidate[];
	city: string | null;
	/** The box holds street detail here; without it nothing can match. */
	street_detail: boolean;
}

export interface PlaceQuestion {
	/** `home`: "Is this home?"; `work`: "Is this where you work?"; `place`:
	 *  what the place was. */
	ask: "home" | "work" | "place";
	/** For `home` and `work`: the person set it elsewhere, and this place
	 *  has led since. */
	changed: boolean;
	id: string;
	latitude: number;
	longitude: number;
	hours: number;
	stop_count: number;
	/** Slept at: the question is whose home it was. */
	overnight: boolean;
	naming: PlaceNaming;
	/** The map's match the person said was wrong. */
	rejected_match: string | null;
	/** The place's title until it's answered: its city. */
	title: string;
	/** When the person was there, in words. */
	recall: string;
	/** The day with the most time there, YYYY-MM-DD. */
	best_day: string | null;
	/** A typical week there, from the first stay to the last. */
	hours_a_week: number;
	nights_a_week: number;
}

export interface ConfirmedPlace {
	id: string;
	title: string;
	source: "home" | "work" | "named" | "map";
	latitude: number;
	longitude: number;
	hours: number;
	stop_count: number;
}

export interface PlaceReview {
	questions: PlaceQuestion[];
	confirmed: ConfirmedPlace[];
}

export type Answer =
	| { kind: "name"; name: string; category?: string | null }
	| { kind: "passing" }
	| { kind: "not_this"; name: string }
	| { kind: "home" | "not_home" | "work" | "not_work" }
	| { kind: "home_here" | "work_here"; lat: number; lon: number }
	| { kind: "reopen" };

export function fetchReview(): Promise<PlaceReview> {
	return apiGet<PlaceReview>("/timeline/places/review");
}

/** Saves an answer; `place_id` is the place a tap on the map set Home or
 *  Work on, when it wasn't the one asked about. */
export function sendAnswer(id: string, a: Answer): Promise<{ place_id: string | null }> {
	return apiSend<{ place_id: string | null }>("POST", `/timeline/places/${encodeURIComponent(id)}/answer`, a);
}

/** A kind of place, the open question's answer: saved as its label alone,
 *  or followed by the name ("Which hotel?", "Whose home?", its name). */
export interface Kind {
	label: string;
	category: string | null;
	/** Its icon in the list of kinds. */
	icon: string;
	/** What picking it asks next; `work` files the place under Work. */
	then: "hotel" | "person" | "name" | "work";
}

/** The question a place asks. */
export interface Ask {
	text: string;
	/** The question as one line under its stay in the day's list. */
	line: string;
	/** A guess's "Yes", answered from the list's line. */
	yes?: Answer;
	/** A guess's "No": the guess set aside for good, so the place is asked
	 *  about without it. */
	no?: Answer;
	/** A guess's Yes and No, or Home's and Work's. */
	chips: { label: string; answer: Answer }[];
	/** "Which of these were you at?": the places to pick from, more than one
	 *  allowed, saved together. */
	picks: PlaceCandidate[];
	/** The open question's kinds of place; empty for a guess. Other is a
	 *  typed name beside them, never a kind. */
	kinds: Kind[];
	/** "None of these": more named places around, beyond the close ones. */
	more: string[];
	/** "Just Passing Through" is an answer: never for a night. */
	passing: boolean;
	/** The named places the question is about, labelled on the map. */
	shown: PlaceCandidate[];
	/** How far out they lie; 0 with none. */
	reachM: number;
	/** Home or Work: "No" asks the person to point to the right place on the
	 *  map instead of answering. */
	role?: "home" | "work";
}

/** A guess is asked about by name only from this close: further out, the
 *  nearest named place was usually a neighbour (a car dealership beside a
 *  stay, a hotel beside the one the person slept at). */
const GUESS_M = 60;
const HOTEL: Kind = { label: "A hotel", category: "lodging", icon: "ri:hotel-bed-line", then: "hotel" };
const SOMEONES_HOME: Kind = { label: "Someone’s home", category: null, icon: "ri:home-4-line", then: "person" };
/** A day's kinds of place; the label is the name saved when the person
 *  skips naming it. Work files the place under the person's Work. */
const DAY_KINDS: Kind[] = [
	{ label: "Restaurant", category: "dining", icon: "ri:restaurant-line", then: "name" },
	{ label: "Shop", category: "shopping", icon: "ri:shopping-bag-line", then: "name" },
	{ label: "Park", category: "outdoors", icon: "ri:tree-line", then: "name" },
	{ label: "Work", category: "office", icon: "ri:briefcase-line", then: "work" },
	SOMEONES_HOME,
];
const NIGHT_KINDS: Kind[] = [HOTEL, SOMEONES_HOME];

/** A typed name for whose home it was: "<name>'s place" (main.js:2198). */
export function homeOf(who: string): string {
	return `${who.trim()}’s place`;
}

/** Several places picked as one answer: their names together, and their
 *  category when they share one. */
export function pickAnswer(picked: PlaceCandidate[]): Answer {
	const cats = new Set(picked.map((c) => c.category));
	return { kind: "name", name: picked.map((c) => c.name).join(", "), category: cats.size === 1 ? picked[0].category : null };
}

/** A typical week there, for Home and Work: "About 6 nights a week",
 *  "About 10 hours a week". */
export function weekly(q: PlaceQuestion): string {
	if (q.ask === "home") return q.nights_a_week < 1 ? "Less than a night a week" : `About ${Math.round(q.nights_a_week)} ${Math.round(q.nights_a_week) === 1 ? "night" : "nights"} a week`;
	return q.hours_a_week < 1 ? "Less than an hour a week" : `About ${Math.round(q.hours_a_week)} ${Math.round(q.hours_a_week) === 1 ? "hour" : "hours"} a week`;
}

/** The question for a place. Home and Work are asked for the places where
 *  the time went. Otherwise Virtues asks about a place by name only on
 *  strong evidence: one named place within 60 m ("Were you at X?" Yes,
 *  No), several ("Which of these were you at?"), and after a night only a
 *  hotel within 60 m. Anything weaker is an open question whose answers
 *  are kinds of place, always true, with the names around only as the
 *  typed answer's suggestions (the owner's call, 2026-10-09: a button
 *  nothing supports, like a car dealership 175 m off, is worse than none).
 *  A match the person said was wrong is never offered again. */
export function askFor(q: PlaceQuestion): Ask {
	const n = q.naming;
	const ok = (c: PlaceCandidate) => c.name !== q.rejected_match;
	const named = (name: string, category: string | null = null): Answer => ({ kind: "name", name, category });
	const base = { chips: [], picks: [], kinds: [], more: [], passing: true, shown: [], reachM: 0 };
	const yesNo = (text: string, role: "home" | "work", yes: Answer, no: Answer): Ask => ({
		...base,
		text,
		line: text,
		yes,
		role,
		chips: [
			{ label: "Yes", answer: yes },
			{ label: "No", answer: no },
		],
		passing: false,
	});
	if (q.ask === "home") return yesNo(q.changed ? "Is this your home now?" : "Is this home?", "home", { kind: "home" }, { kind: "not_home" });
	if (q.ask === "work")
		return yesNo(q.changed ? "Is this where you work now?" : "Is this where you work?", "work", { kind: "work" }, { kind: "not_work" });
	/** "Were you at X?": Yes names the place after X; No sets X aside. */
	const guess = (c: PlaceCandidate): Ask => ({
		...base,
		text: `Were you at ${c.name}?`,
		line: `Were you at ${c.name}?`,
		yes: named(c.name, c.category),
		no: { kind: "not_this", name: c.name },
		chips: [
			{ label: "Yes, that’s it", answer: named(c.name, c.category) },
			{ label: "No", answer: { kind: "not_this", name: c.name } },
		],
		kinds: q.overnight ? NIGHT_KINDS : DAY_KINDS,
		passing: !q.overnight,
		shown: [c],
		reachM: c.meters,
	});
	if (q.overnight) {
		const hotel = n.lodging && ok(n.lodging) && n.lodging.meters <= GUESS_M ? n.lodging : null;
		if (hotel) return guess(hotel);
		return {
			...base,
			text: "Where did you stay that night?",
			line: "Where did you stay that night?",
			kinds: NIGHT_KINDS,
			passing: false,
		};
	}
	const cands = n.candidates.filter(ok);
	if (cands.length > 1)
		return {
			...base,
			text: "Which of these were you at?",
			line: `Which of ${cands.length} places nearby?`,
			picks: cands,
			kinds: DAY_KINDS,
			more: n.nearby.map((c) => c.name).filter((x) => !cands.some((c) => c.name === x) && x !== q.rejected_match).slice(0, 8),
			shown: cands,
			reachM: Math.max(...cands.map((c) => c.meters)),
		};
	if (cands.length === 1) return guess(cands[0]);
	// The nearest named places are labelled on the map, so a place the base
	// map leaves unlabelled at this zoom (a school among its grounds) can be
	// found, though none is offered as a button.
	const around = n.nearby.filter(ok).slice(0, 5);
	return {
		...base,
		text: "Virtues couldn’t name this spot. What was it?",
		line: "What was this spot?",
		kinds: DAY_KINDS,
		shown: around,
		reachM: around.length ? Math.max(...around.map((c) => c.meters)) : 0,
	};
}

/** The map's zoom that shows places `reachM` out readably (main.js:2216-
 *  2220): a span of twice the farthest plus a margin, fitted to about 400 px,
 *  between 15.5 and 18.5; with nothing to tell apart, 16.3 or closer. */
export function focusZoom(lat: number, reachM: number, current: number): number {
	if (reachM <= 0) return Math.max(current, 16.3);
	const metresPerPx = (Math.max(2 * reachM, 90) * 1.9) / 400;
	const z = Math.log2((78271.5169 * Math.cos((lat * Math.PI) / 180)) / metresPerPx);
	return Math.min(Math.max(z, 15.5), 18.5);
}
