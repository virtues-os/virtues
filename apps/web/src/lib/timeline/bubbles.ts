/**
 * bubbles.ts - the day's moments on the map (dayback/src/main.js:1017-1145):
 * one dot per conversation or walk, where the track says you were at its
 * midpoint (anchor.ts), its label in a free slot around it with a leader
 * line. Dots closer than 48 px on screen fold into a count chip; opening a
 * chip zooms toward its members and re-clusters there, and when they still
 * share one spot, one place card lists them in time order. The moment under
 * the playhead wears the place colour and a larger label.
 */
import type { LngLat, Map as MlMap, Marker, PaddingOptions } from "maplibre-gl";

/** Dots closer than this on screen fold into one count chip. */
export const CLUSTER_PX = 48;

export interface Pt {
	x: number;
	y: number;
}
export interface Box {
	x: number;
	y: number;
	w: number;
	h: number;
}
/** The clear map: not under the day bar, the rail or (later) the scrubber. */
export interface Area {
	l: number;
	t: number;
	r: number;
	b: number;
}

/** Greedy screen-space clustering (main.js:1054-1057): each point joins the
 *  first cluster whose running centre is within `px`, else starts one. */
export function clusterPoints(pts: Pt[], px = CLUSTER_PX): { x: number; y: number; items: number[] }[] {
	const out: { x: number; y: number; items: number[] }[] = [];
	pts.forEach((p, i) => {
		const hit = out.find((c) => Math.hypot(c.x - p.x, c.y - p.y) < px);
		if (hit) {
			const n = hit.items.length;
			hit.x = (hit.x * n + p.x) / (n + 1);
			hit.y = (hit.y * n + p.y) / (n + 1);
			hit.items.push(i);
		} else out.push({ x: p.x, y: p.y, items: [i] });
	});
	return out;
}

/** Each label's offset from its dot (main.js:1075-1082), placed in the order
 *  given: the first slot of two rings (above, below, right, left, the four
 *  diagonals; then a label further out) that stays inside the clear area and
 *  keeps an 8 px gap from every box already placed. None free: above. */
export function placeLabels(items: (Pt & { w: number; h: number })[], lim: Area, obstacles: Box[]): [number, number][] {
	const g = 14; // dot to label
	const gp = 8; // label to label
	const placed = [...obstacles];
	return items.map(({ x, y, w, h }) => {
		const cand: [number, number][] = [
			[0, -(g + h / 2)],
			[0, g + h / 2],
			[g + w / 2, 0],
			[-(g + w / 2), 0],
			[g + w / 2, -(g + h / 2)],
			[-(g + w / 2), -(g + h / 2)],
			[g + w / 2, g + h / 2],
			[-(g + w / 2), g + h / 2],
			[0, -(g + h / 2) - (h + g)],
			[0, g + h / 2 + (h + g)],
			[g + w / 2 + (w + g), 0],
			[-(g + w / 2) - (w + g), 0],
		];
		let slot = cand[0];
		for (const [ox, oy] of cand) {
			const bx = x + ox - w / 2;
			const by = y + oy - h / 2;
			if (bx < lim.l || by < lim.t || bx + w > lim.r || by + h > lim.b) continue;
			if (placed.some((q) => !(bx + w + gp <= q.x || bx >= q.x + q.w + gp || by + h + gp <= q.y || by >= q.y + q.h + gp))) continue;
			slot = [ox, oy];
			break;
		}
		placed.push({ x: x + slot[0] - w / 2, y: y + slot[1] - h / 2, w, h });
		return slot;
	});
}

/** A moment to show: a conversation or a walk, at its anchor. */
export interface MapMoment {
	s: number;
	e: number;
	kind: "conversation" | "walk";
	title: string;
	lng: number;
	lat: number;
	/** Track covered while it ran, km. */
	km: number;
	moving: boolean;
}

export interface BubbleHost {
	/** The clear map, in container pixels. */
	area: () => Area;
	/** Framing padding for a fit. */
	pad: () => PaddingOptions;
	/** "12:20 PM" on the day's clock. */
	time: (ms: number) => string;
	/** The name of the stay holding `ms`, or null in a drive or a gap. */
	place: (ms: number) => string | null;
	/** A bubble or a card row was picked: park the playhead at `s`. */
	onpick: (s: number) => void;
	/** Frame the whole day. */
	fitDay: () => void;
	/** Bring a spot onto the clear map; whether the camera moved. */
	showPoint: (ll: LngLat) => boolean;
}

interface Mark {
	m: MapMoment;
	marker: Marker;
	el: HTMLElement;
	lbl: HTMLElement;
	line: SVGLineElement;
}

/** The label: the title, its time span and, for a moving conversation, how
 *  far it went (a walk's title already says). */
