<script lang="ts">
	/**
	 * The lifeline — an instrument for reading a life against itself.
	 *
	 * **What only this can do.** Chat can already tell you your heart rates. What
	 * nothing else can do is show that the month your resting rate climbed is the
	 * month you stopped texting and slept badly. Coincidence ACROSS streams is
	 * the reason this exists.
	 *
	 * **The chart changes what it draws as you zoom.** This is the correction to
	 * the version that felt lifeless: density bars are a STATISTICAL grammar and
	 * they need N in the thousands to say anything. At a two-hour window holding
	 * nine messages there is no statistic — there is a list. So the chart runs a
	 * ladder:
	 *
	 *   far   — density: bars for totals, bands for rates
	 *   near  — MOMENTS: every record drawn at its own instant, hoverable
	 *   above both — the SPINE: the events Virtues has already named
	 *
	 * The hinge is the data, not the calendar. The chart asks for the records in
	 * the window with a cap; if they fit, it draws them, and if they do not it
	 * draws densities. A quiet year therefore renders as moments while a busy
	 * afternoon renders as density, which is the honest way round.
	 *
	 * **The spine is the point.** Segmentation already produces named events with
	 * summaries. Drawing counts while those sat undrawn is what made a life look
	 * like a signal.
	 *
	 * **Hover is shared.** Pointing at a mark highlights its row in the panel and
	 * pointing at a row marks the chart. A chart that points at things you can
	 * read is the difference between a picture and an instrument.
	 *
	 * **The wheel does not steal the page's scroll.** Zoom needs intent: focus
	 * the chart, or hold ⌘/ctrl. Silently eating a scroll gesture on a page that
	 * scrolls is the rudest thing a canvas can do.
	 *
	 * **Direct manipulation is instant; indirect motion springs.** Dragging and
	 * wheel-zoom track the pointer 1:1 — easing there reads as lag. Buttons, keys
	 * and Reset animate, because you did not move it by hand and the motion is
	 * what tells you where you went. `prefers-reduced-motion` removes all of it.
	 */
	import { onMount } from 'svelte';
	import {
		getLifeline,
		getFeed,
		getProcessed,
		getClock,
		type Clock,
		type LifelineLane,
		type LifelineRecord,
		type Interpreted,
		type Stay
	} from '$lib/wiki/api';
	import LifelineMap from './LifelineMap.svelte';
	import LifelineFeed from './LifelineFeed.svelte';

	interface Props {
		minLaneHeight?: number;
		maxLaneHeight?: number;
	}
	let { minLaneHeight = 26, maxLaneHeight = 54 }: Props = $props();

	// ── constants ───────────────────────────────────────────────────────────
	const AXIS_H = 20;
	const SPINE_H = 30;
	const DORMANT_H = 24;
	const MIN_SPAN_MS = 60 * 60 * 1000;
	const CLICK_SLOP = 4;
	/** Pointer slack for grabbing a 1px mark or a selection edge. */
	const HIT = 12;
	/** More moments than this and the window wants a density, not a list. */
	const MOMENT_CAP = 500;
	/** Above this span, do not even ask — nothing that wide fits under the cap. */
	const MOMENT_MAX_DAYS = 120;

	// ── state ───────────────────────────────────────────────────────────────
	let canvas = $state<HTMLCanvasElement | null>(null);
	let mini = $state<HTMLCanvasElement | null>(null);
	let plotEl = $state<HTMLDivElement | null>(null);

	let lanes = $state<LifelineLane[]>([]);
	let loading = $state(true);
	let from = $state(0);
	let to = $state(1);
	let bounds = $state<{ from: number; to: number } | null>(null);
	let expanded = $state<string[]>([]);
	let measures = $state<Record<string, string>>({});
	let overview = $state<number[]>([]);

	/**
	 * The day-clock: time of day against date.
	 *
	 * The primary band. A density chart has one kind of mark, so it shows
	 * weather and never landmarks — you can see that something happened and
	 * never what, and nothing in it is findable by eye. This is the
	 * chronobiologist's actogram, and it is made of landmarks: waking life is
	 * the ink, sleep is the pale channel running through it, and a fortnight
	 * abroad dislocates the whole band by the time difference and puts it back.
	 */
	let clock = $state<Clock | null>(null);
	/** Painted once per response, then scaled — 28,800 rects per frame is not free. */
	let clockBitmap: HTMLCanvasElement | null = null;

	/** Named events in the window — the spine. */
	let events = $state<Interpreted[]>([]);
	/** Everything the interpreted layer covers, whatever the window shows. */
	let coverage = $state<[number, number] | null>(null);
	let daysProcessed = $state(0);

	/** Individual records, when few enough to draw one by one. */
	let moments = $state<LifelineRecord[]>([]);
	let momentMode = $state(false);

	let cursor = $state<{ x: number; y: number } | null>(null);
	let selection = $state<{ a: number; b: number } | null>(null);
	let drag = $state<{
		mode: 'select' | 'pan' | 'move-sel' | 'edge-a' | 'edge-b';
		x0: number;
		t0: number;
		a0: number;
		b0: number;
	} | null>(null);
	let spaceHeld = $state(false);
	let focused = $state(false);
	let menuFor = $state<string | null>(null);
	let hotLane = $state<number | null>(null);
	let pickedStay = $state<Stay | null>(null);
	let register = $state<'raw' | 'processed'>('raw');
	let focusLane = $state<string | null>(null);
	let showKeys = $state(false);

	/** Shared hover. `chartHover` points chart→panel; `panelHover` back. */
	let chartHover = $state<LifelineRecord | null>(null);
	let panelHover = $state<string | null>(null);

	let stageH = $state(0);
	let plotW = $state(900);

	const spanMs = $derived(to - from);
	const reduced =
		typeof window !== 'undefined' &&
		window.matchMedia?.('(prefers-reduced-motion: reduce)').matches;

	const panelFrom = $derived(selection ? Math.min(selection.a, selection.b) : from);
	const panelTo = $derived(selection ? Math.max(selection.a, selection.b) : to);

	const dormant = $derived(
		new Set(
			lanes
				.filter((l) => {
					const seen = l.first_seen ? new Date(l.first_seen).getTime() : -Infinity;
					return seen > to;
				})
				.map((l) => l.id)
		)
	);

	const spineH = $derived(events.length ? SPINE_H : 0);

	/**
	 * Half the stage, within reason.
	 *
	 * Twenty-four rows need height to read as a rhythm rather than a smear; the
	 * lanes, now that they are provenance rather than the point, do not.
	 */
	const clockH = $derived(
		clock ? Math.max(150, Math.min(280, Math.round(((stageH || 420) - AXIS_H - spineH) * 0.55))) : 0
	);

	/**
	 * How wide the interpreted layer is, in pixels, at this zoom.
	 *
	 * On a real record this is the difference between a feature and a rumour:
	 * 19 interpreted days inside an 8.6-year window is 0.6% of the plot, so the
	 * spine renders as nine pixels at the right-hand edge and the default view
	 * becomes the one place where none of it is visible. Below a legible width
	 * the spine stops drawing blocks nobody can see and draws a door instead.
	 */
	const coverageW = $derived(
		coverage ? Math.max(0, ((coverage[1] - coverage[0]) / spanMs) * plotW) : 0
	);
	const coverageTiny = $derived(!!coverage && coverageW < 40);

	const rowTops = $derived.by(() => {
		const live = lanes.filter((l) => !dormant.has(l.id)).length || 1;
		const spare = Math.max(
			0,
			(stageH || 360) - AXIS_H - spineH - clockH - dormant.size * DORMANT_H
		);
		const h = Math.min(maxLaneHeight, Math.max(minLaneHeight, Math.floor(spare / live)));
		let y = spineH + clockH;
		return lanes.map((l) => {
			const rh = dormant.has(l.id) ? DORMANT_H : h;
			const top = y;
			y += rh;
			return { top, h: rh };
		});
	});

	const plotH = $derived(
		spineH + clockH + rowTops.reduce((a, r) => a + r.h, 0) + AXIS_H
	);

	/** Where each hour row sits inside the clock band. */
	const hourH = $derived(clockH ? clockH / 24 : 0);

	/** Moments per lane, so a row does not scan the whole list. */
	const momentsByLane = $derived.by(() => {
		const m: Record<string, LifelineRecord[]> = {};
		for (const r of moments) (m[r.lane] ??= []).push(r);
		return m;
	});

	// ── data ────────────────────────────────────────────────────────────────
	const tOf = (x: number) => from + (x / plotW) * spanMs;
	const xOf = (t: number) => ((t - from) / spanMs) * plotW;

	let inflight = 0;
	async function load() {
		const seq = ++inflight;
		const buckets = Math.min(1200, Math.max(24, Math.round(plotW)));
		const data = await getLifeline(
			buckets,
			bounds ? new Date(from).toISOString() : undefined,
			bounds ? new Date(to).toISOString() : undefined,
			expanded,
			measures
		);
		if (seq !== inflight || !data) return;
		if (!bounds) {
			from = new Date(data.from).getTime();
			to = new Date(data.to).getTime();
			bounds = { from, to };
			// The first response IS the whole record, so the overview strip costs
			// no extra request — only the arithmetic to flatten it.
			const n = data.lanes[0]?.density.length ?? 0;
			const sum = new Array(n).fill(0);
			for (const l of data.lanes) {
				const p = l.peak || 1;
				for (let i = 0; i < n; i++) sum[i] += l.density[i] / p;
			}
			overview = sum;
		}
		lanes = data.lanes;
		loading = false;
		draw();
		void loadOverlays(seq);
	}

	/**
	 * The spine and the moments, after the lanes land.
	 *
	 * Deliberately not awaited by `load`: the lanes are what must repaint during
	 * a gesture, and making them wait on two more round trips would trade the
	 * responsiveness that already works for detail that can arrive a moment
	 * later.
	 */
	async function loadOverlays(seq: number) {
		const a = new Date(from).toISOString();
		const b = new Date(to).toISOString();

		// One zone for the whole raster — the browser's. See getClock.
		const tz = Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC';
		const ck = await getClock(a, b, Math.min(1400, Math.max(48, Math.round(plotW))), tz);
		if (seq !== inflight) return;
		if (ck) {
			clock = ck;
			paintClock(ck);
		}
		draw();

		const p = await getProcessed(a, b);
		if (seq !== inflight) return;
		if (p) {
			events = p.items;
			coverage = p.coverage
				? [new Date(p.coverage[0]).getTime(), new Date(p.coverage[1]).getTime()]
				: null;
			daysProcessed = p.days_processed;
		}

		// Ask for one more than we would draw: `has_more` is then the answer to
		// "does this window want a list or a histogram", straight from the data
		// rather than from a threshold in days that would be wrong for any life
		// quieter or busier than the one it was tuned on.
		if (spanMs / 86_400_000 <= MOMENT_MAX_DAYS) {
			const f = await getFeed(a, b, { limit: MOMENT_CAP });
			if (seq !== inflight) return;
			momentMode = !!f && !f.has_more && f.records.length > 0;
			moments = momentMode && f ? f.records : [];
		} else {
			momentMode = false;
			moments = [];
		}
		draw();
	}

	let refetch: ReturnType<typeof setTimeout> | undefined;
	function scheduleLoad() {
		clearTimeout(refetch);
		refetch = setTimeout(load, 140);
	}

	onMount(() => {
		void load();
		const ro = new ResizeObserver(([e]) => {
			plotW = Math.max(1, e.contentRect.width);
			draw();
			scheduleLoad();
		});
		if (plotEl) ro.observe(plotEl);
		return () => ro.disconnect();
	});

	// ── navigation ──────────────────────────────────────────────────────────
	function clampWindow(nf: number, nt: number) {
		if (bounds) {
			const span = nt - nf;
			if (nf < bounds.from) {
				nf = bounds.from;
				nt = nf + span;
			}
			if (nt > bounds.to) {
				nt = bounds.to;
				nf = Math.max(bounds.from, nt - span);
			}
		}
		from = nf;
		to = nt;
	}

	/**
	 * Indirect movement, animated.
	 *
	 * A button or a key did not move the window by hand, so the motion is the
	 * only thing that says where it went. Direct gestures never come through
	 * here — easing a drag reads as lag.
	 */
	let anim = 0;
	function glideTo(nf: number, nt: number, ms = 260) {
		cancelAnimationFrame(anim);
		if (reduced) {
			clampWindow(nf, nt);
			draw();
			void load();
			return;
		}
		const f0 = from;
		const t0 = to;
		const start = performance.now();
		const step = (now: number) => {
			const p = Math.min(1, (now - start) / ms);
			const e = 1 - Math.pow(1 - p, 3);
			clampWindow(f0 + (nf - f0) * e, t0 + (nt - t0) * e);
			draw();
			drawMini();
			if (p < 1) anim = requestAnimationFrame(step);
			else void load();
		};
		anim = requestAnimationFrame(step);
	}

	function zoomAt(anchorX: number, factor: number, glide = false) {
		const maxSpan = bounds ? bounds.to - bounds.from : spanMs;
		const span = Math.min(maxSpan, Math.max(MIN_SPAN_MS, spanMs * factor));
		const at = Math.min(1, Math.max(0, anchorX / plotW));
		const anchor = from + at * spanMs;
		const nf = anchor - at * span;
		if (glide) glideTo(nf, nf + span);
		else {
			clampWindow(nf, nf + span);
			draw();
			scheduleLoad();
		}
	}

	/** Centre the window on an instant, keeping (or setting) its span. */
	function goTo(t: number, span = spanMs) {
		glideTo(t - span / 2, t + span / 2);
	}

	function reset() {
		if (!bounds) return;
		selection = null;
		glideTo(bounds.from, bounds.to, 340);
	}

	function zoomToSelection() {
		if (!selection) return;
		const a = Math.min(selection.a, selection.b);
		const b = Math.max(selection.a, selection.b);
		if (b - a < MIN_SPAN_MS) return;
		selection = null;
		glideTo(a, b);
	}

	function onWheel(e: WheelEvent) {
		// Intent required. A canvas that eats the scroll of a page that scrolls
		// leaves people stranded halfway down a document with no way past it.
		const pinch = e.ctrlKey || e.metaKey;
		if (!pinch && !focused) return;
		e.preventDefault();
		const r = canvas!.getBoundingClientRect();
		if (e.shiftKey || Math.abs(e.deltaX) > Math.abs(e.deltaY)) {
			const dt = ((e.shiftKey ? e.deltaY : e.deltaX) / plotW) * spanMs;
			clampWindow(from + dt, to + dt);
			draw();
			scheduleLoad();
			return;
		}
		// Clamped per event: trackpad momentum emits deltas an order of magnitude
		// past a wheel notch, and unclamped one flick crossed eight years.
		const step = Math.max(-60, Math.min(60, e.deltaY));
		zoomAt(e.clientX - r.left, Math.exp(step * 0.0022));
	}

	// ── pointer ─────────────────────────────────────────────────────────────
	/** Which part of the selection, if any, is under x. */
	function selZone(x: number): 'edge-a' | 'edge-b' | 'move-sel' | null {
		if (!selection) return null;
		const a = xOf(Math.min(selection.a, selection.b));
		const b = xOf(Math.max(selection.a, selection.b));
		if (Math.abs(x - a) <= HIT / 2) return 'edge-a';
		if (Math.abs(x - b) <= HIT / 2) return 'edge-b';
		return x > a && x < b ? 'move-sel' : null;
	}

	function onDown(e: PointerEvent) {
		const r = canvas!.getBoundingClientRect();
		const x = e.clientX - r.left;
		canvas!.focus();
		(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
		menuFor = null;

		const a0 = selection ? Math.min(selection.a, selection.b) : 0;
		const b0 = selection ? Math.max(selection.a, selection.b) : 0;

		if (spaceHeld || e.button === 1) {
			drag = { mode: 'pan', x0: x, t0: tOf(x), a0, b0 };
			return;
		}
		// A selection is an object once it exists: its edges resize it and its
		// body moves it, exactly like the box on the overview strip. One
		// interaction learned twice over rather than two that merely look alike.
		const zone = selZone(x);
		if (zone) {
			drag = { mode: zone, x0: x, t0: tOf(x), a0, b0 };
			return;
		}
		drag = { mode: 'select', x0: x, t0: tOf(x), a0, b0 };
		selection = { a: tOf(x), b: tOf(x) };
	}

	function onMove(e: PointerEvent) {
		const r = canvas!.getBoundingClientRect();
		const x = e.clientX - r.left;
		const y = e.clientY - r.top;
		cursor = { x, y };

		const row = rowTops.findIndex((t) => y >= t.top && y < t.top + t.h);
		hotLane = row < 0 ? null : row;

		if (drag) {
			const dt = ((x - drag.x0) / plotW) * spanMs;
			if (drag.mode === 'select') selection = { a: drag.t0, b: tOf(x) };
			else if (drag.mode === 'edge-a') selection = { a: tOf(x), b: drag.b0 };
			else if (drag.mode === 'edge-b') selection = { a: drag.a0, b: tOf(x) };
			else if (drag.mode === 'move-sel') selection = { a: drag.a0 + dt, b: drag.b0 + dt };
			else if (drag.mode === 'pan') {
				const d = (e.movementX / plotW) * spanMs;
				clampWindow(from - d, to - d);
				scheduleLoad();
			}
		} else {
			chartHover = momentMode ? nearestMoment(x, hotLane) : null;
		}
		draw();
	}

	function onUp(e: PointerEvent) {
		const r = canvas!.getBoundingClientRect();
		// A click that wobbled is still a click: clear, rather than leave a
		// zero-width selection dimming the whole chart for no reason.
		if (drag?.mode === 'select' && Math.abs(e.clientX - r.left - drag.x0) < CLICK_SLOP) {
			selection = null;
		}
		drag = null;
		draw();
	}

	/** The record nearest the pointer, within a slop a 1px mark can be hit at. */
	function nearestMoment(x: number, row: number | null): LifelineRecord | null {
		if (row === null || !lanes[row]) return null;
		const list = momentsByLane[lanes[row].id];
		if (!list) return null;
		let best: LifelineRecord | null = null;
		let bestD = HIT;
		for (const r of list) {
			const d = Math.abs(xOf(new Date(r.at).getTime()) - x);
			if (d < bestD) {
				bestD = d;
				best = r;
			}
		}
		return best;
	}

	function onKey(e: KeyboardEvent) {
		if (e.key === ' ') {
			spaceHeld = true;
			e.preventDefault();
			return;
		}
		const step = e.shiftKey ? 0.5 : 0.15;
		const k = e.key;
		if (k === 'ArrowLeft') glideTo(from - spanMs * step, to - spanMs * step, 160);
		else if (k === 'ArrowRight') glideTo(from + spanMs * step, to + spanMs * step, 160);
		else if (k === '=' || k === '+') zoomAt(plotW / 2, 0.6, true);
		else if (k === '-' || k === '_') zoomAt(plotW / 2, 1 / 0.6, true);
		else if (k === '0') reset();
		else if (k === 'Escape') {
			selection = null;
			showKeys = false;
		} else if (k === 'Enter' && selection) zoomToSelection();
		else if (k === '?') showKeys = !showKeys;
		else return;
		e.preventDefault();
		draw();
	}

	// ── overview strip ──────────────────────────────────────────────────────
	const MINI_HANDLE = 7;
	let miniDrag = $state<
		{ mode: 'move' | 'left' | 'right'; x0: number; f0: number; t0: number } | null
	>(null);
	let miniHot = $state<'move' | 'left' | 'right' | null>(null);

	function miniGeom() {
		if (!bounds || !mini) return null;
		const W = mini.clientWidth;
		const span = bounds.to - bounds.from || 1;
		return {
			W,
			span,
			a: ((from - bounds.from) / span) * W,
			b: ((to - bounds.from) / span) * W
		};
	}

	function miniZone(x: number): 'move' | 'left' | 'right' | null {
		const g = miniGeom();
		if (!g) return null;
		if (Math.abs(x - g.a) <= MINI_HANDLE) return 'left';
		if (Math.abs(x - g.b) <= MINI_HANDLE) return 'right';
		return x > g.a && x < g.b ? 'move' : null;
	}

	function miniDown(e: PointerEvent) {
		const g = miniGeom();
		if (!g || !mini) return;
		const x = e.clientX - mini.getBoundingClientRect().left;
		mini.setPointerCapture(e.pointerId);
		let zone = miniZone(x);
		if (!zone) {
			goTo(bounds!.from + (x / g.W) * g.span);
			zone = 'move';
		}
		miniDrag = { mode: zone, x0: x, f0: from, t0: to };
	}

	function miniMove(e: PointerEvent) {
		const g = miniGeom();
		if (!g || !mini) return;
		const x = e.clientX - mini.getBoundingClientRect().left;
		if (!miniDrag) {
			miniHot = miniZone(x);
			return;
		}
		const at = bounds!.from + (x / g.W) * g.span;
		if (miniDrag.mode === 'move') {
			const dt = ((x - miniDrag.x0) / g.W) * g.span;
			clampWindow(miniDrag.f0 + dt, miniDrag.t0 + dt);
		} else if (miniDrag.mode === 'left') {
			from = Math.max(bounds!.from, Math.min(at, to - MIN_SPAN_MS));
		} else {
			to = Math.min(bounds!.to, Math.max(at, from + MIN_SPAN_MS));
		}
		draw();
		drawMini();
		scheduleLoad();
	}

	// ── lane controls ───────────────────────────────────────────────────────
	function chooseMeasure(laneId: string, id: string) {
		const root = laneId.split('/')[0];
		if (id === 'records') {
			const { [root]: _drop, ...rest } = measures;
			measures = rest;
		} else measures = { ...measures, [root]: id };
		menuFor = null;
		void load();
	}

	function toggleExpand(id: string) {
		const root = id.split('/')[0];
		expanded = expanded.includes(root) ? expanded.filter((x) => x !== root) : [...expanded, root];
		menuFor = null;
		void load();
	}

	// ── numbers ─────────────────────────────────────────────────────────────
	function compact(v: number): string {
		const a = Math.abs(v);
		if (a >= 1_000_000) return `${(v / 1e6).toFixed(1)}M`;
		if (a >= 10_000) return `${Math.round(v / 1000)}k`;
		if (a >= 1_000) return `${(v / 1000).toFixed(1)}k`;
		if (a > 0 && a < 1) return v.toFixed(1);
		return Math.round(v).toLocaleString();
	}

	function fmt(v: number, unit: string): string {
		if (!Number.isFinite(v)) return '—';
		if (unit === '$') return `$${compact(v)}`;
		if (unit === 'h') return `${v < 10 ? v.toFixed(1) : Math.round(v)}h`;
		if (unit === 'min') return `${compact(v)} min`;
		if (unit) return `${compact(v)} ${unit}`;
		return compact(v);
	}

	/**
	 * A date always carries its year.
	 *
	 * "Dec 3, 5 PM" is not a position in a nine-year record — it is a position in
	 * whichever of nine Decembers the reader happened to assume.
	 */
	function fmtDate(d: Date): string {
		const days = spanMs / 86_400_000;
		if (days < 3)
			return d.toLocaleString('en-US', {
				month: 'short',
				day: 'numeric',
				year: 'numeric',
				hour: 'numeric'
			});
		if (days < 400)
			return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' });
		return d.toLocaleDateString('en-US', { month: 'short', year: 'numeric' });
	}

	const shortDate = (t: number) =>
		new Date(t).toLocaleDateString('en-US', { month: 'short', year: 'numeric' });

	function fmtSpan(ms: number): string {
		const h = ms / 3_600_000;
		if (h < 48) return `${Math.round(h)} hours`;
		const d = h / 24;
		if (d < 62) return `${Math.round(d)} days`;
		const mo = d / 30.44;
		if (mo < 24) return `${Math.round(mo)} months`;
		return `${(d / 365.25).toFixed(1)} years`;
	}

	const shortName = (id: string) => (id.includes('/') ? id.split('/')[1] : id);

	const grain = $derived.by(() => {
		const h = spanMs / Math.max(1, lanes[0]?.density.length ?? 1) / 3_600_000;
		if (h < 1.5) return 'hour';
		if (h < 36) return 'day';
		if (h < 24 * 10) return 'week';
		if (h < 24 * 45) return 'month';
		return 'quarter';
	});

	const bucketAt = (x: number) => {
		const n = lanes[0]?.density.length ?? 0;
		if (!n) return -1;
		return Math.min(n - 1, Math.max(0, Math.floor((x / plotW) * n)));
	};

	const cursorTime = $derived(cursor ? new Date(tOf(cursor.x)) : null);
	const tipLeft = $derived(cursor ? Math.min(cursor.x + 16, Math.max(0, plotW - 200)) : 0);

	/** The hour under the pointer, when it is inside the clock band. */
	const cursorHour = $derived.by(() => {
		if (!cursor || !clockH) return null;
		const h = Math.floor((cursor.y - spineH) / hourH);
		return h >= 0 && h < 24 ? h : null;
	});

	const hourLabel = (h: number) =>
		h === 0 ? '12a' : h === 12 ? '12p' : h < 12 ? `${h}a` : `${h - 12}p`;

	const readout = $derived.by(() => {
		if (!cursor || drag || lanes.length === 0 || momentMode) return null;
		const i = bucketAt(cursor.x);
		if (i < 0) return null;
		return lanes.map((l) => ({
			id: l.id,
			unit: l.unit,
			v: l.density[i] ?? 0,
			dormant: dormant.has(l.id)
		}));
	});

	const summary = $derived.by(() => {
		if (lanes.length === 0) return null;
		const n = lanes[0].density.length;
		let i0 = 0;
		let i1 = n - 1;
		if (selection) {
			const a = Math.min(selection.a, selection.b);
			const b = Math.max(selection.a, selection.b);
			i0 = Math.max(0, Math.floor(((a - from) / spanMs) * n));
			i1 = Math.min(n - 1, Math.ceil(((b - from) / spanMs) * n) - 1);
			if (i1 < i0) i1 = i0;
		}
		return lanes.map((l) => {
			let sum = 0;
			let seen = 0;
			for (let i = i0; i <= i1; i++) {
				const v = l.density[i] ?? 0;
				sum += v;
				if (v > 0) seen++;
			}
			// A rate averages over the buckets that HAVE a value; averaging in the
			// empty ones reports a heart rate of 30 for a day off the wrist.
			return {
				id: l.id,
				label: l.measure_label,
				unit: l.unit,
				value: l.kind === 'rate' ? (seen ? sum / seen : NaN) : sum,
				dormant: dormant.has(l.id)
			};
		});
	});

	/**
	 * Somewhere worth going, when here has nothing.
	 *
	 * Every empty state in a time-navigable interface is a chance to move someone
	 * somewhere non-empty. A dead end that knows where the data is and does not
	 * say so is just withholding.
	 */
	const nowhereHere = $derived(
		!loading &&
			lanes.length > 0 &&
			events.length === 0 &&
			moments.length === 0 &&
			lanes.every((l) => l.peak === 0)
	);

	const nearestData = $derived.by(() => {
		const mid = from + spanMs / 2;
		const seeds = lanes
			.map((l) => (l.first_seen ? new Date(l.first_seen).getTime() : null))
			.filter((t): t is number => t !== null);
		if (!seeds.length) return null;
		return seeds.reduce((best, t) => (Math.abs(t - mid) < Math.abs(best - mid) ? t : best));
	});

	function ticks(): { t: number; label: string }[] {
		const days = spanMs / 86_400_000;
		const out: { t: number; label: string }[] = [];
		const d = new Date(from);
		if (days > 1460) {
			const step = days > 5475 ? 2 : 1;
			for (let y = d.getFullYear() + 1; ; y += step) {
				const t = new Date(y, 0, 1).getTime();
				if (t > to) break;
				out.push({ t, label: String(y) });
			}
		} else if (days > 120) {
			const c = new Date(d.getFullYear(), Math.floor(d.getMonth() / 3) * 3 + 3, 1);
			while (c.getTime() <= to) {
				out.push({
					t: c.getTime(),
					label: c.toLocaleDateString('en-US', { month: 'short', year: '2-digit' })
				});
				c.setMonth(c.getMonth() + 3);
			}
		} else if (days > 10) {
			const c = new Date(d.getFullYear(), d.getMonth() + 1, 1);
			while (c.getTime() <= to) {
				out.push({ t: c.getTime(), label: c.toLocaleDateString('en-US', { month: 'short' }) });
				c.setMonth(c.getMonth() + 1);
			}
		} else if (days > 2) {
			const c = new Date(d.getFullYear(), d.getMonth(), d.getDate() + 1);
			while (c.getTime() <= to) {
				out.push({
					t: c.getTime(),
					label: c.toLocaleDateString('en-US', { month: 'short', day: 'numeric' })
				});
				c.setDate(c.getDate() + 1);
			}
		} else {
			const c = new Date(d);
			c.setMinutes(0, 0, 0);
			c.setHours(c.getHours() + 1);
			const step = days > 1 ? 6 : days > 0.4 ? 3 : 1;
			while (c.getTime() <= to) {
				out.push({ t: c.getTime(), label: c.toLocaleTimeString('en-US', { hour: 'numeric' }) });
				c.setHours(c.getHours() + step);
			}
		}
		const max = Math.floor(plotW / 68);
		if (out.length <= max) return out;
		const every = Math.ceil(out.length / max);
		return out.filter((_, i) => i % every === 0);
	}

	// ── render ──────────────────────────────────────────────────────────────
	function palette(el: HTMLElement) {
		const st = getComputedStyle(el);
		return {
			ink: st.getPropertyValue('--color-foreground').trim() || '#171717',
			quiet: st.getPropertyValue('--color-foreground-subtle').trim() || '#737373',
			hair: st.getPropertyValue('--color-border').trim() || '#e7e7e9',
			paper: st.getPropertyValue('--color-background').trim() || '#fff',
			blue: st.getPropertyValue('--color-primary').trim() || '#2883de'
		};
	}
	type Ink = ReturnType<typeof palette>;

	function fit(el: HTMLCanvasElement, cssW: number, cssH: number) {
		const dpr = window.devicePixelRatio || 1;
		el.width = Math.round(cssW * dpr);
		el.height = Math.round(cssH * dpr);
		el.style.height = `${cssH}px`;
		const ctx = el.getContext('2d');
		ctx?.setTransform(dpr, 0, 0, dpr, 0, 0);
		ctx?.clearRect(0, 0, cssW, cssH);
		return ctx;
	}

	const FONT = '10px ui-sans-serif, -apple-system, system-ui, sans-serif';

	/**
	 * Paint the raster into an offscreen bitmap, one pixel per cell.
	 *
	 * Up to 1,400 columns of 24 rows is 33,600 rects — too many to lay down on
	 * every pan frame. Painted once per response and then scaled by `drawImage`,
	 * it costs one blit, and panning stays as responsive as the lanes.
	 */
	function paintClock(ck: Clock) {
		const el = canvas;
		if (!el) return;
		const bmp = document.createElement('canvas');
		bmp.width = ck.columns;
		bmp.height = 24;
		const bx = bmp.getContext('2d');
		if (!bx) return;

		const c = palette(el);
		const img = bx.createImageData(ck.columns, 24);
		const ink = c.ink.startsWith('#') ? c.ink : '#171717';
		const r = parseInt(ink.slice(1, 3), 16);
		const g = parseInt(ink.slice(3, 5), 16);
		const b = parseInt(ink.slice(5, 7), 16);

		// Two factors, because one cannot do it.
		//
		// SHAPE is the hour against that column's own busiest hour, so every day
		// gets full contrast and the rhythm is legible on a quiet Sunday and a
		// loud Monday alike. VOLUME then dims the column by how busy it was
		// against the record, with a floor, so a quiet day still reads.
		//
		// A single global scale — the first attempt — washed the whole raster to
		// mid-grey: most columns peak far below the record's busiest hour, so
		// every one of them rendered faint and the band it was drawing vanished.
		for (let col = 0; col < ck.columns; col++) {
			const cp = Math.max(ck.column_peak[col], 1);
			const volume = Math.max(0.45, Math.min(1, Math.sqrt(cp / Math.max(ck.peak, 1)) * 1.6));
			for (let h = 0; h < 24; h++) {
				const v = ck.cells[col * 24 + h];
				if (v <= 0) continue;
				const shape = Math.pow(v / cp, 0.6);
				const i = (h * ck.columns + col) * 4;
				img.data[i] = r;
				img.data[i + 1] = g;
				img.data[i + 2] = b;
				img.data[i + 3] = Math.round(Math.min(1, shape * volume) * 245);
			}
		}
		bx.putImageData(img, 0, 0);
		clockBitmap = bmp;
	}

	/**
	 * The band itself: hour rules, then the raster, then the hours that matter.
	 *
	 * Midnight and noon are drawn over the data rather than under it — they are
	 * the reference a reader measures the band against, and a gridline hidden
	 * beneath the ink cannot be measured against anything.
	 */
	function drawClock(ctx: CanvasRenderingContext2D, c: Ink, W: number) {
		if (!clockH || !clockBitmap) return;
		const top = spineH;

		// Smoothing stays ON. Nearest-neighbour at a non-integer scale drops whole
		// columns, and a dropped column is a white vertical streak that reads as
		// a day with nothing in it — the raster inventing gaps in a life.
		ctx.imageSmoothingEnabled = true;
		ctx.imageSmoothingQuality = 'high';
		ctx.drawImage(clockBitmap, 0, top, W, clockH);

		ctx.strokeStyle = c.ink;
		for (const h of [0, 6, 12, 18]) {
			const y = Math.round(top + h * hourH) + 0.5;
			ctx.globalAlpha = h === 0 || h === 12 ? 0.16 : 0.08;
			ctx.beginPath();
			ctx.moveTo(0, y);
			ctx.lineTo(W, y);
			ctx.stroke();
		}
		ctx.globalAlpha = 1;
	}

	function draw() {
		const el = canvas;
		if (!el || lanes.length === 0) return;
		const W = el.clientWidth;
		const H = plotH;
		const ctx = fit(el, W, H);
		if (!ctx) return;
		const c = palette(el);
		ctx.font = FONT;
		ctx.textBaseline = 'top';

		// Gridlines under everything, so they read as paper rather than data.
		for (const m of ticks()) {
			const x = Math.round(xOf(m.t)) + 0.5;
			if (x < 0 || x > W) continue;
			ctx.strokeStyle = c.hair;
			ctx.globalAlpha = 0.6;
			ctx.beginPath();
			ctx.moveTo(x, 0);
			ctx.lineTo(x, H - AXIS_H + 2);
			ctx.stroke();
			ctx.globalAlpha = 1;
			ctx.fillStyle = c.quiet;
			ctx.fillText(m.label, x + 4, H - AXIS_H + 5);
		}

		drawNow(ctx, c, H);
		if (spineH) drawSpine(ctx, c, W);
		drawClock(ctx, c, W);
		lanes.forEach((lane, row) => drawLane(ctx, c, W, lane, row));
		drawStay(ctx, c);
		drawSelection(ctx, c, W, H);
		drawCursor(ctx, c, H);
	}

	/** Today. Nine years of chart with no marker for now is a chart you are not in. */
	function drawNow(ctx: CanvasRenderingContext2D, c: Ink, H: number) {
		const x = xOf(Date.now());
		if (x < 0 || x > plotW) return;
		ctx.strokeStyle = c.ink;
		ctx.globalAlpha = 0.25;
		ctx.setLineDash([2, 3]);
		ctx.beginPath();
		ctx.moveTo(Math.round(x) + 0.5, 0);
		ctx.lineTo(Math.round(x) + 0.5, H - AXIS_H);
		ctx.stroke();
		ctx.setLineDash([]);
		ctx.globalAlpha = 1;
	}

	/**
	 * The spine: what the record has already been understood to be.
	 *
	 * Sleep is drawn quieter than waking life on purpose — it is the largest
	 * block on most days and at full weight it would be the only thing you saw.
	 */
	function drawSpine(ctx: CanvasRenderingContext2D, c: Ink, W: number) {
		const top = 4;
		const h = SPINE_H - 10;

		// Sub-pixel events are not information. A bracket over the same span,
		// labelled and pointing at itself, is — and it is the only thing on this
		// view that says the interpreted layer exists at all.
		if (coverageTiny && coverage) {
			const a = xOf(coverage[0]);
			const b = xOf(coverage[1]);
			const x = Math.round(Math.max(0, a)) + 0.5;
			const w = Math.max(3, Math.min(W, b) - Math.max(0, a));
			ctx.strokeStyle = c.blue;
			ctx.globalAlpha = 0.9;
			ctx.beginPath();
			ctx.moveTo(x, top + h);
			ctx.lineTo(x, top + 3);
			ctx.lineTo(x + w, top + 3);
			ctx.lineTo(x + w, top + h);
			ctx.stroke();
			ctx.globalAlpha = 1;
			// Label to whichever side has room, so it is never off the canvas.
			const text = `${daysProcessed} days interpreted`;
			const tw = ctx.measureText(text).width;
			ctx.fillStyle = c.blue;
			ctx.textBaseline = 'middle';
			ctx.fillText(
				text,
				x + w + 6 + tw < W ? x + w + 6 : Math.max(2, x - tw - 6),
				top + h / 2
			);
			ctx.textBaseline = 'top';
			return;
		}

		for (const e of events) {
			const a = xOf(new Date(e.start).getTime());
			const b = e.end ? xOf(new Date(e.end).getTime()) : a + 2;
			if (b < 0 || a > W) continue;
			const x = Math.max(0, a);
			const w = Math.max(2, Math.min(W, b) - x);
			const sleep = e.tag === 'sleep';
			const transit = e.tag === 'transit';

			ctx.fillStyle = c.ink;
			ctx.globalAlpha = sleep ? 0.05 : transit ? 0.08 : 0.11;
			ctx.fillRect(x, top, w, h);
			ctx.globalAlpha = sleep ? 0.2 : 0.4;
			ctx.strokeStyle = c.ink;
			ctx.strokeRect(Math.round(x) + 0.5, top + 0.5, Math.max(1, w - 1), h - 1);
			ctx.globalAlpha = 1;

			// Only label what there is room to label. A clipped word is worse than
			// no word — it reads as a rendering fault rather than as a name.
			const label = e.label ?? e.tag ?? '';
			if (label && w > 46) {
				ctx.save();
				ctx.beginPath();
				ctx.rect(x + 3, top, w - 6, h);
				ctx.clip();
				ctx.fillStyle = c.ink;
				ctx.globalAlpha = sleep ? 0.5 : 0.85;
				ctx.textBaseline = 'middle';
				ctx.fillText(label, x + 5, top + h / 2);
				ctx.restore();
				ctx.globalAlpha = 1;
				ctx.textBaseline = 'top';
			}
		}
	}

	function drawLane(ctx: CanvasRenderingContext2D, c: Ink, W: number, lane: LifelineLane, row: number) {
		const { top, h: rowH } = rowTops[row];

		if (hotLane === row && !dormant.has(lane.id)) {
			ctx.fillStyle = c.hair;
			ctx.globalAlpha = 0.16;
			ctx.fillRect(0, top, W, rowH);
			ctx.globalAlpha = 1;
		}
		if (dormant.has(lane.id)) return;

		const pad = 6;
		const base = top + rowH - pad;
		const usable = rowH - pad * 2;

		// Outside this lane's coverage. A flat tint, not hatching — hatching
		// competed with the data it was meant to qualify.
		const seen = lane.first_seen ? new Date(lane.first_seen).getTime() : from;
		const x1 = Math.min(xOf(seen), W);
		if (seen > from && x1 > 0) {
			ctx.fillStyle = c.hair;
			ctx.globalAlpha = 0.28;
			ctx.fillRect(0, top + 2, x1, rowH - 4);
			ctx.globalAlpha = 1;
		}

		if (momentMode) {
			drawMoments(ctx, c, lane, top, rowH);
			return;
		}

		const n = lane.density.length;
		const colW = W / n;
		const peak = lane.peak || 1;

		if (lane.kind === 'rate') {
			// A band between the lane's own floor and ceiling, broken wherever the
			// measure has nothing to say. A rate of zero is not a low rate — it is
			// an absent one, and joining across it draws a heartbeat that stopped.
			const lo = lane.floor;
			const hi = Math.max(peak, lo + 1e-9);
			const yOf = (v: number) => base - ((v - lo) / (hi - lo)) * usable;
			ctx.beginPath();
			let open = false;
			for (let i = 0; i < n; i++) {
				const v = lane.density[i];
				const x = i * colW + colW / 2;
				if (v <= 0) {
					open = false;
					continue;
				}
				if (!open) {
					ctx.moveTo(x, yOf(v));
					open = true;
				} else ctx.lineTo(x, yOf(v));
			}
			ctx.strokeStyle = c.ink;
			ctx.globalAlpha = 0.9;
			ctx.lineWidth = 1.25;
			ctx.lineJoin = 'round';
			ctx.stroke();
			ctx.globalAlpha = 1;
			ctx.lineWidth = 1;
		} else {
			ctx.strokeStyle = c.hair;
			ctx.beginPath();
			ctx.moveTo(Math.max(0, x1), Math.round(base) + 0.5);
			ctx.lineTo(W, Math.round(base) + 0.5);
			ctx.stroke();
			ctx.fillStyle = c.ink;
			for (let i = 0; i < n; i++) {
				const v = lane.density[i];
				if (v <= 0) continue;
				// Square-rooted: one loud day would otherwise flatten a decade of
				// ordinary ones into nothing.
				const f = Math.sqrt(v / peak);
				const hgt = Math.max(1.5, f * usable);
				ctx.globalAlpha = 0.35 + 0.6 * f;
				ctx.fillRect(i * colW, base - hgt, Math.max(1, colW - 0.35), hgt);
			}
			ctx.globalAlpha = 1;
		}
	}

	/**
	 * Every record at its own instant.
	 *
	 * The bar chart's failure at close range was spending a whole row to encode
	 * the number one. A tick with a cap costs the same pixels and encodes WHICH —
	 * and being hoverable, it is the thing the panel is pointing at.
	 */
	function drawMoments(ctx: CanvasRenderingContext2D, c: Ink, lane: LifelineLane, top: number, rowH: number) {
		const list = momentsByLane[lane.id];
		if (!list?.length) return;
		const y0 = top + 8;
		const y1 = top + rowH - 8;
		for (const r of list) {
			const x = Math.round(xOf(new Date(r.at).getTime())) + 0.5;
			if (x < -2 || x > plotW + 2) continue;
			const on = chartHover?.id === r.id || panelHover === r.id;
			ctx.strokeStyle = on ? c.blue : c.ink;
			ctx.globalAlpha = on ? 1 : 0.45;
			ctx.lineWidth = on ? 1.5 : 1;
			ctx.beginPath();
			ctx.moveTo(x, y0);
			ctx.lineTo(x, y1);
			ctx.stroke();
			ctx.fillStyle = on ? c.blue : c.ink;
			ctx.globalAlpha = on ? 1 : 0.6;
			ctx.beginPath();
			ctx.arc(x, y0, on ? 3 : 1.75, 0, Math.PI * 2);
			ctx.fill();
		}
		ctx.globalAlpha = 1;
		ctx.lineWidth = 1;
	}

	/** A place picked on the map, marked back onto the location lane. */
	function drawStay(ctx: CanvasRenderingContext2D, c: Ink) {
		if (!pickedStay?.first || !pickedStay.last) return;
		const row = lanes.findIndex((l) => l.id.startsWith('location'));
		if (row < 0) return;
		const { top, h } = rowTops[row];
		const a = xOf(new Date(pickedStay.first).getTime());
		const b = xOf(new Date(pickedStay.last).getTime());
		ctx.fillStyle = c.blue;
		ctx.globalAlpha = 0.16;
		ctx.fillRect(a, top + 2, Math.max(2, b - a), h - 4);
		ctx.globalAlpha = 1;
	}

	function drawSelection(ctx: CanvasRenderingContext2D, c: Ink, W: number, H: number) {
		if (!selection) return;
		const a = Math.max(0, Math.min(xOf(selection.a), xOf(selection.b)));
		const b = Math.min(W, Math.max(xOf(selection.a), xOf(selection.b)));
		// Dim what is outside rather than tint what is inside, so the data in the
		// band keeps the exact ink it had before you selected it.
		ctx.fillStyle = c.paper;
		ctx.globalAlpha = 0.62;
		ctx.fillRect(0, 0, a, H - AXIS_H);
		ctx.fillRect(b, 0, W - b, H - AXIS_H);
		ctx.globalAlpha = 1;
		ctx.strokeStyle = c.blue;
		ctx.beginPath();
		ctx.moveTo(Math.round(a) + 0.5, 0);
		ctx.lineTo(Math.round(a) + 0.5, H - AXIS_H);
		ctx.moveTo(Math.round(b) + 0.5, 0);
		ctx.lineTo(Math.round(b) + 0.5, H - AXIS_H);
		ctx.stroke();
		// Grips matching the overview strip's — one interaction, learned once.
		ctx.fillStyle = c.blue;
		const mid = (H - AXIS_H) / 2;
		for (const x of [a, b]) ctx.fillRect(Math.round(x) - 1.5, mid - 8, 3, 16);
	}

	function drawCursor(ctx: CanvasRenderingContext2D, c: Ink, H: number) {
		if (!cursor || drag) return;
		const cx = cursor.x;
		ctx.strokeStyle = c.ink;
		ctx.globalAlpha = 0.3;
		ctx.beginPath();
		ctx.moveTo(Math.round(cx) + 0.5, 0);
		ctx.lineTo(Math.round(cx) + 0.5, H - AXIS_H);
		ctx.stroke();
		ctx.globalAlpha = 1;
		if (momentMode) return;

		// A dot where the crosshair meets each series, so the readout reads as
		// attached to the chart rather than printed beside it.
		const i = bucketAt(cx);
		if (i < 0) return;
		lanes.forEach((lane, row) => {
			if (dormant.has(lane.id)) return;
			const v = lane.density[i] ?? 0;
			if (v <= 0) return;
			const { top, h: rowH } = rowTops[row];
			const base = top + rowH - 6;
			const usable = rowH - 12;
			const y =
				lane.kind === 'rate'
					? base - ((v - lane.floor) / Math.max(1e-9, lane.peak - lane.floor)) * usable
					: base - Math.max(1.5, Math.sqrt(v / (lane.peak || 1)) * usable);
			ctx.fillStyle = c.blue;
			ctx.beginPath();
			ctx.arc(cx, y, 2.5, 0, Math.PI * 2);
			ctx.fill();
		});
	}

	function drawMini() {
		const el = mini;
		if (!el || overview.length === 0 || !bounds) return;
		const W = el.clientWidth;
		const H = 40;
		const ctx = fit(el, W, H);
		if (!ctx) return;
		const c = palette(el);

		const n = overview.length;
		const peak = Math.max(...overview) || 1;
		const colW = W / n;
		ctx.fillStyle = c.ink;
		ctx.globalAlpha = 0.32;
		for (let i = 0; i < n; i++) {
			const v = overview[i];
			if (v <= 0) continue;
			const hgt = Math.max(1, Math.sqrt(v / peak) * (H - 8));
			ctx.fillRect(i * colW, H - 4 - hgt, Math.max(0.6, colW), hgt);
		}
		ctx.globalAlpha = 1;

		const span = bounds.to - bounds.from;
		// How far interpretation reaches, on the same axis as everything else —
		// the asymmetry between raw and processed made visible rather than
		// discovered by switching a toggle and finding nothing there.
		if (coverage) {
			const ca = ((coverage[0] - bounds.from) / span) * W;
			const cb = ((coverage[1] - bounds.from) / span) * W;
			ctx.fillStyle = c.ink;
			ctx.globalAlpha = 0.55;
			ctx.fillRect(ca, H - 2, Math.max(1.5, cb - ca), 2);
			ctx.globalAlpha = 1;
		}

		const a = ((from - bounds.from) / span) * W;
		const b = ((to - bounds.from) / span) * W;
		const bw = Math.max(2, b - a);
		ctx.fillStyle = c.blue;
		ctx.globalAlpha = 0.1;
		ctx.fillRect(a, 0, bw, H);
		ctx.globalAlpha = 1;
		ctx.strokeStyle = c.blue;
		ctx.strokeRect(Math.round(a) + 0.5, 0.5, bw, H - 1);
		ctx.fillStyle = c.blue;
		for (const x of [a, a + bw]) ctx.fillRect(Math.round(x) - 1.5, H / 2 - 7, 3, 14);
	}

	$effect(() => {
		// Re-read the reactive inputs so hover and mode changes repaint.
		void [chartHover, panelHover, momentMode, events, clock, hotLane, moments];
		if (!loading) {
			draw();
			drawMini();
		}
	});

	const cursorClass = $derived(
		drag?.mode === 'pan' || spaceHeld
			? 'grabbing'
			: cursor && selZone(cursor.x) === 'move-sel'
				? 'move'
				: cursor && selZone(cursor.x)
					? 'resize'
					: 'cross'
	);
</script>

<svelte:window
	onkeyup={(e) => {
		if (e.key === ' ') spaceHeld = false;
	}}
/>

<div class="lifeline">
	{#if loading}
		<p class="quiet pad">Reading the record…</p>
	{:else if lanes.length === 0}
		<p class="quiet pad">
			Nothing recorded yet. The lifeline draws whatever the collectors have
			gathered — it needs no articles and no AI.
		</p>
	{:else}
		<header class="bar">
			<span class="range">
				{fmtDate(new Date(from))} <span class="dash">→</span> {fmtDate(new Date(to))}
			</span>
			<span class="grain">
				{fmtSpan(spanMs)} ·
				{momentMode ? `${moments.length} records` : `one bar per ${grain}`}
			</span>
			<span class="grow"></span>
			<div class="seg zoom" role="group" aria-label="Zoom">
				<button type="button" aria-label="Zoom out" onclick={() => zoomAt(plotW / 2, 2, true)}
					>−</button
				>
				<button type="button" aria-label="Zoom in" onclick={() => zoomAt(plotW / 2, 0.5, true)}
					>+</button
				>
			</div>
			<button
				type="button"
				class="ghost"
				disabled={!bounds || (from <= bounds.from && to >= bounds.to)}
				onclick={reset}>Whole record</button
			>
			<button
				type="button"
				class="ghost keys"
				aria-label="Keyboard shortcuts"
				onclick={() => (showKeys = !showKeys)}>?</button
			>
		</header>

		<div class="main">
			<div class="left">
				<div class="rows" bind:clientHeight={stageH}>
					<div class="labels" style="height:{plotH}px">
						{#if spineH}
							<div class="lab spine" style="height:{SPINE_H}px">
								<span class="name static caps">events</span>
								{#if coverageTiny && coverage}
									<button
										type="button"
										class="meas jump"
										onclick={() => glideTo(coverage![0], coverage![1], 420)}
									>
										{daysProcessed} days →
									</button>
								{:else}
									<span class="meas static">{events.length} named</span>
								{/if}
							</div>
						{/if}
						{#if clockH}
							<div class="lab clock" style="height:{clockH}px">
								<span class="name static">day-clock</span>
								<span class="meas static">by hour · {clock?.timezone.split('/').pop()}</span>
								{#each [0, 6, 12, 18] as h (h)}
									<span class="hour" style="top:{(h / 24) * 100}%">{hourLabel(h)}</span>
								{/each}
							</div>
						{/if}
						{#each lanes as lane, i (lane.id)}
							<div
								class="lab"
								class:dim={dormant.has(lane.id)}
								class:hot={hotLane === i}
								style="height:{rowTops[i].h}px"
								onmouseenter={() => (hotLane = i)}
								role="presentation"
							>
								{#if dormant.has(lane.id)}
									<span class="name static">{shortName(lane.id)}</span>
									<button
										type="button"
										class="meas jump"
										onclick={() => goTo(new Date(lane.first_seen!).getTime(), 86_400_000 * 30)}
									>
										from {shortDate(new Date(lane.first_seen!).getTime())} →
									</button>
								{:else}
									<button
										type="button"
										class="name"
										class:focused={focusLane === lane.id.split('/')[0]}
										title="Show only this lane in the panel"
										onclick={() => {
											const root = lane.id.split('/')[0];
											focusLane = focusLane === root ? null : root;
										}}>{shortName(lane.id)}</button
									>
									<button
										type="button"
										class="meas"
										aria-haspopup="listbox"
										aria-expanded={menuFor === lane.id}
										onclick={() => (menuFor = menuFor === lane.id ? null : lane.id)}
									>
										{lane.measure_label}<span class="caret">▾</span>
									</button>
								{/if}
								{#if menuFor === lane.id}
									<ul class="menu" role="listbox">
										<li>
											<button
												type="button"
												class:sel={lane.measure === 'records'}
												onclick={() => chooseMeasure(lane.id, 'records')}>records</button
											>
										</li>
										{#each lane.available as m (m.id)}
											<li>
												<button
													type="button"
													class:sel={lane.measure === m.id}
													onclick={() => chooseMeasure(lane.id, m.id)}
												>
													{m.label}{#if m.unit}<span class="u">{m.unit}</span>{/if}
												</button>
											</li>
										{/each}
										{#if lane.sources.length > 1 || lane.id.includes('/')}
											<li class="rule">
												<button type="button" onclick={() => toggleExpand(lane.id)}>
													{lane.id.includes('/') ? 'Combine sources' : 'Split into sources'}
												</button>
											</li>
										{/if}
									</ul>
								{/if}
							</div>
						{/each}
					</div>

					<div class="plot" bind:this={plotEl}>
						<canvas
							bind:this={canvas}
							class={cursorClass}
							tabindex="0"
							aria-label="Recorded activity over time. Drag to select, arrow keys to move, plus and minus to zoom, question mark for shortcuts."
							onpointerdown={onDown}
							onpointermove={onMove}
							onpointerup={onUp}
							onpointercancel={onUp}
							onpointerleave={() => {
								cursor = null;
								hotLane = null;
								chartHover = null;
								draw();
							}}
							ondblclick={(e) => {
								const x = e.clientX - canvas!.getBoundingClientRect().left;
								zoomAt(x, e.altKey ? 2 : 0.5, true);
							}}
							onwheel={onWheel}
							onkeydown={onKey}
							onfocus={() => (focused = true)}
							onblur={() => (focused = false)}
						></canvas>

						{#if chartHover}
							<div class="tip" style="left:{tipLeft}px">
								<div class="when">
									{new Date(chartHover.at).toLocaleString('en-US', {
										month: 'short',
										day: 'numeric',
										year: 'numeric',
										hour: 'numeric',
										minute: '2-digit'
									})}
								</div>
								{#if chartHover.label}<div class="ml">{chartHover.label}</div>{/if}
								{#if chartHover.preview}<div class="mp">{chartHover.preview}</div>{/if}
							</div>
						{:else if readout && cursorTime}
							<div class="tip" style="left:{tipLeft}px">
								<div class="when">
									{fmtDate(cursorTime)}{#if cursorHour !== null}<span class="hr"
											>{hourLabel(cursorHour)}</span
										>{/if}
								</div>
								{#each readout as r (r.id)}
									<div class="tr">
										<span class="k">{shortName(r.id)}</span>
										<span class="v">{r.dormant ? '—' : fmt(r.v, r.unit)}</span>
									</div>
								{/each}
							</div>
						{/if}

						{#if nowhereHere && nearestData}
							<div class="nothing">
								<p>Nothing recorded in this window.</p>
								<button type="button" onclick={() => goTo(nearestData, 86_400_000 * 30)}>
									Go to {shortDate(nearestData)}
								</button>
							</div>
						{/if}

						{#if showKeys}
							<div class="keysheet">
								<dl>
									<dt>drag</dt>
									<dd>select a range</dd>
									<dt>⌘ scroll</dt>
									<dd>zoom</dd>
									<dt>⇧ scroll</dt>
									<dd>pan</dd>
									<dt>space drag</dt>
									<dd>pan</dd>
									<dt>← →</dt>
									<dd>move</dd>
									<dt>+ −</dt>
									<dd>zoom</dd>
									<dt>⏎</dt>
									<dd>zoom to selection</dd>
									<dt>0</dt>
									<dd>whole record</dd>
									<dt>esc</dt>
									<dd>clear</dd>
								</dl>
							</div>
						{/if}
					</div>
				</div>

				<div
					class="mini"
					class:grab={miniHot === 'move'}
					class:resize={miniHot === 'left' || miniHot === 'right'}
				>
					<canvas
						bind:this={mini}
						aria-label="The whole record. Drag the box to move the window, its edges to change the span."
						onpointerdown={miniDown}
						onpointermove={miniMove}
						onpointerup={() => (miniDrag = null)}
						onpointercancel={() => (miniDrag = null)}
						onpointerleave={() => {
							if (!miniDrag) miniHot = null;
						}}
					></canvas>
				</div>
			</div>

			<aside class="inspector">
				<header class="ihead">
					<div class="titles">
						<h3>{selection ? 'Selection' : 'In view'}</h3>
						<p class="sub">{fmtDate(new Date(panelFrom))} · {fmtSpan(panelTo - panelFrom)}</p>
					</div>
					<div class="seg" role="group" aria-label="Register">
						<button
							type="button"
							class:on={register === 'raw'}
							title="Every collector row, back to the start of the record"
							onclick={() => (register = 'raw')}>Raw</button
						>
						<button
							type="button"
							class:on={register === 'processed'}
							title="Only what Virtues has interpreted into days and events"
							onclick={() => (register = 'processed')}>Processed</button
						>
					</div>
				</header>

				{#if selection}
					<div class="acts">
						<button type="button" class="primary" onclick={zoomToSelection}>
							Zoom to selection
						</button>
						<button type="button" class="ghost" onclick={() => (selection = null)}>Clear</button>
					</div>
				{/if}

				{#if focusLane}
					<button type="button" class="release" onclick={() => (focusLane = null)}>
						{shortName(focusLane)} only — show everything
					</button>
				{/if}

				{#if register === 'raw' && lanes.some((l) => l.id.startsWith('location'))}
					<LifelineMap
						from={panelFrom}
						to={panelTo}
						onpick={(s) => {
							pickedStay = s;
							draw();
						}}
					/>
				{/if}

				<LifelineFeed
					from={panelFrom}
					to={panelTo}
					lane={focusLane}
					mode={register}
					highlight={chartHover?.id ?? null}
					coverageDays={daysProcessed}
					coverageStart={coverage?.[0] ?? null}
					coverageEnd={coverage?.[1] ?? null}
					onhover={(id) => (panelHover = id)}
					ongoto={(t) => goTo(t, 86_400_000 * 2)}
				/>

				{#if summary && register === 'raw'}
					<details class="totals">
						<summary>Totals</summary>
						<ul class="stats">
							{#each summary as s (s.id)}
								<li class:dim={s.dormant}>
									<span class="lab">{shortName(s.id)}</span>
									<span class="met">{s.label}</span>
									<span class="num">{s.dormant ? 'not collecting' : fmt(s.value, s.unit)}</span>
								</li>
							{/each}
						</ul>
					</details>
				{/if}
			</aside>
		</div>
	{/if}
</div>

<style>
	@reference "../../../app.css";

	.lifeline {
		display: flex;
		flex-direction: column;
		width: 100%;
	}

	.pad {
		padding: 1rem 0;
	}

	/* ── chrome ─────────────────────────────────────────────────────────── */
	.bar {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0 0 0.625rem;
		font-size: 0.75rem;
	}

	.grow {
		flex: 1;
	}

	.range {
		font-weight: 500;
		font-variant-numeric: tabular-nums;
	}

	.dash {
		color: var(--color-foreground-subtle);
		padding: 0 0.125rem;
	}

	.grain,
	.quiet {
		color: var(--color-foreground-subtle);
	}

	.seg {
		display: inline-flex;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		overflow: hidden;
	}

	.seg button {
		padding: 0.1875rem 0.5rem;
		background: none;
		border: none;
		font: inherit;
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
		cursor: pointer;
		transition:
			background-color 120ms ease,
			color 120ms ease;
	}

	.seg button + button {
		border-left: 1px solid var(--color-border);
	}

	.seg button:hover {
		color: var(--color-foreground);
	}

	.seg button.on {
		background: var(--color-surface-elevated, var(--color-highlight));
		color: var(--color-foreground);
	}

	.zoom button {
		min-width: 22px;
		font-size: 0.8125rem;
		line-height: 1;
	}

	.ghost {
		padding: 0.1875rem 0.5rem;
		background: none;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		font: inherit;
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
		cursor: pointer;
		transition:
			color 120ms ease,
			border-color 120ms ease;
	}

	.ghost:hover:not(:disabled) {
		color: var(--color-foreground);
		border-color: var(--color-foreground-subtle);
	}

	.ghost:disabled {
		opacity: 0.4;
		cursor: default;
	}

	.keys {
		min-width: 24px;
	}

	/* ── layout ─────────────────────────────────────────────────────────── */
	.main {
		display: flex;
		align-items: stretch;
	}

	.left {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
	}

	.rows {
		display: flex;
		align-items: flex-start;
		min-height: 320px;
		height: clamp(320px, calc(100vh - 300px), 620px);
	}

	.labels {
		flex: 0 0 140px;
		width: 140px;
		padding-right: 0.75rem;
	}

	.plot {
		position: relative;
		flex: 1;
		min-width: 0;
	}

	canvas {
		display: block;
		width: 100%;
		touch-action: none;
		user-select: none;
	}

	canvas:focus {
		outline: none;
	}

	canvas:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
	}

	.cross {
		cursor: crosshair;
	}

	.grabbing {
		cursor: grabbing;
	}

	.move {
		cursor: grab;
	}

	.resize {
		cursor: ew-resize;
	}

	/* ── lane labels ────────────────────────────────────────────────────── */
	/* Space separates the label column from the plot. A rule down the middle of
	   an already-quiet interface is one line too many. */
	.lab {
		position: relative;
		display: flex;
		flex-direction: column;
		justify-content: center;
		gap: 0.0625rem;
		padding-right: 0.25rem;
	}

	/* Hours sit against the rows they name, so the band can be read against the
	   clock without counting. */
	.lab.clock {
		justify-content: flex-start;
		padding-top: 0.125rem;
	}

	.hour {
		position: absolute;
		right: 0.375rem;
		font-size: 0.5625rem;
		line-height: 1;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
		opacity: 0.7;
		transform: translateY(-0.25em);
	}

	.hr {
		margin-left: 0.375rem;
		font-weight: 400;
		color: var(--color-foreground-subtle);
	}

	.lab.hot .name {
		color: var(--color-foreground);
	}

	.lab.dim {
		opacity: 0.6;
	}

	.name,
	.meas {
		align-self: flex-start;
		background: none;
		border: none;
		padding: 0;
		font: inherit;
		text-align: left;
		cursor: pointer;
	}

	.name {
		font-size: 0.8125rem;
		line-height: 1.2;
		color: var(--color-foreground);
	}

	.caps {
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
	}

	.name:hover:not(.static) {
		text-decoration: underline;
		text-underline-offset: 3px;
		text-decoration-color: var(--color-border);
	}

	.name.focused {
		font-weight: 600;
	}

	.meas {
		display: inline-flex;
		align-items: center;
		gap: 0.125rem;
		font-size: 0.6875rem;
		line-height: 1.2;
		color: var(--color-foreground-subtle);
	}

	.meas:hover:not(.static) {
		color: var(--color-foreground);
	}

	.static {
		cursor: default;
	}

	.jump {
		color: var(--color-primary);
	}

	.caret {
		font-size: 0.5rem;
		opacity: 0.6;
	}

	.menu {
		position: absolute;
		z-index: 20;
		top: 100%;
		left: 0;
		min-width: 152px;
		margin: 0.125rem 0 0;
		padding: 0.1875rem;
		list-style: none;
		background: var(--color-background);
		border: 1px solid var(--color-border);
		border-radius: 7px;
		box-shadow:
			0 1px 2px rgba(0, 0, 0, 0.04),
			0 8px 24px rgba(0, 0, 0, 0.08);
	}

	.menu button {
		display: flex;
		justify-content: space-between;
		gap: 0.75rem;
		width: 100%;
		padding: 0.1875rem 0.375rem;
		background: none;
		border: none;
		border-radius: 4px;
		font: inherit;
		font-size: 0.75rem;
		color: var(--color-foreground);
		text-align: left;
		cursor: pointer;
	}

	.menu button:hover {
		background: var(--color-highlight);
	}

	.menu button.sel {
		font-weight: 500;
	}

	.menu .u {
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}

	.menu .rule {
		margin-top: 0.1875rem;
		padding-top: 0.1875rem;
		border-top: 1px solid var(--color-border);
	}

	/* ── overlays ───────────────────────────────────────────────────────── */
	.tip {
		position: absolute;
		top: 4px;
		min-width: 152px;
		max-width: 240px;
		padding: 0.375rem 0.5rem;
		border: 1px solid var(--color-border);
		border-radius: 7px;
		background: var(--color-background);
		font-size: 0.6875rem;
		pointer-events: none;
		box-shadow:
			0 1px 2px rgba(0, 0, 0, 0.04),
			0 6px 18px rgba(0, 0, 0, 0.07);
	}

	.when {
		margin-bottom: 0.25rem;
		font-weight: 500;
		font-variant-numeric: tabular-nums;
	}

	.ml {
		font-size: 0.75rem;
	}

	.mp {
		margin-top: 0.0625rem;
		color: var(--color-foreground-subtle);
		line-height: 1.4;
		overflow-wrap: anywhere;
	}

	.tr {
		display: flex;
		justify-content: space-between;
		gap: 0.75rem;
		line-height: 1.45;
	}

	.k {
		color: var(--color-foreground-subtle);
	}

	/* Fixed column so the tooltip does not reflow as you scrub across it. */
	.v {
		min-width: 62px;
		text-align: right;
		font-variant-numeric: tabular-nums;
	}

	.nothing {
		position: absolute;
		inset: 0;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 0.5rem;
		pointer-events: none;
	}

	.nothing p {
		margin: 0;
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
	}

	.nothing button {
		pointer-events: auto;
		padding: 0.25rem 0.625rem;
		background: var(--color-background);
		border: 1px solid var(--color-border);
		border-radius: 6px;
		font: inherit;
		font-size: 0.6875rem;
		color: var(--color-primary);
		cursor: pointer;
	}

	.keysheet {
		position: absolute;
		z-index: 30;
		top: 8px;
		right: 8px;
		padding: 0.5rem 0.625rem;
		background: var(--color-background);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		box-shadow:
			0 1px 2px rgba(0, 0, 0, 0.04),
			0 8px 24px rgba(0, 0, 0, 0.08);
	}

	.keysheet dl {
		display: grid;
		grid-template-columns: auto auto;
		gap: 0.125rem 0.75rem;
		margin: 0;
		font-size: 0.6875rem;
	}

	.keysheet dt {
		color: var(--color-foreground);
	}

	.keysheet dd {
		margin: 0;
		color: var(--color-foreground-subtle);
	}

	/* ── overview strip ─────────────────────────────────────────────────── */
	.mini {
		margin-top: 0.5rem;
		padding-left: 140px;
	}

	.mini canvas {
		width: 100%;
		cursor: default;
	}

	.mini.grab canvas {
		cursor: grab;
	}

	.mini.resize canvas {
		cursor: ew-resize;
	}

	/* ── inspector ──────────────────────────────────────────────────────── */
	.inspector {
		flex: 0 0 340px;
		width: 340px;
		margin-left: 1rem;
		padding-left: 1rem;
		border-left: 1px solid var(--color-border);
		max-height: clamp(320px, calc(100vh - 250px), 720px);
		overflow-y: auto;
		overscroll-behavior: contain;
	}

	.ihead {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 0.5rem;
		margin-bottom: 0.625rem;
	}

	.titles {
		min-width: 0;
	}

	.inspector h3 {
		margin: 0;
		font-size: 0.75rem;
		font-weight: 500;
	}

	.sub {
		margin: 0.125rem 0 0;
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}

	.release {
		display: block;
		width: 100%;
		margin-bottom: 0.625rem;
		padding: 0.1875rem 0.375rem;
		background: var(--color-highlight);
		border: none;
		border-radius: 5px;
		font: inherit;
		font-size: 0.625rem;
		color: var(--color-foreground);
		text-align: left;
		cursor: pointer;
	}

	.acts {
		display: flex;
		gap: 0.375rem;
		margin-bottom: 0.625rem;
	}

	.primary {
		flex: 1;
		padding: 0.25rem 0.5rem;
		background: var(--color-primary);
		border: 1px solid var(--color-primary);
		border-radius: 6px;
		font: inherit;
		font-size: 0.6875rem;
		color: #fff;
		cursor: pointer;
	}

	.primary:hover {
		background: var(--color-primary-hover, var(--color-primary));
	}

	/* Totals are a footnote now. They were the panel, and being the panel was
	   the whole problem: a column of sums is what chat already does badly. */
	.totals {
		margin-top: 0.75rem;
		border-top: 1px solid var(--color-border);
		padding-top: 0.5rem;
	}

	.totals summary {
		font-size: 0.625rem;
		color: var(--color-foreground-subtle);
		cursor: pointer;
		list-style: none;
	}

	.totals summary::-webkit-details-marker {
		display: none;
	}

	.totals summary::before {
		content: '▸ ';
		font-size: 0.5rem;
	}

	.totals[open] summary::before {
		content: '▾ ';
	}

	.stats {
		list-style: none;
		margin: 0.25rem 0 0;
		padding: 0;
	}

	.stats li {
		display: grid;
		grid-template-columns: 1fr auto;
		gap: 0 0.5rem;
		padding: 0.3125rem 0;
	}

	.stats li.dim {
		opacity: 0.5;
	}

	.stats .lab {
		font-size: 0.75rem;
	}

	.stats .num {
		font-size: 0.8125rem;
		font-variant-numeric: tabular-nums;
		text-align: right;
	}

	.stats .met {
		grid-column: 1;
		grid-row: 2;
		font-size: 0.625rem;
		color: var(--color-foreground-subtle);
	}
</style>
