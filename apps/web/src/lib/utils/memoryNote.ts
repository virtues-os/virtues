/**
 * The assistant's memory notes, as the person reads them in Settings.
 *
 * A note's body is written by the model, which copies record ids from the
 * prompt's circumstances block (`Name (person_…)`). The ids mean nothing to a
 * person, so the page shows the note without them, and an edit starts from
 * that clean text so a note the person rewrites is saved without them too.
 */

/** The three lanes, in the order the page lists them. */
export const LANES = [
	{ key: 'facts', phrase: 'your world' },
	{ key: 'manner', phrase: 'how to talk to you' },
	{ key: 'practices', phrase: "what you're holding to" },
] as const;

export type LaneKey = (typeof LANES)[number]['key'];

/**
 * How many live notes each lane holds before the assistant's adds are
 * refused. Mirrors `lane_cap` in virtues-core/src/api/assistant_memories.rs;
 * change both together.
 */
export const LANE_CAP: Record<LaneKey, number> = {
	facts: 12,
	manner: 8,
	practices: 8,
};

/** One list across lanes: facts, then manner, then practices, each oldest first. */
export function orderNotes<T extends { id: number; lane: string; created_at: string }>(
	notes: readonly T[],
): T[] {
	const rank = (lane: string) => {
		const i = LANES.findIndex((l) => l.key === lane);
		return i === -1 ? LANES.length : i;
	};
	return [...notes].sort(
		(a, b) =>
			rank(a.lane) - rank(b.lane) ||
			a.created_at.localeCompare(b.created_at) ||
			a.id - b.id,
	);
}

/** A record id as `ids::generate_id` mints it: a lowercase kind, `_`, 16 hex. */
const ID = String.raw`[a-z][a-z0-9]*_[0-9a-f]{16}`;
/** A link to a record reads as its text: `[Nick](/person/person_…)` → `Nick`. */
const RECORD_LINK = new RegExp(String.raw`\[([^\]\n]+)\]\(\/[a-z]+\/${ID}\)`, 'g');
/** An id, bare or as a route, with an optional `id:` label in front. */
const RECORD_ID = new RegExp(String.raw`(?:\bids?:[ \t]*)?(?:\/[a-z]+\/)?\b${ID}\b`, 'g');
/** Stands where an id was, so the tidying below knows what it may touch. */
const MARK = '\u0000';

/**
 * A note without the record ids in it, and without the punctuation they
 * leave behind: `Nick (person_…) is` → `Nick is`, and
 * `Nick (person_…, +15125550100) is` → `Nick (+15125550100) is`.
 * A note with no ids or record links comes back unchanged. A note that is nothing but ids
 * comes back as written, since an empty row would say less.
 */
export function displayNote(body: string): string {
	const linked = body.replace(RECORD_LINK, '$1');
	const marked = linked.replace(RECORD_ID, MARK);
	if (!marked.includes(MARK)) return linked;

	const clean = marked
		// A bracket that held an id keeps whatever else it held, and goes
		// when nothing is left.
		.replace(/([ \t]*)([([])([^()[\]]*)([)\]])/g, (whole, space, open, inner, close) => {
			if (!inner.includes(MARK)) return whole;
			const rest = inner
				.split(/[,;|]/)
				.map((part: string) => part.replaceAll(MARK, '').trim())
				.filter(Boolean);
			return rest.length ? `${space}${open}${rest.join(', ')}${close}` : '';
		})
		// An id set off by commas as an aside: `Nick, person_…, is` → `Nick is`.
		.replace(/[ \t]*,[ \t]*\u0000[ \t]*,/g, '')
		// An id heading a list takes its comma with it: `Brother: person_…, Nick`.
		.replace(/(^|:[ \t]*)\u0000[ \t]*[,;][ \t]*/gm, '$1')
		.replace(/[ \t]*\u0000/g, '')
		// A comma left with nothing after it.
		.replace(/[,;]+(?=[ \t]*(?:[.!?)\]]|$))/gm, '')
		.replace(/^[ \t]*[,;:][ \t]*/gm, '')
		.replace(/[ \t]{2,}/g, ' ')
		.replace(/[ \t]+([,.;:!?)\]])/g, '$1')
		.trim();

	return clean || body;
}
