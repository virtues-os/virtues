/**
 * Placing names beside marks, for the drawings that have many of each: the
 * radar and the map. Plain functions, so they can be tested without a page.
 */

export interface Box {
	x1: number;
	y1: number;
	x2: number;
	y2: number;
}

export interface LabelSpot {
	/** The label's top left corner, by its estimated width. */
	x: number;
	y: number;
	/** Which side of its mark it sits on. */
	right: boolean;
	/**
	 * The x of the label's edge nearest its mark. A label's real width is not
	 * known until it is drawn, so draw it from this edge: starting here on the
	 * right of a mark, ending here on the left. The gap to the mark is then
	 * exact either way.
	 */
	anchor: number;
	/** The line back to its mark, when the label had to sit away from it. */
	lead: { x1: number; y1: number; x2: number; y2: number } | null;
}

export interface Wanted {
	key: string;
	text: string;
	/** The mark it names: its center, and its size across. */
	x: number;
	y: number;
	size: number;
	/** The side of the mark to try first. */
	right: boolean;
	/** Always placed, even when nothing is free. */
	pinned: boolean;
}

const LABEL_H = 20;
/**
 * A label's width, estimated from its length at the label size. The estimate
 * is on the wide side, as wide as a name made of capitals and digits, so a
 * label drawn narrower than its box leaves air and one drawn at its widest
 * still fits.
 */
export const labelWidth = (text: string) => Math.round(text.length * 7 + 8);
const overlap = (a: Box, b: Box) => a.x1 < b.x2 && a.x2 > b.x1 && a.y1 < b.y2 && a.y2 > b.y1;

/** The room a round mark takes, with a little air around it. */
export const markBox = (m: { x: number; y: number; size: number }): Box => ({
	x1: m.x - m.size / 2 - 2,
	y1: m.y - m.size / 2 - 2,
	x2: m.x + m.size / 2 + 2,
	y2: m.y + m.size / 2 + 2,
});

/**
 * Where each label goes. A label sits beside its mark, on whichever side is
 * free, a little above or below if it has to. Further away than that it gets
 * a line back to its mark. One that fits nowhere is left out, unless it is
 * pinned. Pinned labels are placed first. A label never covers another
 * label, anything in `taken`, or the edge of the drawing.
 */
export function fitLabels(wanted: Wanted[], taken: Box[], bounds: { w: number; h: number }): Record<string, LabelSpot | null> {
	const used = [...taken];
	const out: Record<string, LabelSpot | null> = {};
	const order = [...wanted].sort((a, b) => Number(b.pinned) - Number(a.pinned));
	for (const l of order) {
		const w = labelWidth(l.text);
		const tries = [0, -13, 13, -27, 27, -41, 41].flatMap((dy) =>
			[l.right, !l.right].map((right) => {
				const gap = Math.abs(dy) > 13 ? 14 : 5;
				const x1 = right ? l.x + l.size / 2 + gap : l.x - l.size / 2 - gap - w;
				return { x1, x2: x1 + w, y1: l.y - LABEL_H / 2 + dy, y2: l.y + LABEL_H / 2 + dy, right, far: Math.abs(dy) > 13 };
			}),
		);
		const fits = tries.find((t) => t.x1 >= 0 && t.x2 <= bounds.w && t.y1 >= 0 && t.y2 <= bounds.h && !used.some((q) => overlap(t, q)));
		const b = fits ?? (l.pinned ? tries[0] : null);
		if (!b) {
			out[l.key] = null;
			continue;
		}
		used.push(b);
		let lead: LabelSpot['lead'] = null;
		if (b.far) {
			const lx = b.right ? b.x1 - 2 : b.x2 + 2;
			const ly = (b.y1 + b.y2) / 2;
			const d = Math.hypot(lx - l.x, ly - l.y) || 1;
			const edge = l.size / 2 + 2;
			lead = { x1: l.x + ((lx - l.x) / d) * edge, y1: l.y + ((ly - l.y) / d) * edge, x2: lx, y2: ly };
		}
		out[l.key] = { x: Math.round(b.x1), y: Math.round(b.y1), right: b.right, anchor: Math.round(b.right ? b.x1 : b.x2), lead };
	}
	return out;
}
