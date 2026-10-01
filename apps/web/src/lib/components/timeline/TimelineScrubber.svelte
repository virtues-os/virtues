<!--
	TimelineScrubber.svelte - the ruler of the day under the map (dayback/src/
	main.js:1221-1437, dayback/index.html:811-838): five fixed lanes on one
	folded time axis - Location, Calendar, Voice, Body, Finance - the nights
	folded to thin bands, and the playhead across them.

	Click or drag moves the playhead exactly there; a sideways two-finger
	swipe scrubs it; a pinch zooms the scale around it. A lane with no source
	connected asks for one, and takes you to where it is connected.
-->
<script lang="ts">
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { bars, barHeight, barWidth, waveform, type RibbonSpan, type ScrubConversation, type ScrubEvent } from '$lib/timeline/lanes';
	import { HOUR, MIN, inFold, ticks, unwarp, warp, type Fold, type Tier } from '$lib/timeline/scale';
	import { fmtDur } from '$lib/timeline/rail';

	let {
		viewStart,
		viewEnd,
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
		hasFinance,
		onseek,
		onzoom,
		ontier,
		onplay,
		cardHeight = $bindable(0),
	}: {
		viewStart: number;
		viewEnd: number;
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
		hasFinance: boolean | null;
		onseek: (t: number) => void;
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
		{ id: 'finance', label: 'Finance' },
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
				const maxc = Math.max(1, Math.floor((sp.w - 16) / 6));
				let label = '';
				if (expanded && r.kind === 'place' && !r.held && sp.w > 34) {
					if (r.title.length <= maxc) label = r.title;
					else {
						// Clip on a word; if not even the first word fits, no stub.
						let acc = '';
						for (const word of r.title.split(/\s+/)) {
							const next = acc ? `${acc} ${word}` : word;
							if (next.length <= maxc - 1) acc = next;
							else break;
						}
						label = acc ? `${acc}…` : '';
					}
				}
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
	const ZONE: Record<string, string> = {
		still: 'color-mix(in srgb, var(--c-body) 46%, var(--color-foreground-muted))',
		walking: 'var(--c-body)',
		active: 'color-mix(in srgb, var(--c-body) 70%, var(--c-sel))',
	};

	// Calendar: each timed event a chip with a left cap, the prototype's form
	// for an event (main.js:1225), drawn over its own share of any overlap (a
	// long event resumes after a shorter one inside it); an invitation you
	// haven't answered is faded.
	const eventChips = $derived(
		calendar.flatMap((ev, k) =>
			ev.pieces
				.map(([ds, de], pi) => {
					const sp = span(ds, de);
					if (!sp) return null;
					const maxc = Math.max(1, Math.floor((sp.w - 14) / 6));
					const label = expanded && pi === 0 && sp.w > 44 ? (ev.title.length > maxc ? `${ev.title.slice(0, maxc - 1).trimEnd()}…` : ev.title) : '';
					return { ...sp, ev, k, ds, de, key: `${k}-${pi}`, label };
				})
				.filter((x) => x !== null),
		),
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
		calendar: { label: 'Connect Google Calendar', href: '/sources/google', name: 'Google', colour: 'var(--c-calendar)' },
		finance: { label: 'Connect your finances', href: '/sources/plaid', name: 'Plaid', colour: 'var(--c-finance)' },
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
			const c = CONNECT[link.dataset.connect as keyof typeof CONNECT];
			windowShellStore.navigate(c.href, { label: c.name });
			return;
		}
		scrubbing = true;
		hover = null;
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
			hover = { ...at, title: ev.title, meta: `${clock(ev.s)} – ${clock(ev.e)} · ${fmtDur(ev.e - ev.s)}`, who };
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
			onseek(unwarp(folds, warp(folds, playT) + (e.deltaX / w) * (wve - wvs)));
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
							x={rr.a}
							y={y + 2}
							width={Math.max(rr.w, 1)}
							height={rowH - 4}
							rx="5"
							fill={rr.colour}
							fill-opacity={on ? 0.55 : rr.r.held ? 0.1 : rr.r.kind === 'gap' ? 0.16 : 0.24}
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
						<rect x={b.x} y={b.y} width={b.w} height={b.h} rx="1.2" fill={ZONE[b.zone]} fill-opacity={armed && Math.abs(playT - b.t) < 7.5 * MIN ? 1 : 0.9} />
					{/each}
				{:else if lane.id === 'calendar' && hasCalendar === true}
					{#each eventChips as ec (ec.key)}
						{@const on = live(ec.ds, ec.de)}
						<g class:unanswered={ec.ev.unanswered}>
							<rect class="event" data-event={ec.k} x={ec.a} y={y + 2} width={Math.max(ec.w, 2)} height={rowH - 4} rx="5" fill-opacity={on ? 0.24 : 0.12} />
							<rect class="event-cap" x={ec.a} y={y + 2} width="3" height={rowH - 4} rx="1.5" />
						</g>
						{#if ec.label}<text class="seg-label" x={ec.a + 9} y={y + 2 + (rowH - 4) / 2 + 3.7}>{ec.label}</text>{/if}
					{/each}
				{:else if (lane.id === 'calendar' && hasCalendar === false) || (lane.id === 'finance' && hasFinance === false)}
					{@const c = CONNECT[lane.id]}
					{@const p = pill(i, c.label)}
					<g class="connect" data-connect={lane.id} style="--cc: {c.colour}" role="link" aria-label={c.label}>
						<rect x={p.x} y={p.y} width={p.w} height="27" rx="13.5" />
						{#if lane.id === 'calendar'}
							<rect class="glyph" x={p.gx} y={p.cy - 5.5} width="13" height="12" rx="2.5" />
							<line class="glyph" x1={p.gx} y1={p.cy - 2} x2={p.gx + 13} y2={p.cy - 2} />
							<line class="glyph" x1={p.gx + 3.5} y1={p.cy - 5.5} x2={p.gx + 3.5} y2={p.cy - 7.6} />
							<line class="glyph" x1={p.gx + 9.5} y1={p.cy - 5.5} x2={p.gx + 9.5} y2={p.cy - 7.6} />
						{:else}
							<rect class="glyph" x={p.gx} y={p.cy - 5} width="15" height="10" rx="2" />
							<line class="glyph stripe" x1={p.gx} y1={p.cy - 1.5} x2={p.gx + 15} y2={p.cy - 1.5} />
						{/if}
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
	/* The card along the bottom: beside the rail, never under it (index.html:
	   812, 844). */
	.scrub {
		position: absolute;
		left: 16px;
		right: calc(min(384px, 42%) + 32px);
		bottom: 16px;
		z-index: 10;
		display: flex;
		flex-direction: column;
		/* design-ok: the Dayback prototype's scrubber padding (owner's call, 2026-09-30) */
		padding: 5px 15px 12px;
		/* The Timeline's one material (TimelineView's tile variables). */
		border-radius: var(--tile-radius);
		background: var(--tile-bg);
		border: var(--tile-border);
		/* design-ok: the Timeline follows the Dayback prototype's look (owner's call, 2026-09-30) */
		box-shadow: var(--tile-shadow);
		font-family: var(--font-sans);
		color: var(--color-foreground);
	}
	:global(.timeline:not(.with-rail)) .scrub {
		right: 16px;
	}
	.top {
		display: flex;
		justify-content: flex-end;
		height: 16px;
		/* design-ok: the Dayback prototype's chevron row (owner's call, 2026-09-30) */
		margin: -1px 4px 2px 0;
	}
	.grab {
		border: 0;
		background: none;
		/* design-ok: the Dayback prototype's chevron (owner's call, 2026-09-30) */
		padding: 2px 7px;
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
	.ctl {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 8px;
	}
	.lead {
		display: flex;
		align-items: center;
		gap: 10px;
	}
	.play {
		width: 27px;
		height: 27px;
		border: 0;
		border-radius: 50%;
		background: var(--color-foreground);
		color: var(--color-background);
		font-size: 11px;
		line-height: 1;
		/* design-ok: optical centring of the play glyph (prototype) */
		padding: 0 0 0 2px;
		cursor: pointer;
		display: inline-flex;
		align-items: center;
		justify-content: center;
	}
	.play:hover {
		opacity: 0.6;
	}
	.tiers {
		display: inline-flex;
		/* design-ok: the Dayback prototype's segmented control (owner's call, 2026-09-30) */
		padding: 2px;
		border-radius: 8px;
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
	}
	.tiers button {
		border: 0;
		background: none;
		/* design-ok: the Dayback prototype's segment (owner's call, 2026-09-30) */
		padding: 5px 10px;
		border-radius: 6px;
		font-family: var(--font-sans);
		font-size: 11px;
		font-weight: 600;
		letter-spacing: 0.02em;
		text-transform: uppercase;
		color: var(--color-foreground-muted);
		cursor: pointer;
	}
	.tiers button:hover {
		color: var(--color-foreground);
	}
	.tiers button.on {
		background: var(--color-foreground);
		color: var(--color-background);
	}
	.clock {
		font-size: 14px;
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}
	.clock b {
		font-weight: 700;
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
	.tick {
		stroke: var(--color-foreground-muted);
		stroke-width: 1;
	}
	.tick-label {
		font-family: var(--font-sans);
		/* design-ok: the Dayback prototype's ruler labels, drawn in SVG under the lanes (owner's call, 2026-09-30) */
		font-size: 10.5px;
		font-weight: 590;
		letter-spacing: 0.01em;
		fill: color-mix(in srgb, var(--color-foreground) 74%, var(--color-background));
	}
	.tally {
		stroke: color-mix(in srgb, var(--color-foreground) 15%, transparent);
	}
	/* A folded night: one quiet fill, whisper seams, a label (main.js:1281-1287). */
	.fold {
		fill: var(--c-rest);
		fill-opacity: 0.1;
	}
	.fold.est {
		fill-opacity: 0.07;
	}
	.seam {
		stroke: var(--c-rest);
		stroke-opacity: 0.26;
		stroke-dasharray: 2 4;
	}
	.fold-label {
		text-anchor: middle;
		dominant-baseline: middle;
		font-family: var(--font-sans);
		font-size: 11px;
		font-weight: 600;
		letter-spacing: 0.01em;
		fill: var(--c-rest-ink);
		paint-order: stroke;
		stroke: var(--color-background);
		stroke-width: 3px;
		stroke-linejoin: round;
	}
	.groove {
		fill: var(--color-foreground);
		fill-opacity: 0.035;
	}
	.lane-label {
		font-family: var(--font-sans);
		font-size: 12px;
		font-weight: 600;
		letter-spacing: 0.01em;
		fill: color-mix(in srgb, var(--color-foreground) 74%, var(--color-background));
	}
	.seg-label {
		font-family: var(--font-sans);
		font-size: 11px;
		font-weight: 600;
		letter-spacing: -0.005em;
		fill: var(--color-foreground);
		paint-order: stroke;
		stroke: var(--color-background);
		stroke-width: 2.6px;
		stroke-linejoin: round;
		pointer-events: none;
	}
	.mic-off {
		stroke: var(--c-voice);
		stroke-opacity: 0.46;
		stroke-width: 1;
		stroke-dasharray: 2 4;
		pointer-events: none;
	}
	.mic-on {
		stroke: var(--c-voice);
		stroke-opacity: 0.55;
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
		stroke-opacity: 0.26;
		stroke-width: 1;
	}
	.wave {
		fill: var(--c-voice);
		pointer-events: none;
	}
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
	.event {
		fill: var(--c-calendar);
		stroke: var(--c-calendar);
		stroke-opacity: 0.5;
	}
	.event-cap {
		fill: var(--c-calendar);
		fill-opacity: 0.9;
		pointer-events: none;
	}
	/* An invitation you haven't answered: there, but faded. */
	.unanswered {
		opacity: 0.45;
	}
	/* Not connected: a dashed pill in the lane's colour (main.js:1376-1378). */
	.connect {
		cursor: pointer;
	}
	.connect > rect:first-child {
		fill: var(--cc);
		fill-opacity: 0.07;
		stroke: var(--cc);
		stroke-opacity: 0.34;
		stroke-width: 1;
		stroke-dasharray: 5 4;
	}
	.connect:hover > rect:first-child {
		fill-opacity: 0.13;
		stroke-opacity: 0.55;
	}
	.connect .glyph {
		fill: none;
		stroke: var(--cc);
		stroke-width: 1.3;
		stroke-linecap: round;
		stroke-linejoin: round;
		pointer-events: none;
	}
	.connect .glyph.stripe {
		stroke-width: 2.2;
	}
	.connect text {
		font-family: var(--font-sans);
		/* design-ok: the Dayback prototype's connect pill (owner's call, 2026-09-30) */
		font-size: 11.5px;
		font-weight: 600;
		letter-spacing: 0.01em;
		fill: var(--cc);
		pointer-events: none;
	}
	.playhead {
		stroke: var(--color-foreground);
		stroke-width: 1.25;
		pointer-events: none;
	}
	.playhead-head {
		fill: var(--color-foreground);
		pointer-events: none;
	}
	/* The hover peek: title, time, who - lighter than the rail, and it never
	   takes the click (index.html:908-911). */
	.peek {
		position: absolute;
		z-index: 12;
		/* Its own width, not what is left of the card to its right. */
		width: max-content;
		max-width: 290px;
		/* design-ok: the Dayback prototype's peek padding (owner's call, 2026-09-30) */
		padding: 11px 14px;
		border-radius: 12px;
		background: var(--color-background);
		border: 1px solid color-mix(in srgb, var(--color-foreground) 15%, transparent);
		/* design-ok: the Timeline follows the Dayback prototype's look (owner's call, 2026-09-30) */
		box-shadow: var(--peek-shadow);
		pointer-events: none;
	}
	.peek p {
		margin: 0;
	}
	.peek-title {
		font-size: 14px;
		font-weight: 600;
		letter-spacing: -0.01em;
		line-height: 1.25;
	}
	.peek-meta {
		font-size: 12px;
		color: var(--color-foreground-muted);
		margin-top: 4px !important;
		font-variant-numeric: tabular-nums;
	}
	.peek-who {
		font-size: 12px;
		color: var(--color-foreground-muted);
		/* design-ok: the Dayback prototype's peek spacing (owner's call, 2026-09-30) */
		margin-top: 6px !important;
		line-height: 1.4;
	}
</style>
