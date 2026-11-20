/**
 * The radar's geometry, as plain functions so it can be tested without a
 * page. The radar is drawn in a fixed box and scaled to fit its pane.
 *
 * Angle says which thread a thing belongs to: every active thread has a
 * slice, slices under the same thing sit together under one band, and one
 * wider wedge at the bottom holds what is in no thread. Distance from the
 * center says how soon it matters, one ring per step.
 */
import { fitLabels, markBox, type Box, type LabelSpot } from './labels';

/** The box is wider than tall: thread names need more room at the sides than above and below. */
export const BOX_W = 780;
export const BOX_H = 680;
export const CX = BOX_W / 2;
export const CY = BOX_H / 2;
/** Ring radii, nearest first. */
export const RING_R = [86, 150, 214, 278] as const;
/** Where the band over a group of threads runs, just outside the last ring. */
export const ARC_R = 288;
/** Where the thread names sit, outside the band. */
const RIM_R = 300;
/** Degrees given to what is in no thread, centered on the bottom. */
const LOOSE_WIDTH = 76;
export const LOOSE = 'none';

/** Half the width of each ring's name in px, nearest ring first. Marks keep clear of it. */
const NAME_HALF = [16, 20, 32, 19] as const;
/** The least distance between two marks on a ring, center to center, in px. */
const MARK_GAP = 22;
/** How far apart two rows of marks sit on one ring, in px. */
const ROW_GAP = 21;

export interface RadarThread {
	id: string;
	name: string;
	/** The thing it sits under, which groups slices; null when it sits under none. */
	group: string | null;
}

export interface Lane {
	center: number;
	width: number;
	/** The ring names run along this edge of the slice, so its marks keep clear of that edge. */
	scale?: 'start' | 'end';
}

export interface RimLabel {
	key: string;
	/** The thread it names, or `LOOSE`. */
	threadId: string;
	x: number;
	y: number;
	/** CSS translate parts that hang the label off its point, away from the circle. */
	tx: string;
	ty: string;
	align: 'flex-start' | 'center' | 'flex-end';
	/** The thing's name in small capitals, on the one label of its group that carries it. */
	caps: string;
	name: string;
}

export interface Layout {
	lanes: Record<string, Lane>;
	/** The lines between slices. `group` is false between two threads under the same thing. */
	bounds: { at: number; group: boolean }[];
	rim: RimLabel[];
	/** A band outside the last ring over the threads under one thing. */
	arcs: { key: string; from: number; to: number }[];
	/** Where the wedge for what is in no thread starts, and how wide it is. */
	loose: { from: number; width: number };
	/** The angle of the line the ring names sit on: the line between two slices nearest the top. */
	scaleAt: number;
}

/** A point at `deg` clockwise from the top, `r` from the center. */
export function polar(deg: number, r: number): { x: number; y: number } {
	const a = (deg * Math.PI) / 180;
	return { x: CX + r * Math.sin(a), y: CY - r * Math.cos(a) };
}

const norm = (deg: number) => ((deg % 360) + 360) % 360;
/** How far an angle is from the top, the short way round. */
const fromTop = (deg: number) => Math.min(norm(deg), 360 - norm(deg));

function rimLabel(key: string, threadId: string, deg: number, caps: string, name: string): RimLabel {
	const p = polar(deg, RIM_R);
	const sin = Math.sin((deg * Math.PI) / 180);
	const cos = Math.cos((deg * Math.PI) / 180);
	const middle = Math.abs(sin) < 0.26;
	return {
		key,
		threadId,
		x: Math.round(p.x),
		y: Math.round(p.y),
		tx: middle ? '-50%' : sin > 0 ? '0%' : '-100%',
		ty: middle ? (cos > 0 ? '-100%' : '0%') : '-50%',
		align: middle ? 'center' : sin > 0 ? 'flex-start' : 'flex-end',
		caps,
		name,
	};
}

