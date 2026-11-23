/**
 * What your server asks you: something it cannot settle alone and you can
 * settle in one tap or one line. The Inbox lists these first.
 *
 * Today the only questions are about places your server cannot name. It
 * asks "Is this home?" and "Is this where you work?" for the places where
 * most of your time goes, and never assumes either. Any other place it asks
 * about by name only when one named place sits within 60 m of where you
 * stayed. Otherwise the question is open and you type the name.
 * `GET /api/timeline/places/review` lists them. An answer is written to the
 * place (`POST /api/timeline/places/:id/answer`), so a place is asked once.
 */
import { apiGet, apiSend } from '$lib/api/client';

/** A named place the map found near the place being asked about. */
export interface PlaceCandidate {
	name: string;
	meters: number;
	category: string;
}

export interface PlaceQuestion {
	/** `home` asks "Is this home?", `work` asks "Is this where you work?", `place` asks what the place was. */
	ask: 'home' | 'work' | 'place';
	/** For `home` and `work`: you set it somewhere else, and this place has led since. */
	changed: boolean;
	id: string;
	latitude: number;
	longitude: number;
	hours: number;
	/** You slept there. */
	overnight: boolean;
	naming: {
		/** Named places within 60 m, nearest first. */
		candidates: PlaceCandidate[];
		/** The nearest hotel within 250 m. */
		lodging: PlaceCandidate | null;
	};
	/** A match you already said was wrong. It is not offered again. */
	rejected_match: string | null;
	/** The place's name until it is answered, which is its city. */
	title: string;
	/** The day with the most time there, YYYY-MM-DD. */
	best_day: string | null;
	hours_a_week: number;
	nights_a_week: number;
	/** The last four weeks there, a day each, oldest first, ending today. An older server sends none. */
	days?: PlaceDay[];
}

/** One day at a place, in the place's own time. */
export interface PlaceDay {
	/** YYYY-MM-DD. */
	date: string;
	hours: number;
	/** The night before this morning was spent there. */
	slept: boolean;
}

export type PlaceAnswer =
	| { kind: 'name'; name: string; category?: string | null }
	| { kind: 'passing' }
	| { kind: 'not_this'; name: string }
	| { kind: 'home' | 'not_home' | 'work' | 'not_work' }
	/** "Not now": your server asks again in a week. */
	| { kind: 'later' };

/** One answer you can give with a tap. */
export interface Reply {
	label: string;
	answer: PlaceAnswer;
}

export interface Question {
	key: string;
	/** The question, as a sentence. */
	text: string;
	/** Where or what it is about, in a few words. */
	about: string;
	/** A fact that helps you answer it. */
	context: string;
	replies: Reply[];
	/** Set when the answer can be a name you type. */
	typed: { label: string } | null;
	place: PlaceQuestion;
}

/** A named place counts as a guess only from this close. Further out it was usually a neighbor. */
const GUESS_M = 60;
/** The most named places offered as replies to one question. */
const MAX_PICKS = 3;

const about = (n: number, unit: string) => {
	const r = Math.round(n);
	return n < 1 ? `Less than ${unit === 'hour' ? 'an' : 'a'} ${unit}` : `About ${r} ${unit}${r === 1 ? '' : 's'}`;
};

/** The question your server asks about a place, with the answers one tap gives. */
export function placeQuestion(q: PlaceQuestion): Question {
	const base = { key: `place:${q.id}`, about: q.title, place: q };
	const named = (c: PlaceCandidate): PlaceAnswer => ({ kind: 'name', name: c.name, category: c.category });
	const yesNo = (yes: PlaceAnswer, no: PlaceAnswer): Reply[] => [
		{ label: 'Yes', answer: yes },
		{ label: 'No', answer: no },
	];
	if (q.ask === 'home') {
		return {
			...base,
			text: q.changed ? 'Is this your home now?' : 'Is this home?',
			context: `${about(q.nights_a_week, 'night')} a week there`,
			replies: yesNo({ kind: 'home' }, { kind: 'not_home' }),
			typed: null,
		};
	}
	if (q.ask === 'work') {
		return {
			...base,
			text: q.changed ? 'Is this where you work now?' : 'Is this where you work?',
			context: `${about(q.hours_a_week, 'hour')} a week there`,
			replies: yesNo({ kind: 'work' }, { kind: 'not_work' }),
			typed: null,
		};
	}
	const context = `${about(q.hours, 'hour')} there`;
	const ok = (c: PlaceCandidate) => c.name !== q.rejected_match;
	const guess = (c: PlaceCandidate): Question => ({
		...base,
		text: `Were you at ${c.name}?`,
		context,
		replies: yesNo(named(c), { kind: 'not_this', name: c.name }),
		typed: null,
	});
	if (q.overnight) {
		const hotel = q.naming.lodging && ok(q.naming.lodging) && q.naming.lodging.meters <= GUESS_M ? q.naming.lodging : null;
		if (hotel) return guess(hotel);
		return { ...base, text: 'Where did you stay that night?', context, replies: [], typed: { label: 'Where you stayed' } };
	}
	const passing: Reply = { label: 'Just passing through', answer: { kind: 'passing' } };
	const near = q.naming.candidates.filter(ok);
	if (near.length === 1) return guess(near[0]);
	if (near.length > 1) {
		return {
			...base,
			text: 'Which of these were you at?',
			context,
			replies: [...near.slice(0, MAX_PICKS).map((c) => ({ label: c.name, answer: named(c) })), passing],
			typed: { label: 'Somewhere else' },
		};
	}
	return { ...base, text: 'Where were you?', context, replies: [passing], typed: { label: 'The name of the place' } };
}

export async function fetchPlaceQuestions(): Promise<PlaceQuestion[]> {
	return (await apiGet<{ questions: PlaceQuestion[] }>('/timeline/places/review')).questions;
}

export function sendPlaceAnswer(id: string, answer: PlaceAnswer): Promise<unknown> {
	return apiSend('POST', `/timeline/places/${encodeURIComponent(id)}/answer`, answer);
}