function labelHtml(m: MapMoment, time: (ms: number) => string): HTMLElement {
	const lbl = document.createElement("span");
	lbl.className = "tl-bubble-label";
	const title = document.createElement("span");
	title.textContent = m.title;
	const when = document.createElement("i");
	when.textContent = `${time(m.s)}-${time(m.e)}`;
	lbl.append(title, when);
	if (m.moving && m.kind !== "walk") {
		const far = document.createElement("i");
		far.textContent = `On the move · ${m.km.toFixed(1)} km`;
		lbl.append(far);
	}
	return lbl;
}

export class Bubbles {
	private marks: Mark[] = [];
	private chips = new Map<string, { el: HTMLButtonElement; keys: number[]; marker: Marker }>();
	private card: { el: HTMLElement; key: string; marker: Marker } | null = null;
	/** The knot the user opened: its members, and the zoom it opened at. */
	private expanded: { keys: Set<number>; zoom: number } | null = null;
	private cur: number | null = null;
	private readonly onMove = () => this.layout();

	constructor(
		private ml: typeof import("maplibre-gl"),
		private map: MlMap,
		private host: BubbleHost,
	) {
		// Labels re-hug their slots as the map pans and zooms.
		map.on("move", this.onMove);
	}

	/** The day's moments, replacing the last day's. */
	build(moments: MapMoment[]): void {
		this.clear();
		for (const m of moments) {
			const el = document.createElement("div");
			el.className = "tl-bubble";
			const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
			svg.setAttribute("class", "tl-bubble-line");
			svg.setAttribute("viewBox", "0 0 400 400");
			const line = document.createElementNS("http://www.w3.org/2000/svg", "line");
			for (const a of ["x1", "y1", "x2", "y2"]) line.setAttribute(a, "200");
			svg.append(line);
			const lbl = labelHtml(m, this.host.time);
			el.append(svg, lbl);
			// A bubble is a target: it parks the playhead on its moment.
			el.addEventListener("click", (e) => {
				e.stopPropagation();
				this.host.onpick(m.s);
			});
			const marker = new this.ml.Marker({ element: el, anchor: "center" }).setLngLat([m.lng, m.lat]).addTo(this.map);
			this.marks.push({ m, marker, el, lbl, line });
		}
		this.layout();
	}

	/** Light the moment under the playhead; re-place labels only when it changes. */
	sync(playT: number): void {
		let cur: number | null = null;
		for (const k of this.marks) {
			const on = playT >= k.m.s && playT < k.m.e;
			k.el.classList.toggle("cur", on);
			if (on) cur = k.m.s;
		}
		if (cur !== this.cur) {
			this.cur = cur;
			this.layout();
		}
	}

	/** The moment under the playhead, if it moved: its span lights on the map. */
	movingCurrent(): MapMoment | null {
		const k = this.marks.find((x) => x.m.s === this.cur);
		return k && k.m.moving ? k.m : null;
	}

	/** Picked from the rail: show that moment's bubble (main.js:1087-1091). */
	reveal(s: number): void {
		for (const rec of this.chips.values()) {
			if (rec.keys.includes(s)) {
				this.expand(rec.keys);
				return;
			}
		}
		const k = this.marks.find((x) => x.m.s === s);
		if (!k) return;
		// An open place card for somewhere else gives up the stage.
		if (this.expanded && !this.expanded.keys.has(s)) this.expanded = null;
		if (!this.host.showPoint(k.marker.getLngLat())) this.layout();
	}

	/** A section picked from the rail, with the starts of its rows
	 *  (main.js:1097-1100). True when the bubbles took the map there. */
	revealSection(keys: number[]): boolean {
		if (keys.length && this.expanded && keys.every((k) => this.expanded!.keys.has(k))) return true;
		for (const rec of this.chips.values()) {
			if (keys.some((k) => rec.keys.includes(k))) {
				this.expand(rec.keys);
				return true;
			}
		}
		this.expanded = null;
		return false;
	}

	/** A drive was framed: any open card folds. */
	fold(): void {
		this.expanded = null;
	}

	clear(): void {
		for (const k of this.marks) k.marker.remove();
		this.marks = [];
		for (const rec of this.chips.values()) rec.marker.remove();
		this.chips.clear();
		this.card?.marker.remove();
		this.card = null;
		this.expanded = null;
		this.cur = null;
	}

	destroy(): void {
		this.clear();
		this.map.off("move", this.onMove);
	}

