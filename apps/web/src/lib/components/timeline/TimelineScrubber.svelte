<!--
	TimelineScrubber.svelte - the ruler of the day under the map (dayback/src/
	main.js:1221-1437, dayback/index.html:811-838): five fixed lanes on one
	folded time axis - Location, Calendar, Voice, Body - the nights
	folded to thin bands, and the playhead across them.

	Click or drag moves the playhead exactly there; a sideways two-finger
	swipe scrubs it; a pinch zooms the scale around it. A lane with no source
	connected asks for one, and opens where it is connected in a tab of its own.
-->
<script lang="ts">
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { bars, barHeight, barWidth, fitWords, waveform, type RibbonKind, type RibbonSpan, type ScrubConversation, type ScrubEvent } from '$lib/timeline/lanes';
	import { HOUR, MIN, inFold, ticks, unwarp, warp, type Fold, type Tier } from '$lib/timeline/scale';
	import { fmtDur } from '$lib/timeline/inspector';

	let {
		viewStart,
		viewEnd,
		dayStart,
		dayEnd,
		folds,
		playT,
		armed,
		playing,
		tier,
		zone,
		offset,
		ribbon,
		voice,
		conversations,
		nights,
		steps,
		stepScale,
		hasCalendar,
		calendar,
		onseek,
		onswipe,
		onribbon,
		onzoom,
		ontier,
		onplay,
		cardHeight = $bindable(0),
	}: {
		viewStart: number;
		viewEnd: number;
		/** The day the title, the map and the inspector show. */
		dayStart: number;
		dayEnd: number;
		folds: Fold[];
		playT: number;
		armed: boolean;
		playing: boolean;
		tier: Tier;
		zone: string;
		/** The day's offset east of UTC, ms: where the ruler's hours fall. */
		offset: number;
		ribbon: RibbonSpan[];
		/** Every window the mic recorded. */
		voice: { s: number; e: number }[];
		conversations: ScrubConversation[];
		nights: { s: number; e: number }[];
		steps: { t: number; v: number }[];
		stepScale: number;
		/** Null: not known (the lanes didn't load), so the lane draws nothing. */
		hasCalendar: boolean | null;
		calendar: ScrubEvent[];
		onseek: (t: number) => void;
		/** A sideways two-finger swipe to time `t`. */
		onswipe: (t: number) => void;
		/** A click (not a drag) on a stretch of the Location lane, at time `t`:
		 *  the map goes there as a pick in the inspector would take it. */
		onribbon: (r: { kind: RibbonKind; t: number }) => void;
		onzoom: (factor: number) => void;
		ontier: (t: Tier) => void;
		onplay: () => void;
		/** The card's height, for what sits above it and the map's framing. */
		cardHeight?: number;
	} = $props();

	/** Label gutter, right pad, the ruler's strip (main.js:1238). */
	const LG = 66;
	const PAD = 8;
	const TOP = 20;
	const LANES = [
		{ id: 'where', label: 'Location' },
		{ id: 'calendar', label: 'Calendar' },
		{ id: 'voice', label: 'Voice' },
		{ id: 'body', label: 'Body' },
	] as const;
	const TIERS: [Tier, string][] = [
		['week', 'Week'],
		['day', 'Day'],
		['hour', 'Hour'],
		['min', 'Minute'],
	];

	let card = $state<HTMLElement | null>(null);
	let svg = $state<SVGSVGElement | null>(null);
	let sw = $state(600);
	/** Expanded: taller rows with their labels (main.js:1257, 1424). */
	let expanded = $state(false);
	let scrubbing = false;
	/** Where a press began, and the Location stretch under it: a press that
	 *  ends where it began is a click on that stretch. */
	let pressed: { x: number; y: number; rib: RibbonSpan | null } | null = null;
	let hover = $state<{ cx: number; cy: number; title: string; meta: string; who: string } | null>(null);
	let peek = $state<HTMLElement | null>(null);
	let peekAt = $state({ left: 0, top: 0 });

	const rowH = $derived(expanded ? 46 : 24);
	const gap = $derived(expanded ? 9 : 6);
	const sh = $derived(TOP + LANES.length * rowH + (LANES.length - 1) * gap + 4);
	const x0 = LG;
	const x1 = $derived(sw - PAD);
	const tw = $derived(Math.max(1, x1 - x0));
	const wvs = $derived(warp(folds, viewStart));
	const wve = $derived(warp(folds, viewEnd));
	const SX = $derived((t: number) => x0 + ((warp(folds, t) - wvs) / (wve - wvs || 1)) * tw);
	const laneY = (i: number) => TOP + i * (rowH + gap);
	const lanesBot = $derived(laneY(LANES.length - 1) + rowH);
	const live = (s: number, e: number) => armed && playT >= s && playT < e;
	/** The part of [s, e) on screen, or null. */
	const span = (s: number, e: number) => {
		const a = Math.max(SX(s), x0);
		const b = Math.min(SX(e), x1);
		return b > a ? { a, b, w: b - a } : null;
	};

	const clock = (ms: number) => new Date(ms).toLocaleTimeString('en-US', { timeZone: zone, hour: 'numeric', minute: '2-digit' });

	// The ruler (main.js:1273-1279): labelled ticks, and short tallies between them.
	const ruler = $derived.by(() => {
		const ts = ticks(viewStart, viewEnd, tw, offset);
		const out: { x: number; label: string; tallies: number[] }[] = [];
		ts.forEach(([t, label], i) => {
			const next = ts[i + 1];
			const tallies: number[] = [];
			if (next) {
				const n = Math.max(1, Math.round((SX(next[0]) - SX(t)) / 14));
				for (let m = 1; m < n; m++) {
					const tm = t + ((next[0] - t) * m) / n;
					const pm = SX(tm);
					if (!inFold(folds, tm) && pm >= x0 && pm <= x1) tallies.push(pm);
				}
			}
			const x = SX(t);
			if (!inFold(folds, t) && x >= x0 - 2 && x <= x1) out.push({ x, label, tallies });
			else out.push({ x: NaN, label: '', tallies });
		});
		return out;
	});

	// On the Week span, the day the rest of the screen shows wears a pale band
	// across every lane, as Calendar shades today's column in a week (the
	// owner's call); it moves when the playhead walks into another day.
	const dayBand = $derived(viewEnd - viewStart > dayEnd - dayStart + MIN ? span(dayStart, dayEnd) : null);

	// The folded nights (main.js:1281-1287): one calm band, soft seams, a label.
	const foldBands = $derived(
		folds
			.map((f) => {
				const sp = span(f.a, f.b);
				if (!sp) return null;
				const dur = Math.max(1, Math.round((f.b - f.a) / HOUR));
				const est = f.est ? '~' : '';
				const label = sp.w <= 26 ? '' : sp.w > 88 ? `☾ Sleep · ${est}${dur}h` : sp.w > 46 ? `☾ ${est}${dur}h` : '☾';
				return { ...sp, est: f.est, label };
			})
			.filter((f) => f !== null),
	);

	// Location (main.js:1319-1333): blue at a place, orange on the move, grey
	// in a signal gap; a held span faint in the colour it holds.
	const ribbonRects = $derived(
		ribbon
			.map((r) => {
				const sp = span(r.s, r.e);
				if (!sp) return null;
				const colour = r.held ? (r.kind === 'place' ? 'var(--c-place)' : 'var(--c-move)') : r.kind === 'gap' ? 'var(--c-gap)' : r.kind === 'transit' ? 'var(--c-move)' : 'var(--c-place)';
				const label = expanded && r.kind === 'place' && !r.held && sp.w > 34 ? fitWords(r.title, Math.floor((sp.w - 16) / 6)) : '';
				return { ...sp, r, colour, label };
			})
			.filter((x) => x !== null),
	);

	// Voice (main.js:1334-1350): dashed = no audio, a solid line = the mic on,
	// a filled waveform = a conversation. Height claims nothing about volume.
	const voiceLines = $derived(voice.map((v) => span(v.s, v.e)).filter((x) => x !== null));
	const convShapes = $derived.by(() => {
		const i = LANES.findIndex((l) => l.id === 'voice');
		const h = rowH - 4;
		const cy = laneY(i) + 2 + h / 2;
		const maxA = Math.max(3, h / 2 - 3);
		return conversations
			.map((c, k) => {
				const sp = span(c.s, c.e);
				if (!sp) return null;
				return { ...sp, c, k, path: waveform(sp.a, sp.b, cy, maxA, c.s) };
			})
			.filter((x) => x !== null);
	});

	// Body (main.js:1354-1371): In Bed as the rest band, and a bar per step
	// reading - height the reading against the record's scale, colour its zone.
	const bodyBars = $derived.by(() => {
		const i = LANES.findIndex((l) => l.id === 'body');
		const h = rowH - 4;
		const base = laneY(i) + 2 + h - 2;
		const amp = Math.max(6, h - 8);
		const inView = steps.filter((b) => b.t >= viewStart - 30 * MIN && b.t < viewEnd + 30 * MIN);
		const bs = bars(inView, nights, stepScale);
		const width = barWidth(bs.map((b) => SX(b.t)));
		return {
			base,
			list: bs
				.map((b) => {
					const x = SX(b.t);
					if (x < x0 - 2 || x > x1 + 2) return null;
					const hh = barHeight(b.f, amp);
					return { t: b.t, x: Math.max(x0, Math.min(x1 - width, x - width / 2)), y: base - hh, h: hh, w: width, zone: b.zone };
				})
				.filter((x) => x !== null),
		};
	});
	/** Steps by pace, by weight of ink alone: the bin at the playhead is the
	 *  only one in the accent, since the accent means now. */
	const ZONE: Record<string, string> = {
		still: 'color-mix(in srgb, var(--c-body) 55%, transparent)',
		walking: 'var(--c-body)',
		active: 'var(--color-foreground)',
	};

	// Calendar: each timed event a chip with a left cap, the prototype's form
	// for an event (main.js:1225), drawn over its own share of any overlap (a
	// long event resumes after a shorter one inside it); an invitation you
	// haven't answered is faded. A chip names its event wherever the name
	// fits, at either height, on its widest piece (the owner's call: a bare
	// block reads as vague).
	const eventChips = $derived(
		calendar.flatMap((ev, k) => {
			const shown = ev.pieces.map(([ds, de]) => span(ds, de));
			let widest = -1;
			shown.forEach((sp, i) => {
				if (sp && (widest < 0 || sp.w > (shown[widest]?.w ?? 0))) widest = i;
			});
			return ev.pieces
				.map(([ds, de], pi) => {
					const sp = shown[pi];
					if (!sp) return null;
					const label = pi === widest ? fitWords(ev.title, Math.floor((sp.w - 14) / 6)) : '';
					return { ...sp, ev, k, ds, de, key: `${k}-${pi}`, label };
				})
				.filter((x) => x !== null);
		}),
	);

	/** An unconnected lane's pill (main.js:1372-1381), centred in the lane. */
	function pill(i: number, label: string) {
		const h = rowH - 4;
		const cy = laneY(i) + 2 + h / 2;
		const cx = (x0 + x1) / 2;
		const groupW = 15 + 8 + label.length * 6.3;
		const pw = Math.max(40, Math.min(tw - 16, groupW + 36));
		const gx = cx - groupW / 2;
		return { x: cx - pw / 2, y: cy - 27 / 2, w: pw, cy, gx, tx: gx + 23 };
	}
	const CONNECT = {
		calendar: { label: 'Connect Google Calendar', href: '/sources/google', name: 'Google' },
	} as const;

	const playX = $derived(SX(playT));
	const playOn = $derived(playX >= x0 - 1 && playX <= x1 + 1);

	/** The time under a pointer x, through the folds. */
	function timeAt(clientX: number): number {
		const r = svg!.getBoundingClientRect();
		const w = Math.max(1, r.width - LG - PAD);
		const u = Math.max(0, Math.min(1, (clientX - r.left - LG) / w));
		return unwarp(folds, wvs + u * (wve - wvs));
	}

	function down(e: PointerEvent) {
		const link = (e.target as Element).closest<SVGElement>('[data-connect]');
		if (link) {
			// In a tab of its own, so the Timeline stays where it was; a second
			// click goes back to that tab rather than opening another.
			const c = CONNECT[link.dataset.connect as keyof typeof CONNECT];
			const there = windowShellStore.findTab((t) => t.route === c.href);
			if (there) windowShellStore.setActiveTab(there.tab.id);
			else windowShellStore.openTabFromRoute(c.href, { label: c.name, forceNew: true });
			return;
		}
		scrubbing = true;
		hover = null;
		const rib = (e.target as Element).closest<SVGElement>('[data-rib]');
		pressed = { x: e.clientX, y: e.clientY, rib: rib ? ribbonRects[Number(rib.dataset.rib)].r : null };
		try {
			svg!.setPointerCapture(e.pointerId);
		} catch {
			// Capture is a nicety: without it the drag still follows inside the lanes.
		}
		onseek(timeAt(e.clientX));
	}
	function move(e: PointerEvent) {
		if (scrubbing) {
			onseek(timeAt(e.clientX));
			return;
		}
		// A light peek on hover (main.js:1413, 1431-1437): the conversation's
		// title, time and who was mentioned; a calendar event's title and time.
		const el = (e.target as Element).closest<SVGElement>('[data-conv], [data-event]');
		if (!el || !card) {
			hover = null;
			return;
		}
		const at = { cx: e.clientX, cy: e.clientY };
		if (el.dataset.conv !== undefined) {
			const c = conversations[Number(el.dataset.conv)];
			const who = [c.speakers ? `${c.speakers} speaker${c.speakers !== 1 ? 's' : ''}` : '', c.people.length ? `Mentioned ${c.people.slice(0, 4).join(', ')}` : '']
				.filter(Boolean)
				.join('  ·  ');
			hover = { ...at, title: c.title, meta: `${clock(c.s)} – ${clock(c.e)} · ${fmtDur(c.e - c.s)}`, who };
		} else {
			const ev = calendar[Number(el.dataset.event)];
			const who = [ev.unanswered ? 'Not answered yet' : '', ev.where ?? ''].filter(Boolean).join('  ·  ');
			// Scheduled: a calendar event is what was planned, never proof you were at it.
			hover = { ...at, title: ev.title, meta: `Scheduled ${clock(ev.s)} – ${clock(ev.e)} · ${fmtDur(ev.e - ev.s)}`, who };
		}
	}
	// The peek sits up and to the right of the pointer, flipping left at the
	// window's edge and below when there is no room above (main.js:1435).
	$effect(() => {
		if (!hover || !peek || !card) return;
		const cw = peek.offsetWidth;
		const ch = peek.offsetHeight;
		let px = hover.cx + 14;
		let py = hover.cy - ch - 12;
		if (px + cw > innerWidth - 8) px = hover.cx - cw - 14;
		if (px < 8) px = 8;
		if (py < 8) py = hover.cy + 18;
		const r = card.getBoundingClientRect();
		peekAt = { left: px - r.left, top: py - r.top };
	});
	function up(e: PointerEvent) {
		scrubbing = false;
		const p = pressed;
		pressed = null;
		if (p?.rib && Math.abs(e.clientX - p.x) + Math.abs(e.clientY - p.y) < 5) onribbon({ kind: p.rib.kind, t: timeAt(e.clientX) });
		try {
			svg?.releasePointerCapture(e.pointerId);
		} catch {
			// Nothing was captured.
		}
	}

	// A pinch zooms the scale around the playhead; a sideways two-finger swipe
	// scrubs it; a vertical one does nothing (main.js:1415-1421). Not passive,
	// so the page never scrolls under it.
	$effect(() => {
		const el = svg;
		if (!el) return;
		const wheel = (e: WheelEvent) => {
			e.preventDefault();
			if (e.ctrlKey) {
				onzoom(Math.exp(e.deltaY * 0.01));
				return;
			}
			if (Math.abs(e.deltaX) <= Math.abs(e.deltaY)) return;
			const w = Math.max(1, el.getBoundingClientRect().width - LG - PAD);
			onswipe(unwarp(folds, warp(folds, playT) + (e.deltaX / w) * (wve - wvs)));
		};
		el.addEventListener('wheel', wheel, { passive: false });
		return () => el.removeEventListener('wheel', wheel);
	});