/** Threads under the same thing, together, in the order the things first appear. */
export function grouped<T extends { group: string | null }>(threads: T[]): { name: string | null; threads: T[] }[] {
	const groups: { name: string | null; threads: T[] }[] = [];
	for (const t of threads) {
		const group = t.group ? groups.find((g) => g.name === t.group) : undefined;
		if (group) group.threads.push(t);
		else groups.push({ name: t.group, threads: [t] });
	}
	return groups;
}

export function layout(threads: RadarThread[]): Layout {
	const lanes: Record<string, Lane> = {};
	const rim: RimLabel[] = [];
	if (threads.length === 0) {
		// The wedge is the whole circle, less a gap at the top for the ring names.
		lanes[LOOSE] = { center: 180, width: 320 };
		return { lanes, bounds: [], rim, arcs: [], loose: { from: 0, width: 360 }, scaleAt: 0 };
	}
	const width = (360 - LOOSE_WIDTH) / threads.length;
	const looseFrom = 180 - LOOSE_WIDTH / 2;
	const bounds: Layout['bounds'] = [];
	const arcs: Layout['arcs'] = [];
	/** Each line between two slices, with the slice that ends at it and the one that starts at it. */
	const edges: { at: number; before: string; after: string }[] = [];

	// Slices run clockwise from the far edge of the loose wedge, round the top, and back to it.
	let a = 180 + LOOSE_WIDTH / 2;
	let last = LOOSE;
	for (const g of grouped(threads)) {
		const start = a;
		g.threads.forEach((t, i) => {
			bounds.push({ at: norm(a), group: i === 0 });
			edges.push({ at: a, before: last, after: t.id });
			lanes[t.id] = { center: norm(a + width / 2), width };
			a += width;
			last = t.id;
		});
		// The thing's name rides on the label nearest the middle of its group.
		const carrier = Math.floor((g.threads.length - 1) / 2);
		g.threads.forEach((t, i) => rim.push(rimLabel(`thread:${t.id}`, t.id, lanes[t.id].center, g.name && i === carrier ? g.name : '', t.name)));
		if (g.name && g.threads.length > 1) arcs.push({ key: g.name, from: start + 2, to: a - 2 });
	}
	bounds.push({ at: norm(a), group: true });
	edges.push({ at: a, before: last, after: LOOSE });
	lanes[LOOSE] = { center: 180, width: LOOSE_WIDTH };
	rim.push(rimLabel(`thread:${LOOSE}`, LOOSE, 180, '', 'Not in a thread'));

	// The ring names sit on the line nearest the top. The slices either side keep their marks clear of it.
	const scale = edges.reduce((best, e) => (fromTop(e.at) < fromTop(best.at) ? e : best));
	lanes[scale.before].scale = 'end';
	lanes[scale.after].scale = 'start';

	return { lanes, bounds, rim, arcs, loose: { from: looseFrom, width: LOOSE_WIDTH }, scaleAt: norm(scale.at) };
}

/** The band over a group of threads, as an SVG path. */
export function arcPath(from: number, to: number): string {
	const a = polar(from, ARC_R);
	const b = polar(to, ARC_R);
	return `M${a.x.toFixed(1)} ${a.y.toFixed(1)} A${ARC_R} ${ARC_R} 0 ${to - from > 180 ? 1 : 0} 1 ${b.x.toFixed(1)} ${b.y.toFixed(1)}`;
}

export interface Dot {
	key: string;
	/** A thread id or `LOOSE`. */
	lane: string;
	ring: number;
	/** The mark's size in px. */
	size: number;
}

export interface Spot {
	x: number;
	y: number;
	ang: number;
}

/**
 * Where each dot sits. Things that share a slice and a ring are spread along
 * the ring. When the slice is full they stack in rows, each row a half step
 * round from the last like bricks, so no two marks touch. In a slice with
 * the ring names along one edge, the marks keep clear of the name.
 */