	/** Open a chip: zoom toward its members and re-cluster there; the place
	 *  card opens only if zooming could not separate them (main.js:1135-1145). */
	private expand(keys: number[]): void {
		const marks = this.marks.filter((k) => keys.includes(k.m.s));
		if (!marks.length) return;
		this.expanded = null;
		// The chip holding the whole day just goes home to the day's frame.
		if (marks.length === this.marks.length) {
			this.host.fitDay();
			return;
		}
		const b = new this.ml.LngLatBounds();
		for (const k of marks) b.extend(k.marker.getLngLat());
		const settle = () => {
			const ps = marks.map((k) => this.map.project(k.marker.getLngLat()));
			const cx = ps.reduce((a, q) => a + q.x, 0) / ps.length;
			const cy = ps.reduce((a, q) => a + q.y, 0) / ps.length;
			if (ps.every((q) => Math.hypot(q.x - cx, q.y - cy) < CLUSTER_PX)) {
				this.expanded = { keys: new Set(keys), zoom: this.map.getZoom() };
			}
			this.layout();
		};
		this.map.once("moveend", settle);
		this.map.fitBounds(b, { padding: this.host.pad(), maxZoom: 16, duration: 650 });
		// Nothing to move: settle now.
		setTimeout(() => {
			if (!this.map.isMoving()) {
				this.map.off("moveend", settle);
				settle();
			}
		}, 40);
	}

	/** Declutter in three passes (main.js:1045-1086): tight knots fold into
	 *  count chips, every remaining label takes a free slot, then the leader
	 *  lines are drawn. */
	private layout(): void {
		if (!this.marks.length) return;
		const map = this.map;
		const lim = this.host.area();
		// Zoomed back out past where the knot was opened: it folds into a chip again.
		if (this.expanded && map.getZoom() < this.expanded.zoom - 0.75) this.expanded = null;
		type It = { k: Mark; p: Pt; cur: boolean; knot: boolean };
		const all: It[] = this.marks.map((k) => ({ k, p: map.project(k.marker.getLngLat()), cur: k.m.s === this.cur, knot: false }));

		// An opened knot's members never re-cluster; if they are still one
		// spot, ONE card lists them.
		const open = this.expanded?.keys ?? null;
		let card: { items: It[]; x: number; y: number } | null = null;
		if (open) {
			const mem = all.filter((it) => open.has(it.k.m.s));
			if (mem.length >= 2) {
				const x = mem.reduce((a, it) => a + it.p.x, 0) / mem.length;
				const y = mem.reduce((a, it) => a + it.p.y, 0) / mem.length;
				if (mem.every((it) => Math.hypot(it.p.x - x, it.p.y - y) < CLUSTER_PX)) card = { items: mem, x, y };
			}
		}
		const inCard = new Set(card?.items ?? []);

		// Pass 1: knots into chips, reconciled by membership so panning doesn't churn them.
		const free = all.filter((it) => !(open && open.has(it.k.m.s)));
		const knots = clusterPoints(free.map((it) => it.p))
			.filter((c) => c.items.length >= 2)
			.map((c) => ({ x: c.x, y: c.y, items: c.items.map((i) => free[i]) }));
		const seen = new Set<string>();
		for (const c of knots) {
			for (const it of c.items) it.knot = true;
			const keys = c.items.map((it) => it.k.m.s).sort((a, b) => a - b);
			const key = keys.join("|");
			seen.add(key);
			const ll = map.unproject([c.x, c.y]);
			let rec = this.chips.get(key);
			if (!rec) {
				const el = document.createElement("button");
				el.type = "button";
				el.className = "tl-knot";
				el.addEventListener("click", (e) => {
					e.stopPropagation();
					this.expand(keys);
				});
				rec = { el, keys, marker: new this.ml.Marker({ element: el, anchor: "center" }).setLngLat(ll).addTo(map) };
				this.chips.set(key, rec);
			} else rec.marker.setLngLat(ll);
			rec.el.textContent = String(c.items.length);
			rec.el.title = `${c.items.length} events here`;
			// The chip lights when the playhead's moment is inside it.
			rec.el.classList.toggle("cur", c.items.some((it) => it.cur));
		}
		for (const [key, rec] of this.chips) {
			if (!seen.has(key)) {
				rec.marker.remove();
				this.chips.delete(key);
			}
		}
		// Knot members hide behind their chip; card members keep their shared
		// dot but drop the label and line.
		for (const it of all) {
			it.k.el.style.display = it.knot ? "none" : "";
			it.k.el.classList.toggle("in-card", inCard.has(it));
		}
		const cardBox = this.syncCard(card, lim);

		// Pass 2: a free slot for every visible label, measured after showing;
		// the playhead's moment picks first, then in time order.
		const items = all.filter((it) => !it.knot && !inCard.has(it));
		items.sort((a, b) => (b.cur ? 1 : 0) - (a.cur ? 1 : 0) || a.k.m.s - b.k.m.s);
		const sized = items.map((it) => ({ ...it.p, w: it.k.lbl.offsetWidth || 160, h: it.k.lbl.offsetHeight || 34 }));
		const obstacles: Box[] = knots.map((c) => ({ x: c.x - 16, y: c.y - 16, w: 32, h: 32 }));
		if (cardBox) obstacles.push(cardBox);
		const slots = placeLabels(sized, lim, obstacles);

		// Pass 3: the label moves to its slot; the line runs from the dot's
		// edge to the label's centre (the opaque label hides its tail).
		items.forEach((it, i) => {
			const [ox, oy] = slots[i];
			const len = Math.hypot(ox, oy);
			it.k.lbl.style.transform = `translate(-50%, -50%) translate(${Math.round(ox)}px, ${Math.round(oy)}px)`;
			const sx = len ? 200 + (7 * ox) / len : 200;
			const sy = len ? 200 + (7 * oy) / len : 200;
			it.k.line.setAttribute("x1", sx.toFixed(1));
			it.k.line.setAttribute("y1", sy.toFixed(1));
			it.k.line.setAttribute("x2", (200 + ox).toFixed(1));
			it.k.line.setAttribute("y2", (200 + oy).toFixed(1));
		});
	}

