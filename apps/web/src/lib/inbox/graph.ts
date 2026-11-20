/**
 * The graph's nodes, links and layout, as plain functions so they can be
 * tested without a page.
 *
 * The graph is drawn in the radar's frame, with the same slices and the
 * same thread names on the rim, so a thread is in the same place in both
 * views. Inside the rim the radar draws time and the graph draws links: you
 * in the middle, a hub for each thread, each thing hanging off its thread,
 * and a line for everything that joins two of them. An insight about two
 * threads sits between their hubs with a line to each. What is in no thread
 * hangs off you, in the wedge at the bottom.
 */
import { MARK_SIZE, type InboxItem } from './items';
import { CX, CY, LOOSE, grouped, polar, type Layout, type RadarThread } from './radar';

/** How far from the center each kind of node sits. */
const R_THING = 60;
const R_BRIDGE = 106;
const R_HUB = 150;
/** Rows for a thread's things, beyond its hub and inside the rim. */
const THREAD_ROWS = [196, 220, 244, 268];
/** Rows for what is in no thread, which hangs off you. */
const LOOSE_ROWS = [92, 116, 140, 164, 188, 212];
const R_SUBJECT = 252;
/** The distance between two marks in a row, center to center. */
const STEP = 24;

export type NodeKind = 'you' | 'thing' | 'hub' | 'item' | 'subject';

export interface GraphNode {
	id: string;
	kind: NodeKind;
	x: number;
	y: number;
	label: string;
	/** Its size across, in px. */
	size: number;
	item?: InboxItem;
	/** For a hub, the thread it stands for. */
	threadId?: string;
}

export interface GraphLink {
	key: string;
	from: string;
	to: string;
	/** What the link means, in a word or two, shown when one of its ends is chosen. */
	rel: string;
	/**
	 * `tree`: what holds a thing (you, a thing, a thread). `loose`: from you
	 * to something in no thread. `cross`: a link that joins two branches.
	 */
	style: 'tree' | 'loose' | 'cross';
}

/** Something a task is linked to besides an active thread: a person, a place, a thread that is not active. */
export interface Subject {
	key: string;
	label: string;
}

/** A link from a thing to a thread's hub or to a subject, found by the caller. */
export interface ExtraLink {
	to: { thread: string } | { subject: Subject };
	rel: string;
}

export interface Graph {
	nodes: GraphNode[];
	links: GraphLink[];
}

export const hubId = (threadId: string) => `hub:${threadId}`;
const degrees = (px: number, r: number) => ((px / r) * 180) / Math.PI;

/** Marks in rows across a slice: the inner rows fill first, and each row is centered on the slice. */
function pack(count: number, center: number, width: number, radii: readonly number[]): { ang: number; r: number }[] {
	const out: { ang: number; r: number }[] = [];
	for (let row = 0; out.length < count; row++) {
		// Past the last listed row, rows carry on outward at the same spacing.
		const r = radii[Math.min(row, radii.length - 1)] + Math.max(0, row - (radii.length - 1)) * STEP;
		const sep = degrees(STEP, r);
		const n = Math.min(count - out.length, Math.max(1, Math.floor(Math.max(0, width - 6) / sep) + 1));
		for (let i = 0; i < n; i++) out.push({ ang: center + (i - (n - 1) / 2) * sep, r });
	}
	return out;
}

/** The angle midway between several, the short way round. */
function midAngle(angles: number[]): number {
	const rad = angles.map((a) => (a * Math.PI) / 180);
	const x = rad.reduce((sum, a) => sum + Math.sin(a), 0);
	const y = rad.reduce((sum, a) => sum + Math.cos(a), 0);
	// Opposite angles have no middle; a quarter turn from the first is as good as any.
	if (Math.hypot(x, y) < 1e-6) return angles[0] + 90;
	return (Math.atan2(x, y) * 180) / Math.PI;
}