</script>

<section class="scrub" class:exp={expanded} bind:this={card} bind:offsetHeight={cardHeight} aria-label="The day's streams">
	<div class="top">
		<button class="grab" aria-label={expanded ? 'Collapse the streams' : 'Expand the streams'} onclick={() => (expanded = !expanded)}>
			<svg viewBox="0 0 14 14" width="15" height="15" aria-hidden="true"><path d="M4 5.5 L7 8.5 L10 5.5" /></svg>
		</button>
	</div>
	<div class="ctl">
		<div class="lead">
			<button class="play" aria-label={playing ? 'Pause' : 'Play the day'} onclick={onplay}>{playing ? '❚❚' : '▶︎'}</button>
			<div class="tiers" role="tablist" aria-label="Span">
				{#each TIERS as [k, l] (k)}
					<button role="tab" aria-selected={tier === k} class:on={tier === k} onclick={() => ontier(k)}>{l}</button>
				{/each}
			</div>
		</div>
		<div class="clock"><b>{clock(playT)}</b></div>
	</div>
	<div class="svgbox" bind:clientWidth={sw}>
		<svg
			bind:this={svg}
			viewBox="0 0 {sw} {sh}"
			width={sw}
			height={sh}
			role="slider"
			tabindex="-1"
			aria-label="The day"
			aria-valuemin={viewStart}
			aria-valuemax={viewEnd}
			aria-valuenow={playT}
			onpointerdown={down}
			onpointermove={move}
			onpointerup={up}
			onpointercancel={up}
			onpointerleave={() => (hover = null)}
		>
			{#each ruler as tk, i (i)}
				{#if !Number.isNaN(tk.x)}
					<line class="tick" x1={tk.x} y1={TOP - 6} x2={tk.x} y2={TOP} />
					<text class="tick-label" x={tk.x + 4} y="11">{tk.label}</text>
				{/if}
				{#each tk.tallies as px, j (j)}
					<line class="tally" x1={px} y1={TOP - 3.5} x2={px} y2={TOP} />
				{/each}
			{/each}

			{#if dayBand}
				<rect class="day-band" x={dayBand.a} y={TOP - 4} width={dayBand.w} height={lanesBot - TOP + 6} rx="6" />
			{/if}

			{#each foldBands as f, i (i)}
				<rect class="fold" class:est={f.est} x={f.a} y={TOP} width={f.w} height={lanesBot - TOP} rx="4" />
				<line class="seam" x1={f.a} y1={TOP} x2={f.a} y2={lanesBot} />
				<line class="seam" x1={f.b} y1={TOP} x2={f.b} y2={lanesBot} />
				{#if f.label}
					<text class="fold-label" x={(f.a + f.b) / 2} y={(TOP + lanesBot) / 2}>{f.label}</text>
				{/if}
			{/each}

			{#each LANES as lane, i (lane.id)}
				{@const y = laneY(i)}
				<rect class="groove" x={x0} {y} width={tw} height={rowH} rx="5" />
				<text class="lane-label" x="2" y={y + rowH / 2 + 4}>{lane.label}</text>

				{#if lane.id === 'where'}
					{#each ribbonRects as rr, k (k)}
						{@const on = live(rr.r.s, rr.r.e)}
						<rect
							data-rib={rr.r.held ? undefined : k}
							x={rr.a}
							y={y + 2}
							width={Math.max(rr.w, 1)}
							height={rowH - 4}
							rx="5"
							fill={rr.colour}
							fill-opacity={on ? 0.9 : rr.r.held ? 0.1 : rr.r.kind === 'gap' ? 0.16 : 0.5}
						/>
						{#if on}<rect x={rr.a} y={y + 2} width="3" height={rowH - 4} fill={rr.colour} />{/if}
						{#if rr.label}<text class="seg-label" x={rr.a + 9} y={y + 2 + (rowH - 4) / 2 + 3.7}>{rr.label}</text>{/if}
					{/each}
				{:else if lane.id === 'voice'}
					{@const cy = y + 2 + (rowH - 4) / 2}
					<line class="mic-off" x1={x0} y1={cy} x2={x1} y2={cy} />
					{#each voiceLines as v, k (k)}
						<line class="mic-on" x1={v.a} y1={cy} x2={v.b} y2={cy} />
					{/each}
					{#each convShapes as cs (cs.k)}
						{@const on = live(cs.c.s, cs.c.e)}
						<line class="mic-on" x1={cs.a} y1={cy} x2={cs.b} y2={cy} />
						<rect class="chip" data-conv={cs.k} x={cs.a} y={y + 2} width={Math.max(cs.w, 2)} height={rowH - 4} rx="5" fill-opacity={on ? 0.15 : 0.06} />
						{#if cs.path}<path class="wave" d={cs.path} fill-opacity={on ? 0.66 : 0.46} />{/if}
						{#if on}<rect class="wave-edge" x={cs.a} y={y + 2} width="2.5" height={rowH - 4} rx="1.25" />{/if}
					{/each}
				{:else if lane.id === 'body'}
					{#each nights as n, k (k)}
						{@const sp = span(n.s, n.e)}
						{#if sp}<rect class="rest" x={sp.a} y={y + 2} width={sp.w} height={rowH - 4} rx="3" fill-opacity={live(n.s, n.e) ? 0.42 : 0.12} />{/if}
					{/each}
					<line class="axis" x1={x0} y1={bodyBars.base + 0.5} x2={x1} y2={bodyBars.base + 0.5} />
					{#each bodyBars.list as b (b.t)}
						<rect x={b.x} y={b.y} width={b.w} height={b.h} rx="1.2" fill={armed && Math.abs(playT - b.t) < 7.5 * MIN ? 'var(--c-sel)' : ZONE[b.zone]} />
					{/each}
				{:else if lane.id === 'calendar' && hasCalendar === true}
					{#each eventChips as ec (ec.key)}
						{@const on = live(ec.ds, ec.de)}
						<g class:unanswered={ec.ev.unanswered}>
							<rect class="event" data-event={ec.k} x={ec.a} y={y + 2} width={Math.max(ec.w, 2)} height={rowH - 4} rx="5" fill-opacity={on ? 0.24 : 0.12} />
							<rect class="event-cap" x={ec.a} y={y + 2} width="3" height={rowH - 4} rx="1.5" />
							{#if ec.label}<text class="seg-label" x={ec.a + 9} y={y + 2 + (rowH - 4) / 2 + 3.7}>{ec.label}</text>{/if}
						</g>
					{/each}
				{:else if lane.id === 'calendar' && hasCalendar === false}
					{@const c = CONNECT[lane.id]}
					{@const p = pill(i, c.label)}
					<g class="connect" data-connect={lane.id} role="link" aria-label={c.label}>
						<rect x={p.x} y={p.y} width={p.w} height="27" rx="13.5" />
						<rect class="glyph" x={p.gx} y={p.cy - 5.5} width="13" height="12" rx="2.5" />
						<line class="glyph" x1={p.gx} y1={p.cy - 2} x2={p.gx + 13} y2={p.cy - 2} />
						<line class="glyph" x1={p.gx + 3.5} y1={p.cy - 5.5} x2={p.gx + 3.5} y2={p.cy - 7.6} />
						<line class="glyph" x1={p.gx + 9.5} y1={p.cy - 5.5} x2={p.gx + 9.5} y2={p.cy - 7.6} />
						<text x={p.tx} y={p.cy + 4}>{c.label}</text>
					</g>
				{/if}
			{/each}

			{#if playOn}
				<line class="playhead" x1={playX} y1={TOP - 2} x2={playX} y2={lanesBot} />
				<path class="playhead-head" d="M{playX - 4} {TOP - 4} L{playX + 4} {TOP - 4} L{playX} {TOP + 1} Z" />
			{/if}
		</svg>
	</div>
	{#if hover}
		<div class="peek" bind:this={peek} style="left: {peekAt.left}px; top: {peekAt.top}px">
			<p class="peek-title">{hover.title}</p>
			<p class="peek-meta">{hover.meta}</p>
			{#if hover.who}<p class="peek-who">{hover.who}</p>{/if}
		</div>
	{/if}
</section>

<style>
	/* The card along the bottom, beside the inspector and never under it: the
	   page's surface in a hairline, like every card over the map. */
	.scrub {
		position: absolute;
		left: 16px;
		right: calc(min(384px, 42%) + 32px);
		bottom: 16px;
		z-index: 10;
		display: flex;
		flex-direction: column;
		padding: 4px 16px 12px;
		border-radius: var(--tile-radius);
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		font-family: var(--font-sans);
		color: var(--color-foreground);
	}
	:global(.timeline:not(.with-inspector)) .scrub {
		right: 16px;
	}
	.top {
		display: flex;
		justify-content: flex-end;
		height: 16px;
		margin: 0 4px 4px 0;
	}
	.grab {
		border: 0;
		background: none;
		padding: 0 8px;
		cursor: pointer;
		line-height: 0;
		color: var(--color-foreground-subtle);
	}
	.grab:hover {
		color: var(--color-foreground-muted);
	}
	.grab path {
		fill: none;
		stroke: currentColor;
		stroke-width: 1.6;
		stroke-linecap: round;
		stroke-linejoin: round;
	}
	/* Collapsed, the chevron points up: expand. */
	.grab svg {
		transform: rotate(180deg);
		transition: transform 0.2s;
	}
	.exp .grab svg {
		transform: rotate(0);
	}
	@media (prefers-reduced-motion: reduce) {
		.grab svg {
			transition: none;
		}
	}
	/* Play and the spans at the left, the clock in the middle. */
	.ctl {
		display: grid;
		grid-template-columns: 1fr auto 1fr;
		align-items: center;
		margin-bottom: 8px;
	}
	.lead {
		justify-self: start;
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.play {
		width: 28px;
		height: 28px;
		border: 0;
		border-radius: 50%;
		background: var(--color-foreground);
		color: var(--color-background);
		font-size: 11px;
		line-height: 1;
		/* design-ok: optical centring of the play glyph */
		padding: 0 0 0 2px;
		cursor: pointer;
		display: inline-flex;
		align-items: center;
		justify-content: center;
	}
	.play:hover {
		background: color-mix(in srgb, var(--color-foreground) 80%, var(--color-background));
	}
	/* The spans: words in the sans, the one picked in the accent - it is the
	   scale you are on now. */
	.tiers {
		display: inline-flex;
		gap: 4px;
	}
	.tiers button {
		border: 0;
		background: none;
		padding: 4px 8px;
		border-radius: 6px;
		font-family: var(--font-sans);
		font-size: 13px;
		font-weight: 500;
		color: var(--color-foreground-muted);
		cursor: pointer;
	}
	.tiers button:hover {
		background: var(--hover-bg);
		color: var(--color-foreground);
	}
	.tiers button.on {
		color: var(--color-primary);
	}
	.clock {
		justify-self: center;
		/* design-ok: a clock time (design-grammar §4) */
		font-family: var(--font-mono);
		font-size: 12px;
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}
	.clock b {
		font-weight: 400;
		color: var(--color-foreground);
	}
	.svgbox {
		width: 100%;
	}
	svg {
		display: block;
		cursor: pointer;
		touch-action: none;
		user-select: none;
	}
	/* The chart's own labels, set as Home's deck sets them: mono, small,
	   subtle, inside the SVG only (design-grammar §4). */
	.tick {
		stroke: var(--color-foreground);
		stroke-opacity: 0.2;
		stroke-width: 1;
	}
	.tick-label {
		/* design-ok: the chart's own hour ticks, inside the SVG (design-grammar §4) */
		font-family: var(--font-mono);
		/* design-ok: the chart's own hour ticks, inside the SVG (design-grammar §4) */
		font-size: 9px;
		letter-spacing: 0.06em;
		fill: var(--color-foreground-subtle);
	}
	.tally {
		stroke: var(--color-foreground);
		stroke-opacity: 0.12;
	}
	/* The day on screen, on the Week span: a pale column under every lane. */
	.day-band {
		fill: var(--color-foreground);
		fill-opacity: 0.06;
		pointer-events: none;
	}
	/* A folded night: one quiet fill, whisper seams, a label. */
	.fold {
		fill: var(--c-rest);
	}
	.fold.est {
		fill-opacity: 0.7;
	}
	.seam {
		stroke: var(--c-rest-ink);
		stroke-opacity: 0.4;
		stroke-dasharray: 2 4;
	}
	.fold-label {
		text-anchor: middle;
		dominant-baseline: middle;
		/* design-ok: the chart's own label, inside the SVG (design-grammar §4) */
		font-family: var(--font-mono);
		/* design-ok: the chart's own label, inside the SVG (design-grammar §4) */
		font-size: 9px;
		letter-spacing: 0.04em;
		fill: var(--c-rest-ink);
		paint-order: stroke;
		stroke: var(--color-surface);
		stroke-width: 3px;
		stroke-linejoin: round;
	}
	.groove {
		fill: var(--color-foreground);
		fill-opacity: 0.035;
	}
	.lane-label {
		/* design-ok: the chart's own lane names, inside the SVG (design-grammar §4) */
		font-family: var(--font-mono);
		/* design-ok: the chart's own lane names, inside the SVG (design-grammar §4) */
		font-size: 9.5px;
		letter-spacing: 0.04em;
		fill: var(--color-foreground-subtle);
	}
	.seg-label {
		font-family: var(--font-sans);
		font-size: 11px;
		font-weight: 500;
		fill: var(--color-foreground);
		paint-order: stroke;
		stroke: var(--color-surface);
		stroke-width: 2.6px;
		stroke-linejoin: round;
		pointer-events: none;
	}
	.mic-off {
		stroke: var(--c-voice);
		stroke-opacity: 0.5;
		stroke-width: 1;
		stroke-dasharray: 2 4;
		pointer-events: none;
	}
	.mic-on {
		stroke: var(--c-voice);
		stroke-width: 1.6;
		stroke-linecap: round;
		pointer-events: none;
	}
	.chip {
		fill: var(--c-voice);
	}
	.chip:hover,
	.event:hover {
		stroke: var(--color-foreground);
		stroke-opacity: 0.3;
		stroke-width: 1;
	}
	.wave,
	.wave-edge {
		fill: var(--c-voice);
		pointer-events: none;
	}
	.rest {
		fill: var(--c-rest);
	}
	.axis {
		stroke: var(--color-foreground);
		stroke-opacity: 0.09;
	}
	/* A calendar event: hollow, as an intention is not a trace - the outline
	   and a cap, never a fill of its own. */
	.event {
		fill: var(--c-calendar);
		stroke: var(--c-calendar);
		stroke-opacity: 0.6;
	}
	.event-cap {
		fill: var(--c-calendar);
		pointer-events: none;
	}
	/* An invitation you haven't answered: there, but faded. */
	.unanswered {
		opacity: 0.45;
	}
	/* Not connected: a quiet dashed pill whose words are a link, so they take
	   the accent - they are pressable. */
	.connect {
		cursor: pointer;
	}
	.connect > rect:first-child {
		fill: none;
		stroke: var(--color-foreground);
		stroke-opacity: 0.25;
		stroke-width: 1;
		stroke-dasharray: 4 4;
	}
	.connect:hover > rect:first-child {
		fill: var(--color-foreground);
		fill-opacity: 0.05;
	}
	.connect .glyph {
		fill: none;
		stroke: var(--color-primary);
		stroke-width: 1.3;
		stroke-linecap: round;
		stroke-linejoin: round;
		pointer-events: none;
	}
	.connect text {
		font-family: var(--font-sans);
		font-size: 12px;
		font-weight: 500;
		fill: var(--color-primary);
		pointer-events: none;
	}
	/* The playhead is now, so it is the accent. */
	.playhead {
		stroke: var(--c-sel);
		stroke-width: 1.5;
		pointer-events: none;
	}
	.playhead-head {
		fill: var(--c-sel);
		pointer-events: none;
	}
	/* The hover peek: title, time, who - lighter than the inspector, and it
	   never takes the click. */
	.peek {
		position: absolute;
		z-index: 12;
		/* Its own width, not what is left of the card to its right. */
		width: max-content;
		max-width: 288px;
		padding: 12px 16px;
		border-radius: 12px;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		pointer-events: none;
	}
	.peek p {
		margin: 0;
	}
	.peek-title {
		font-family: var(--font-serif);
		font-size: 18px;
		line-height: 1.25;
	}
	.peek-meta,
	.peek-who {
		margin-top: 4px !important;
		font-size: 13px;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
	}
</style>