	/** ONE callout anchored to a shared spot, its moments as rows in time
	 *  order (main.js:1115-1133); its footprint, so labels keep clear of it. */
	private syncCard(card: { items: { k: Mark; cur: boolean }[]; x: number; y: number } | null, lim: Area): Box | null {
		if (!card) {
			this.card?.marker.remove();
			this.card = null;
			return null;
		}
		const ms = card.items.map((it) => it.k.m).sort((a, b) => a.s - b.s);
		const curS = card.items.find((it) => it.cur)?.k.m.s ?? null;
		const key = `${ms.map((m) => m.s).join("|")}#${curS ?? ""}`;
		const ll = this.map.unproject([card.x, card.y]);
		if (!this.card) {
			const el = document.createElement("div");
			el.className = "tl-card";
			el.addEventListener("click", (e) => {
				e.stopPropagation();
				const row = (e.target as HTMLElement).closest<HTMLElement>("[data-s]");
				if (row) this.host.onpick(Number(row.dataset.s));
				else if ((e.target as HTMLElement).closest(".tl-card-x")) {
					this.expanded = null;
					this.layout();
				}
			});
			// Scroll the rows, not the map.
			el.addEventListener("wheel", (e) => e.stopPropagation());
			this.card = { el, key: "", marker: new this.ml.Marker({ element: el, anchor: "bottom", offset: [0, -12] }).setLngLat(ll).addTo(this.map) };
		} else this.card.marker.setLngLat(ll);

		// Rebuilt only when its members or the playhead's row change.
		if (this.card.key !== key) {
			this.card.key = key;
			const place = this.host.place(ms[0].s);
			const span = `${this.host.time(ms[0].s)}-${this.host.time(ms[ms.length - 1].e)}`;
			const el = this.card.el;
			el.replaceChildren();
			const hd = document.createElement("div");
			hd.className = "tl-card-hd";
			const title = document.createElement("b");
			title.textContent = place ?? `${ms.length} events here`;
			const meta = document.createElement("span");
			meta.textContent = place ? `${ms.length} events · ${span}` : span;
			const x = document.createElement("button");
			x.type = "button";
			x.className = "tl-card-x";
			x.title = "Close";
			x.textContent = "×";
			hd.append(title, meta, x);
			const rows = document.createElement("div");
			rows.className = "tl-card-rows";
			for (const m of ms) {
				const r = document.createElement("button");
				r.type = "button";
				r.className = "tl-card-row";
				r.classList.toggle("cur", m.s === curS);
				r.dataset.s = String(m.s);
				const t = document.createElement("i");
				t.textContent = this.host.time(m.s);
				const s = document.createElement("span");
				s.textContent = m.title;
				r.append(t, s);
				rows.append(r);
			}
			const tail = document.createElement("div");
			tail.className = "tl-card-tail";
			el.append(hd, rows, tail);
			// Sized ONCE, when it opens: the list grows to its content, capped by
			// the clear map above the dot (header about 46 px). Never on a pan.
			const ch = this.map.getContainer().clientHeight;
			const cap = Math.min(ch * 0.62, ch - 150);
			const room = card.y - 12 - lim.t - 46;
			rows.style.maxHeight = `${Math.max(110, Math.min(cap, room))}px`;
		}
		const w = this.card.el.offsetWidth || 260;
		const h = this.card.el.offsetHeight || 120;
		return { x: card.x - w / 2, y: card.y - 12 - h, w, h };
	}
}