/** The active threads an insight is about. */
function threadsOf(it: InboxItem, lay: Layout): string[] {
	const ids = (it.note?.subjects ?? []).map((s) => s.match(/^\/thread\/([^/?#]+)/)?.[1]).filter((id): id is string => !!id && id !== LOOSE && !!lay.lanes[id]);
	return [...new Set(ids)];
}

export function buildGraph(input: {
	items: InboxItem[];
	threads: RadarThread[];
	lay: Layout;
	/** A task's links beyond its own thread. */
	extra?: (it: InboxItem) => ExtraLink[];
}): Graph {
	const { items, threads, lay } = input;
	const nodes: GraphNode[] = [{ id: 'you', kind: 'you', x: CX, y: CY, label: 'You', size: 56 }];
	const links: GraphLink[] = [];
	const at = (id: string, kind: NodeKind, ang: number, r: number, label: string, size: number, more: Partial<GraphNode> = {}) => {
		const p = polar(ang, r);
		nodes.push({ id, kind, x: p.x, y: p.y, label, size, ...more });
	};
	const link = (from: string, to: string, rel: string, style: GraphLink['style']) => {
		const key = `${from}>${to}`;
		if (!links.some((l) => l.key === key)) links.push({ key, from, to, rel, style });
	};

	// You, then each thing and the threads under it. A thread under no thing hangs off you.
	for (const g of grouped(threads)) {
		const centers = g.threads.map((t) => lay.lanes[t.id].center);
		const root = g.name ? `thing:${g.name}` : 'you';
		if (g.name) {
			at(root, 'thing', midAngle(centers), R_THING, g.name, 10);
			link('you', root, '', 'tree');
		}
		for (const t of g.threads) {
			at(hubId(t.id), 'hub', lay.lanes[t.id].center, R_HUB, t.name, 14, { threadId: t.id });
			link(root, hubId(t.id), g.name ? 'under' : '', 'tree');
		}
	}

	// An insight about two threads or more sits between their hubs.
	const bridges = items.filter((it) => threadsOf(it, lay).length > 1);
	const bridging = new Set(bridges.map((it) => it.key));
	const placed: { x: number; y: number }[] = [];
	for (const it of bridges) {
		const about = threadsOf(it, lay);
		let ang = midAngle(about.map((id) => lay.lanes[id].center));
		// Two threads on opposite sides have a middle at the top and one at the
		// bottom. The bottom one is in the wedge, among what hangs off you, so
		// take the other.
		const wedge = lay.lanes[LOOSE];
		const fromWedge = Math.abs(((((ang - wedge.center) % 360) + 540) % 360) - 180);
		if (fromWedge < wedge.width / 2 + degrees(STEP, R_BRIDGE)) ang += 180;
		let p = polar(ang, R_BRIDGE);
		// Two that would land on each other step round the ring.
		for (let tries = 0; tries < 12 && placed.some((q) => Math.hypot(q.x - p.x, q.y - p.y) < STEP); tries++) {
			ang += degrees(STEP, R_BRIDGE);
			p = polar(ang, R_BRIDGE);
		}
		placed.push(p);
		nodes.push({ id: it.key, kind: 'item', x: p.x, y: p.y, label: it.title, size: MARK_SIZE[it.kind], item: it });
		for (const id of about) link(it.key, hubId(id), 'about', 'cross');
	}

	// Everything else hangs off its thread, or off you when it has none.
	const homes = new Map<string, InboxItem[]>();
	for (const it of items) {
		if (bridging.has(it.key)) continue;
		const home = it.threadId && lay.lanes[it.threadId] ? it.threadId : LOOSE;
		homes.set(home, [...(homes.get(home) ?? []), it]);
	}
	for (const [home, list] of homes) {
		const lane = lay.lanes[home];
		const spots = pack(list.length, lane.center, lane.width, home === LOOSE ? LOOSE_ROWS : THREAD_ROWS);
		list.forEach((it, i) => {
			at(it.key, 'item', spots[i].ang, spots[i].r, it.title, MARK_SIZE[it.kind], { item: it });
			if (home === LOOSE) link('you', it.key, '', 'loose');
			else link(hubId(home), it.key, 'in', 'tree');
		});
	}

	// What else a thing is linked to: the task an insight is about, another
	// thread's hub, or a subject that gets a node of its own in the wedge.
	const has = new Set(nodes.map((n) => n.id));
	const subjects: Subject[] = [];
	for (const it of items) {
		for (const subject of it.note?.subjects ?? []) {
			const task = subject.match(/^\/task\/([^/?#]+)/)?.[1];
			if (task && has.has(`task:${task}`)) link(it.key, `task:${task}`, 'about', 'cross');
		}
		for (const e of input.extra?.(it) ?? []) {
			if ('thread' in e.to) {
				if (has.has(hubId(e.to.thread)) && e.to.thread !== it.threadId) link(it.key, hubId(e.to.thread), e.rel, 'cross');
				continue;
			}
			const s = e.to.subject;
			if (!subjects.some((q) => q.key === s.key)) subjects.push(s);
			link(it.key, `subject:${s.key}`, e.rel, 'cross');
		}
	}
	const wedge = lay.lanes[LOOSE];
	const spread = Math.max(0, wedge.width - 16);
	subjects.forEach((s, i) => {
		const ang = wedge.center + (subjects.length > 1 ? (i / (subjects.length - 1) - 0.5) * spread : 0);
		at(`subject:${s.key}`, 'subject', ang, R_SUBJECT, s.label, 10);
	});

	return { nodes, links };
}