export function slots(dots: Dot[], lay: Layout): Record<string, Spot> {
	const out: Record<string, Spot> = {};
	const cells = new Map<string, Dot[]>();
	for (const d of dots) {
		const lane = lay.lanes[d.lane] ? d.lane : LOOSE;
		const cell = `${lane}|${d.ring}`;
		cells.set(cell, [...(cells.get(cell) ?? []), { ...d, lane }]);
	}
	const degrees = (px: number, r: number) => ((px / r) * 180) / Math.PI;
	for (const cell of cells.values()) {
		const lane = lay.lanes[cell[0].lane];
		const ring = cell[0].ring;
		const r = RING_R[ring];
		const clear = lane.scale ? Math.min(degrees(NAME_HALF[ring] + 12, r), lane.width * 0.6) : 0;
		const center = lane.center + (lane.scale === 'start' ? clear / 2 : lane.scale === 'end' ? -clear / 2 : 0);
		const span = Math.max(0, lane.width - clear - 6);
		// One row if it fits. Otherwise rows that each leave a half step for the
		// stagger. The step is measured on the innermost row, where marks sit
		// closest, and more rows move that row further in, so it is settled in
		// a few passes.
		let rows = 1;
		let sep = 0;
		for (let pass = 0; pass < 4; pass++) {
			sep = Math.max(7, degrees(MARK_GAP, r - ((rows - 1) / 2) * ROW_GAP));
			const fits = (room: number) => Math.max(1, Math.floor(Math.max(0, room) / sep) + 1);
			const need = cell.length <= fits(span) ? 1 : Math.ceil(cell.length / fits(span - sep / 2));
			if (need <= rows) break;
			rows = need;
		}
		const cols = Math.ceil(cell.length / rows);
		cell.forEach((d, i) => {
			const row = i % rows;
			const col = Math.floor(i / rows);
			const stagger = rows > 1 ? ((row % 2) - 0.5) * (sep / 2) : 0;
			const ang = center + (col - (cols - 1) / 2) * sep + stagger;
			const p = polar(ang, r + (row - (rows - 1) / 2) * ROW_GAP);
			out[d.key] = { x: p.x, y: p.y, ang };
		});
	}
	return out;
}

export interface WantedLabel {
	key: string;
	text: string;
	x: number;
	y: number;
	size: number;
	/** The mark's angle on the dial. Its label goes on the outer side first. */
	ang: number;
	/** The thing you picked is always labeled, even when nothing is free. */
	pinned: boolean;
}

/** The room a thread's name takes on the rim, estimated from its length. */
function rimBox(l: RimLabel): Box {
	const w = Math.max(Math.min(168, l.name.length * 7.6), l.caps.length * 8.2);
	const h = (l.name ? 20 : 0) + (l.caps ? 16 : 0);
	const x1 = l.tx === '-50%' ? l.x - w / 2 : l.tx === '0%' ? l.x : l.x - w;
	const y1 = l.ty === '-50%' ? l.y - h / 2 : l.ty === '0%' ? l.y : l.y - h;
	return { x1, y1, x2: x1 + w, y2: y1 + h };
}

/**
 * Where each label on the radar goes (see `fitLabels`). A label never covers
 * another label, a mark, the center, the ring names or, given the layout, a
 * thread's name on the rim.
 */
export function placeLabels(
	wanted: WantedLabel[],
	dots: { x: number; y: number; size: number }[],
	lay?: Pick<Layout, 'scaleAt' | 'rim'>,
): Record<string, LabelSpot | null> {
	const taken: Box[] = [
		{ x1: CX - 38, y1: CY - 38, x2: CX + 38, y2: CY + 38 },
		...RING_R.map((r, i) => {
			const p = polar(lay?.scaleAt ?? 0, r);
			return { x1: p.x - NAME_HALF[i] - 2, y1: p.y - 9, x2: p.x + NAME_HALF[i] + 2, y2: p.y + 9 };
		}),
		...(lay?.rim ?? []).map(rimBox),
		...dots.map(markBox),
	];
	return fitLabels(
		wanted.map((l) => ({ ...l, right: Math.sin((l.ang * Math.PI) / 180) >= 0 })),
		taken,
		{ w: BOX_W, h: BOX_H },
	);
}
