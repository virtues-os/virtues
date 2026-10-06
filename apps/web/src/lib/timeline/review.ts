/**
 * review.ts - "Help Virtues out": the places the box's map can't name, asked
 * one at a time, the place with the most time first (dayback/src/main.js:
 * 2113-2201). A clear match, Home and Work are accepted without asking and
 * listed for review. Every answer is written to the place's `wiki_places`
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
	/** Named places within 300 m: the typed answer's suggestions. */
	nearby: string[];
	city: string | null;
	/** The box holds street detail here; without it nothing can match. */
	street_detail: boolean;
}

export interface PlaceQuestion {
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
	/** A line from a conversation there. */
	snippet: string | null;
	/** The day with the most time there, YYYY-MM-DD. */
	best_day: string | null;
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
	| { kind: "reopen" };

export function fetchReview(): Promise<PlaceReview> {
	return apiGet<PlaceReview>("/timeline/places/review");
}

export function sendAnswer(id: string, a: Answer): Promise<void> {
	return apiSend<void>("POST", `/timeline/places/${encodeURIComponent(id)}/answer`, a);
}

/** The question a place asks. */
export interface Ask {
	text: string;
	/** One-tap answers. */
	chips: { label: string; answer: Answer }[];
	/** "Which of these were you at?": the places to pick from, more than one
	 *  allowed, saved together. */
	picks: PlaceCandidate[];
	/** What "Other" takes: the place's name, or whose home it was. */
	other: "place" | "person";
	/** The named places the question is about, labelled on the map. */
	shown: PlaceCandidate[];
	/** How far out they lie, so the map frames them readably; 0 with none. */
	reachM: number;
}

/** "Mine / family" names the place Family (main.js:2169). */
const FAMILY = "Family";
/** What a spot nobody could name might have been (main.js:2171): the label
 *  is the name saved, as the prototype saves it. */
const KINDS = ["park", "office", "shop", "outdoors"];

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

/** The question for a place (main.js:2156-2179): which of several close
 *  places; whether it was a named place just out of reach (or someone's
 *  home, after a night there); whose home (where the map's places would
 *  show a hotel, else where they stayed); else what it was. A match the
 *  person said was wrong is never offered again. */
export function askFor(q: PlaceQuestion): Ask {
	const n = q.naming;
	const cands = n.candidates.filter((c) => c.name !== q.rejected_match);
	const named = (name: string, category: string | null = null): Answer => ({ kind: "name", name, category });
	if (cands.length > 1)
		return {
			text: "Which of these were you at?",
			chips: [],
			picks: cands,
			other: "place",
			shown: cands,
			reachM: Math.max(...cands.map((c) => c.meters)),
		};
	const near = cands[0] ?? (n.hint && n.hint.name !== q.rejected_match ? n.hint : null);
	if (near) {
		const base = { picks: [], shown: [near], reachM: near.meters };
		return q.overnight
			? {
					...base,
					text: `Were you at ${near.name}, or someone’s home?`,
					chips: [{ label: near.name, answer: named(near.name, near.category) }],
					other: "person",
				}
			: {
					...base,
					text: `Were you at ${near.name}?`,
					chips: [{ label: "Yes, that’s it", answer: named(near.name, near.category) }],
					other: "place",
				};
	}
	// Without the map's places here, nothing named nearby means nothing seen,
	// not nothing there: a night may have been a hotel as well as a home.
	if (q.overnight && !n.street_detail)
		return {
			text: "Where did you stay that night?",
			chips: [{ label: "Mine / family", answer: named(FAMILY) }],
			picks: [],
			other: "place",
			shown: [],
			reachM: 0,
		};
	if (q.overnight)
		return {
			text: "Looks like you stayed at a home. Whose place?",
			chips: [{ label: "Mine / family", answer: named(FAMILY) }],
			picks: [],
			other: "person",
			shown: [],
			reachM: 0,
		};
	return {
		text: "Virtues couldn’t name this spot. What was it?",
		chips: KINDS.map((k) => ({ label: k, answer: named(k) })),
		picks: [],
		other: "place",
		shown: [],
		reachM: 0,
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
