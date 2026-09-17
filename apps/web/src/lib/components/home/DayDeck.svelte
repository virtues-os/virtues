<!--
	DayDeck.svelte — today as a multitrack recording.

	Nine tracks, midnight to midnight, drawn from the raw record. The semantic
	dayline (labelled events, novelty, the prose) cannot exist for today — the
	box composes it at ~4am the next morning, and the server refuses to fuse a
	day that is not over. So this is deliberately the layer underneath: what the
	collectors actually captured, while they are still capturing it.

	Two things a deck must be honest about, which a plain chart is not:

	  · a track with no source is drawn, not hidden. An empty lane and a lane
	    nothing writes to look identical otherwise, and the second one is the
	    one you can fix. Arm state comes from /api/streams/health.
	  · the day is not over. The deck's one axis runs solid up to now and dashed
	    after it, and the open location clip — the place you are still in —
	    carries a live right edge instead of a squared-off one.

	Colour carries one meaning only: now. Everything else separates by form and
	weight, so a lane never implies a category that isn't real.

	Geometry is day-fraction (0…1) throughout; `px()` is the only place that
	becomes a coordinate. Day length is measured, not assumed, so the 23- and
	25-hour days land on the right ticks.
-->
<script lang="ts">
	import type { TodayStreamsView, DayHeartRateSample, LifelineData, TimelineDayPoint } from "$lib/wiki/api";
	import type { StreamHealth } from "$lib/api/client";

	interface SleepCycle {
		start_time: string;
		end_time: string;
		dominant_stage: string;
	}

	interface Props {
		dayStartMs: number;
		dayEndMs: number;
		nowMs: number;
		streams: TodayStreamsView | null;
		heart: DayHeartRateSample[];
		life: LifelineData | null;
		points: TimelineDayPoint[];
		sleepCycles: SleepCycle[];
		/** Registry name → health row, for arm state. */
		health: Record<string, StreamHealth>;
		/** Instant under the pointer, or null. Bound so the map can follow it. */
		scrubMs?: number | null;
		/** Instant the reader clicked, which outlives the pointer leaving. The
		 *  page answers it by fetching the records that actually fall there. */
		pinnedMs?: number | null;
	}

	let {
		dayStartMs,
		dayEndMs,
		nowMs,
		streams,
		heart,
		life,
		points,
		sleepCycles,
		health,
		scrubMs = $bindable(null),
		pinnedMs = $bindable(null),
	}: Props = $props();

	/** The pointer wins while it is over the deck; the pin holds when it isn't. */
	const activeMs = $derived(scrubMs ?? pinnedMs);

	const reduce =
		typeof window !== "undefined" &&
		window.matchMedia &&
		window.matchMedia("(prefers-reduced-motion: reduce)").matches;

	// ── the axis ────────────────────────────────────────────────
	const dayMs = $derived(Math.max(1, dayEndMs - dayStartMs));
	/** Where an instant falls in the day, 0…1, clamped. */
	function f(ms: number): number {
		return Math.min(1, Math.max(0, (ms - dayStartMs) / dayMs));
	}
	function ms(frac: number): number {
		return dayStartMs + frac * dayMs;
	}
	const nowF = $derived(f(nowMs));

	// ── layout ──────────────────────────────────────────────────
	const GUTTER = 62;
	const PAD_R = 8;
	const RULER_H = 18;
	const LANE_GAP = 7;
	const GROUP_GAP = 15;

	let width = $state(760);
	const plotW = $derived(Math.max(200, width - GUTTER - PAD_R));
	function px(frac: number): number {
		return GUTTER + frac * plotW;
	}

	const HOURS = [0, 3, 6, 9, 12, 15, 18, 21, 24];
	function hourLabel(h: number): string {
		if (h === 0 || h === 24) return "12a";
		if (h === 12) return "12p";
		return h < 12 ? `${h}a` : `${h - 12}p`;
	}

	// ── track shapes ────────────────────────────────────────────
	type Clip = { x1: number; x2: number; label?: string; weight: number; open?: boolean };
	type Track =
		| { id: string; name: string; stream: string; h: number; group: number; kind: "clips"; clips: Clip[] }
		| { id: string; name: string; stream: string; h: number; group: number; kind: "bars"; vals: number[] }
		| { id: string; name: string; stream: string; h: number; group: number; kind: "area"; vals: number[] }
		| { id: string; name: string; stream: string; h: number; group: number; kind: "line"; segs: [number, number][][]; lo: number; hi: number };

	/** A clip narrower than a hairline still happened — give it one pixel. */
	const MIN_W = 0.0012;

	function clip(startIso: string, endIso: string | null, label: string | undefined, weight: number, open = false): Clip | null {
		const s = Date.parse(startIso);
		if (isNaN(s)) return null;
		let e = endIso ? Date.parse(endIso) : s;
		if (isNaN(e) || e < s) e = s;
		const x1 = f(s);
		const x2 = Math.max(f(e), x1 + MIN_W);
		if (x2 <= 0 || x1 >= 1) return null;
		return { x1, x2: Math.min(1, x2), label, weight, open };
	}

	// sleep — last night's scored cycles, which start before this midnight.
	const sleepClips = $derived.by<Clip[]>(() =>
		sleepCycles
			.map((c) => clip(c.start_time, c.end_time, undefined, c.dominant_stage === "deep" ? 0.13 : 0.07))
			.filter((c): c is Clip => c !== null),
	);

	// places — an open visit comes back zero-width (departure_time is null).
	// It is the clip that matters most: it is where you are.
	const placeClips = $derived.by<Clip[]>(() => {
		const rows = streams?.location ?? [];
		return rows
			.map((r) => {
				const open = !r.end_time || Date.parse(r.end_time) <= Date.parse(r.start_time);
				return clip(r.start_time, open ? new Date(nowMs).toISOString() : r.end_time, r.place_name ?? "unnamed", 0.1, open);
			})
			.filter((c): c is Clip => c !== null);
	});

	const calClips = $derived.by<Clip[]>(() =>
		(streams?.calendar ?? [])
			.filter((e) => !e.is_all_day)
			.map((e) => clip(e.start_time, e.end_time, e.title, 0.09))
			.filter((c): c is Clip => c !== null),
	);

	// mic — 5-minute chunks, merged where they touch. Silence is drawn, not
	// dropped: a quiet hour with the mic open is not the same as no mic.
	const micClips = $derived.by<Clip[]>(() => {
		const rows = [...(streams?.audio ?? [])].sort((a, b) => Date.parse(a.start_time) - Date.parse(b.start_time));
		const out: Clip[] = [];
		for (const r of rows) {
			const c = clip(r.start_time, r.end_time, undefined, r.is_silent ? 0.05 : 0.15);
			if (!c) continue;
			const prev = out[out.length - 1];
			// 90s of slack absorbs the gap between consecutive chunk writes.
			if (prev && prev.weight === c.weight && c.x1 - prev.x2 < 90_000 / dayMs) prev.x2 = c.x2;
			else out.push(c);
		}
		return out;
	});

	// heart — dots joined by a line, never a smoothed curve, and broken where
	// the watch was off. A curve across a two-hour gap invents a heartbeat.
	const HEART_GAP_MS = 25 * 60_000;
	const heartTrack = $derived.by(() => {
		const pts = heart
			.map((s) => [Date.parse(s.timestamp), s.bpm] as [number, number])
			.filter(([t, b]) => !isNaN(t) && b > 0 && t >= dayStartMs && t <= dayEndMs)
			.sort((a, b) => a[0] - b[0]);
		if (!pts.length) return { segs: [] as [number, number][][], lo: 0, hi: 1 };
		let lo = Infinity,
			hi = -Infinity;
		for (const [, b] of pts) {
			lo = Math.min(lo, b);
			hi = Math.max(hi, b);
		}
		const segs: [number, number][][] = [];
		let cur: [number, number][] = [];
		for (const p of pts) {
			if (cur.length && p[0] - cur[cur.length - 1][0] > HEART_GAP_MS) {
				segs.push(cur);
				cur = [];
			}
			cur.push(p);
		}
		if (cur.length) segs.push(cur);
		return { segs, lo: lo - 2, hi: Math.max(hi + 2, lo + 6) };
	});

	/** One lifeline lane's 96 buckets, scaled 0…1 against its own peak. */
	function bucketsOf(laneId: string): number[] {
		const lane = life?.lanes?.find((l) => l.id === laneId);
		if (!lane?.density?.length) return [];
		const peak = lane.peak || Math.max(...lane.density, 1);
		return lane.density.map((v) => (peak > 0 ? Math.min(1, v / peak) : 0));
	}
	/** The same buckets unscaled, for counting rather than drawing. */
	function rawOf(laneId: string): number[] {
		return life?.lanes?.find((l) => l.id === laneId)?.density ?? [];
	}
	const rawBuckets = $derived(rawOf("health"));
	const rawScreen = $derived(rawOf("activity"));
	const rawMsgs = $derived(rawOf("communication"));
	const sum = (xs: number[]) => xs.reduce((a, b) => a + b, 0);

	// movement — speed between fixes, bucketed. Reads as the day's transit:
	// flat where you sat, spiked where you moved. Normalised against the 90th
	// percentile so one GPS jump does not flatten the rest of the day.
	const moveVals = $derived.by(() => {
		const N = 96;
		if (points.length < 2) return [];
		const bins = new Array(N).fill(0);
		for (let i = 1; i < points.length; i++) {
			const a = points[i - 1],
				b = points[i];
			const t0 = Date.parse(a.timestamp),
				t1 = Date.parse(b.timestamp);
			const dt = (t1 - t0) / 1000;
			if (!(dt > 1) || dt > 900) continue;
			const dLat = (b.latitude - a.latitude) * 111_320;
			const dLon = (b.longitude - a.longitude) * 111_320 * Math.cos((a.latitude * Math.PI) / 180);
			const speed = Math.hypot(dLat, dLon) / dt;
			const idx = Math.min(N - 1, Math.max(0, Math.floor(f(t1) * N)));
			bins[idx] = Math.max(bins[idx], speed);
		}
		const nz = bins.filter((v) => v > 0).sort((a, b) => a - b);
		if (!nz.length) return [];
		const p90 = nz[Math.min(nz.length - 1, Math.floor(nz.length * 0.9))] || 1;
		return bins.map((v) => Math.min(1, v / p90));
	});

	const tracks = $derived.by<Track[]>(() => [
		{ id: "sleep", name: "sleep", stream: "health_sleep", h: 11, group: 0, kind: "clips", clips: sleepClips },
		{ id: "heart", name: "heart", stream: "health_heart_rate", h: 26, group: 0, kind: "line", segs: heartTrack.segs, lo: heartTrack.lo, hi: heartTrack.hi },
		{ id: "steps", name: "steps", stream: "health_steps", h: 17, group: 0, kind: "bars", vals: bucketsOf("health") },
		{ id: "place", name: "place", stream: "location_visit", h: 19, group: 1, kind: "clips", clips: placeClips },
		{ id: "move", name: "move", stream: "location_point", h: 15, group: 1, kind: "area", vals: moveVals },
		{ id: "cal", name: "calendar", stream: "calendar_event", h: 19, group: 1, kind: "clips", clips: calClips },
		{ id: "mic", name: "mic", stream: "audio_session", h: 11, group: 2, kind: "clips", clips: micClips },
		{ id: "screen", name: "screen", stream: "activity_app_session", h: 17, group: 2, kind: "bars", vals: bucketsOf("activity") },
		{ id: "msgs", name: "messages", stream: "communication_message", h: 11, group: 2, kind: "bars", vals: bucketsOf("communication") },
	]);

	// ── why a track is empty ────────────────────────────────────
	// An empty track has three quite different meanings and they must not look
	// alike: nothing writes this stream, the stream writes but has not reported
	// since some earlier moment, or it is reporting fine and today simply holds
	// none of this. The middle case is the common one — a phone that has not synced
	// since bedtime leaves five tracks blank — and drawing it as a flat zero
	// says "you did nothing", which is a lie about the person rather than a
	// fact about the box.
	// `blocked` is the fourth case, and the one this taxonomy was missing: the
	// stream has a writer, the writer is failing, and the lane is empty BECAUSE
	// of that. It used to fall through to `silent` — "last 5:22pm yesterday" —
	// which describes the symptom while withholding the cause the box already
	// knew. A named cause is the difference between a lane you wonder about and
	// one you can fix.
	type Cover =
		| { kind: "ok" }
		| { kind: "never" }
		| { kind: "silent"; since: number }
		| { kind: "blocked"; why: string };

	function hasToday(t: Track): boolean {
		if (t.kind === "clips") return t.clips.length > 0;
		if (t.kind === "line") return t.segs.length > 0;
		return t.vals.some((v) => v > 0);
	}

	function coverOf(t: Track): Cover {
		if (hasToday(t)) return { kind: "ok" };
		const hh = health[t.stream];
		// Health not loaded yet: claim nothing rather than guess.
		if (!hh) return { kind: "ok" };
		if (hh.status === "blocked")
			return { kind: "blocked", why: hh.blocked_reason || "the source is blocked" };
		if (hh.status === "never" || hh.total === 0) return { kind: "never" };
		const last = hh.last_event ? Date.parse(hh.last_event) : NaN;
		// Reported today but this lane is empty — the stream is healthy and the
		// measure is genuinely zero. Saying "nothing since 5:22pm" would be a
		// non-sequitur about a stream that is working.
		if (isNaN(last) || last >= dayStartMs) return { kind: "ok" };
		return { kind: "silent", since: last };
	}

	// Terse on purpose. Six dark tracks each spelling out a full sentence reads
	// as clutter and buries the one thing that matters — when it last spoke.
	function sinceLabel(t: number): string {
		const d = new Date(t);
		const midnightOf = new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
		const days = Math.round((dayStartMs - midnightOf) / 86_400_000);
		const ap = d.getHours() >= 12 ? "pm" : "am";
		const time = `${d.getHours() % 12 || 12}:${String(d.getMinutes()).padStart(2, "0")}${ap}`;
		if (days <= 1) return `last ${time} yesterday`;
		if (days < 7) return `last ${d.toLocaleDateString(undefined, { weekday: "long" })}`;
		return `last ${d.toLocaleDateString(undefined, { month: "short", day: "numeric" })}`;
	}

	/** y offset per track, with a wider gap where the kind of record changes. */
	const laid = $derived.by(() => {
		let y = RULER_H;
		const out: Array<Track & { y: number; cover: Cover }> = [];
		let prevGroup = tracks[0]?.group ?? 0;
		for (const t of tracks) {
			if (t.group !== prevGroup) {
				y += GROUP_GAP - LANE_GAP;
				prevGroup = t.group;
			}
			out.push({ ...t, y, cover: coverOf(t) });
			y += t.h + LANE_GAP;
		}
		return out;
	});
	const deckH = $derived((laid[laid.length - 1]?.y ?? RULER_H) + (laid[laid.length - 1]?.h ?? 0) + 4);

	function barGeom(vals: number[]) {
		const w = plotW / vals.length;
		return { w, bw: Math.max(1, w - 1.6) };
	}
	function areaPath(vals: number[], y: number, h: number): string {
		const w = plotW / vals.length;
		let d = `M ${GUTTER} ${y + h}`;
		vals.forEach((v, i) => {
			const x = GUTTER + i * w;
			d += ` L ${x.toFixed(1)} ${(y + h - v * h).toFixed(1)} L ${(x + w).toFixed(1)} ${(y + h - v * h).toFixed(1)}`;
		});
		return d + ` L ${GUTTER + plotW} ${y + h} Z`;
	}
	function linePath(seg: [number, number][], y: number, h: number, lo: number, hi: number): string {
		const span = Math.max(1, hi - lo);
		return seg
			.map(([t, v], i) => `${i ? "L" : "M"} ${px(f(t)).toFixed(1)} ${(y + h - ((v - lo) / span) * h).toFixed(1)}`)
			.join(" ");
	}

	// ── scrubbing ───────────────────────────────────────────────
	// A scrubber is a slider over the day in minutes, so it is one: arrow keys
	// step it and the readout is its value text. Reading the day back is the
	// whole point of the deck, and it should not need a pointer.
	let plotEl = $state<HTMLDivElement | undefined>(undefined);
	function instantAt(clientX: number): number | null {
		const el = plotEl;
		if (!el) return null;
		const r = el.getBoundingClientRect();
		const frac = (clientX - r.left - GUTTER) / plotW;
		return frac < -0.02 || frac > 1.02 ? null : ms(Math.min(1, Math.max(0, frac)));
	}
	function onMove(e: PointerEvent) {
		scrubMs = instantAt(e.clientX);
	}
	/** Clicking the same instant twice puts it away again. */
	function onClick(e: MouseEvent) {
		const t = instantAt(e.clientX);
		if (t == null) return;
		pinnedMs = pinnedMs != null && Math.abs(pinnedMs - t) < 4 * 60_000 ? null : t;
	}

	const STEP = 15 * 60_000;
	function onKey(e: KeyboardEvent) {
		const cur = activeMs ?? nowMs;
		if (e.key === "Enter" || e.key === " ") {
			e.preventDefault();
			pinnedMs = pinnedMs == null ? cur : null;
			return;
		}
		const jump = e.shiftKey ? 4 * STEP : STEP;
		let next: number | null = null;
		if (e.key === "ArrowLeft") next = cur - jump;
		else if (e.key === "ArrowRight") next = cur + jump;
		else if (e.key === "Home") next = dayStartMs;
		else if (e.key === "End") next = nowMs;
		else if (e.key === "Escape") {
			scrubMs = null;
			pinnedMs = null;
			return;
		} else return;
		e.preventDefault();
		scrubMs = Math.min(dayEndMs, Math.max(dayStartMs, next));
	}
	const scrubMinute = $derived(Math.round((((activeMs ?? nowMs) - dayStartMs) / dayMs) * 1440));

	function clockOf(t: number): string {
		const d = new Date(t);
		const ap = d.getHours() >= 12 ? "pm" : "am";
		return `${d.getHours() % 12 || 12}:${String(d.getMinutes()).padStart(2, "0")} ${ap}`;
	}
	function dur(minutes: number): string {
		const h = Math.floor(minutes / 60),
			m = Math.round(minutes % 60);
		return h ? `${h}h ${m}m` : `${m}m`;
	}
	const int = new Intl.NumberFormat();

	function clipAt(clips: Clip[], frac: number): Clip | undefined {
		return clips.find((c) => frac >= c.x1 && frac <= c.x2);
	}

	/** What the record says about one instant — the scrub readout. */
	const atCursor = $derived.by(() => {
		const at = activeMs;
		if (at == null) return null;
		const frac = f(at);
		const parts: string[] = [];
		const sl = clipAt(sleepClips, frac);
		if (sl) parts.push("asleep");
		const pl = clipAt(placeClips, frac);
		if (pl?.label) parts.push(pl.label);
		const ev = clipAt(calClips, frac);
		if (ev?.label) parts.push(ev.label);
		const mc = clipAt(micClips, frac);
		if (mc) parts.push(mc.weight > 0.1 ? "mic open" : "mic quiet");
		let best: DayHeartRateSample | null = null;
		let bestGap = 10 * 60_000;
		for (const s of heart) {
			const g = Math.abs(Date.parse(s.timestamp) - at);
			if (g < bestGap) {
				bestGap = g;
				best = s;
			}
		}
		if (best) parts.push(`${Math.round(best.bpm)} bpm`);
		const raw = rawBuckets;
		if (raw.length) {
			const v = raw[Math.min(raw.length - 1, Math.floor(frac * raw.length))];
			if (v > 0) parts.push(`${int.format(Math.round(v))} steps`);
		}
		if (at > nowMs) return { time: clockOf(at), text: "not yet" };
		return { time: clockOf(at), text: parts.length ? parts.join(" · ") : "nothing recorded" };
	});

	/** The resting caption: the day so far, in totals. */
	const summary = $derived.by(() => {
		const parts: string[] = [];
		const places = new Set((streams?.location ?? []).map((r) => r.place_name).filter(Boolean));
		const visits = streams?.location?.length ?? 0;
		if (visits) parts.push(places.size > 1 ? `${places.size} places` : visits === 1 ? "one place" : `${visits} stops`);
		const micMin = micClips.reduce((a, c) => a + ((c.x2 - c.x1) * dayMs) / 60_000, 0);
		if (micMin > 1) parts.push(`mic ${dur(micMin)}`);
		const steps = sum(rawBuckets);
		if (steps > 0) parts.push(`${int.format(Math.round(steps))} steps`);
		// Screen and messages are the densest streams a desk-bound day has; a
		// summary that omits them describes someone else's day.
		const screenH = sum(rawScreen);
		if (screenH > 0.05) parts.push(`${dur(screenH * 60)} at a screen`);
		const msgs = sum(rawMsgs);
		if (msgs > 0) parts.push(`${int.format(Math.round(msgs))} sent`);
		const open = placeClips.some((c) => c.open);
		parts.push(open ? "still recording" : `${Math.round(nowF * 24)}h in`);
		return parts.join(" · ");
	});

	// Every track counts, not just the span-shaped ones. Screen and messages
	// alone are a full day's evidence on a box whose phone has not synced.
	const anyData = $derived(laid.some(hasToday));
	const nSilent = $derived(laid.filter((t) => t.cover.kind === "silent").length);
	const nNever = $derived(laid.filter((t) => t.cover.kind === "never").length);
	const nBlocked = $derived(laid.filter((t) => t.cover.kind === "blocked").length);
	const coverageNote = $derived.by(() => {
		const bits: string[] = [];
		// Blocked leads: it is the only one of the three the reader can act on.
		if (nBlocked) bits.push(`${nBlocked} blocked`);
		if (nSilent) bits.push(`${nSilent} silent today`);
		if (nNever) bits.push(`${nNever} with no source`);
		return bits.length ? bits.join(" · ") : null;
	});
</script>

<figure class="deck">
	<div
		class="plot"
		bind:this={plotEl}
		bind:clientWidth={width}
		role="slider"
		tabindex="0"
		aria-label="Today as recorded — {tracks.length} tracks. Arrow keys read the day back."
		aria-valuemin={0}
		aria-valuemax={1440}
		aria-valuenow={scrubMinute}
		aria-valuetext={atCursor ? `${atCursor.time} — ${atCursor.text}` : summary}
		onpointermove={onMove}
		onpointerleave={() => (scrubMs = null)}
		onclick={onClick}
		onkeydown={onKey}
	>
		<svg width={width} height={deckH} viewBox="0 0 {width} {deckH}" aria-hidden="true">
			<!-- hour rules, behind everything -->
			{#each HOURS as h}
				<line class="rule" x1={px(h / 24)} y1={RULER_H - 4} x2={px(h / 24)} y2={deckH - 4} />
				{#if h < 24}
					<text class="hr" x={px(h / 24) + 3} y={RULER_H - 8}>{hourLabel(h)}</text>
				{/if}
			{/each}

			{#each laid as t, i (t.id)}
				<g class="track" class:wipe={!reduce} style="animation-delay:{0.05 + i * 0.045}s">
					<text
						class="name"
						class:off={t.cover.kind === "never"}
						class:dim={t.cover.kind === "silent"}
						class:blocked={t.cover.kind === "blocked"}
						x={GUTTER - 10}
						y={t.y + t.h - Math.max(2, (t.h - 8) / 2)}>{t.name}</text
					>

					{#if t.cover.kind === "blocked"}
						<text class="none blocked" x={GUTTER + 6} y={t.y + t.h - 1}>
							{t.cover.why}
						</text>
					{:else if t.cover.kind === "never"}
						<text class="none" x={GUTTER + 6} y={t.y + t.h - 1}>no source</text>
					{:else if t.cover.kind === "silent"}
						<text class="none stale" x={GUTTER + 6} y={t.y + t.h - 1}>{sinceLabel(t.cover.since)}</text>
					{:else if t.kind === "clips"}
						{#each t.clips as c, ci (t.id + ci)}
							<rect
								class="clip"
								x={px(c.x1)}
								y={t.y}
								width={Math.max(1, (c.x2 - c.x1) * plotW)}
								height={t.h}
								rx="1.5"
								style="fill:color-mix(in srgb, var(--color-foreground) {c.weight * 100}%, transparent)"
							/>
							{#if c.open}
								<line class="live" x1={px(c.x2)} y1={t.y} x2={px(c.x2)} y2={t.y + t.h} />
							{/if}
							{#if c.label && (c.x2 - c.x1) * plotW > 44}
								<text class="clabel" x={px(c.x1) + 5} y={t.y + t.h - Math.max(3, (t.h - 8) / 2)}>
									{c.label.length > Math.floor(((c.x2 - c.x1) * plotW - 10) / 5.4)
										? c.label.slice(0, Math.max(1, Math.floor(((c.x2 - c.x1) * plotW - 10) / 5.4) - 1)) + "…"
										: c.label}
								</text>
							{/if}
						{/each}
					{:else if t.kind === "bars"}
						{#each t.vals as v, bi (bi)}
							{#if v > 0}
								<rect
									class="bar"
									x={GUTTER + bi * barGeom(t.vals).w}
									y={t.y + t.h - Math.max(1.5, v * t.h)}
									width={barGeom(t.vals).bw}
									height={Math.max(1.5, v * t.h)}
								/>
							{/if}
						{/each}
					{:else if t.kind === "area"}
						{#if t.vals.length}<path class="area" d={areaPath(t.vals, t.y, t.h)} />{/if}
					{:else if t.kind === "line"}
						{#each t.segs as seg, si (si)}
							{#if seg.length > 1}<path class="hline" d={linePath(seg, t.y, t.h, t.lo, t.hi)} />{/if}
						{/each}
					{/if}
				</g>
			{/each}

			<!-- One axis for the whole deck rather than a baseline per track: solid
			     across the day that has happened, dashed across the day that has
			     not. Nine of these read as a ruled table, which the day is not. -->
			<line class="axis" x1={GUTTER} y1={deckH - 3.5} x2={px(nowF)} y2={deckH - 3.5} />
			<line class="axis future" x1={px(nowF)} y1={deckH - 3.5} x2={GUTTER + plotW} y2={deckH - 3.5} />

			<!-- now -->
			<line class="head" x1={px(nowF)} y1={RULER_H - 6} x2={px(nowF)} y2={deckH - 4} />
			<circle class="headcap" cx={px(nowF)} cy={RULER_H - 6} r="2.4" />

			{#if pinnedMs != null}
				<line class="pin" x1={px(f(pinnedMs))} y1={RULER_H - 9} x2={px(f(pinnedMs))} y2={deckH - 4} />
				<circle class="pincap" cx={px(f(pinnedMs))} cy={RULER_H - 10} r="3" />
			{/if}
			{#if scrubMs != null}
				<line class="scrub" x1={px(f(scrubMs))} y1={RULER_H - 4} x2={px(f(scrubMs))} y2={deckH - 4} />
			{/if}
		</svg>
	</div>

	<figcaption class="read" aria-live="polite">
		{#if atCursor}
			<span class="t mono">{atCursor.time}</span><span class="v">{atCursor.text}</span>
		{:else if anyData}
			<span class="t mono">{clockOf(nowMs)}</span><span class="v">{summary}</span>
		{:else if nSilent}
			<!-- Not "nothing happened": nothing was *delivered*. Naming the last
			     thing that did report is the difference between the two. -->
			<span class="v quiet">Nothing has reached the box today — every track has been silent since it last reported.</span>
		{:else}
			<span class="v quiet">Nothing has arrived yet today.</span>
		{/if}
		{#if coverageNote && !atCursor}
			<span class="dark mono">{coverageNote}</span>
		{/if}
	</figcaption>
</figure>

<style>
	/* The deck measures its container and sizes the SVG from that, so it must
	   never be able to push the container back — `min-width: 0` breaks the
	   flex/grid intrinsic-width loop that would otherwise pin it at its widest. */
	.deck { margin: 0; min-width: 0; }
	.plot { width: 100%; min-width: 0; touch-action: pan-y; cursor: crosshair; outline-offset: 6px; }
	.plot:focus-visible { outline: 2px solid var(--color-border-focus, var(--color-primary)); border-radius: 3px; }
	svg { display: block; max-width: 100%; }

	.rule { stroke: var(--color-foreground); stroke-opacity: 0.055; stroke-width: 1; }
	.hr {
		font-family: var(--font-mono); font-size: 9px; letter-spacing: 0.06em;
		fill: var(--color-foreground-subtle); opacity: 0.75;
	}
	.name {
		font-family: var(--font-mono); font-size: 9.5px; letter-spacing: 0.04em;
		fill: var(--color-foreground-subtle); text-anchor: end;
	}
	.name.off { fill: var(--color-foreground-disabled); }
	/* Silent sits between working and absent, and is coloured that way. */
	.name.dim { fill: var(--color-foreground-disabled); opacity: 0.85; }
	.none {
		font-family: var(--font-mono); font-size: 9px;
		fill: var(--color-foreground-disabled); opacity: 0.8;
	}
	.none.stale { fill: var(--color-foreground-subtle); opacity: 0.72; }
	/* Blocked is the one state here the reader can act on, so it is the one
	   state that does not recede. Full opacity, warning hue — deliberately
	   unlike `never` and `silent`, which are both varieties of "nothing to
	   report" and are drawn to fade. */
	.none.blocked { fill: var(--color-warning); opacity: 1; }
	.name.blocked { fill: var(--color-warning); opacity: 0.9; }

	.axis { stroke: var(--color-foreground); stroke-opacity: 0.16; stroke-width: 1; }
	.axis.future { stroke-opacity: 0.11; stroke-dasharray: 1 3; }

	.clip { shape-rendering: crispEdges; }
	.clabel {
		font-family: var(--font-sans); font-size: 10px;
		fill: var(--color-foreground-muted); pointer-events: none;
	}
	.live { stroke: var(--color-primary); stroke-width: 1.5; stroke-opacity: 0.9; }

	.bar { fill: var(--color-foreground); fill-opacity: 0.28; shape-rendering: crispEdges; }
	.area { fill: var(--color-foreground); fill-opacity: 0.13; }
	.hline {
		fill: none; stroke: var(--color-foreground); stroke-opacity: 0.5;
		stroke-width: 1; stroke-linejoin: round;
	}

	.head { stroke: var(--color-primary); stroke-width: 1; stroke-opacity: 0.9; animation: pulse 3.4s ease-in-out infinite; }
	.headcap { fill: var(--color-primary); animation: pulse 3.4s ease-in-out infinite; }
	@keyframes pulse { 50% { opacity: 0.45; } }
	.scrub { stroke: var(--color-foreground); stroke-opacity: 0.3; stroke-width: 1; }
	.pin { stroke: var(--color-foreground); stroke-opacity: 0.55; stroke-width: 1; }
	.pincap { fill: var(--color-foreground); fill-opacity: 0.7; }

	.track.wipe { animation: wipe 0.66s cubic-bezier(0.2, 0.75, 0.2, 1) both; }
	@keyframes wipe {
		from { clip-path: inset(0 100% 0 0); opacity: 0.4; }
		to { clip-path: inset(0 0 0 0); opacity: 1; }
	}

	.read {
		display: flex; align-items: baseline; gap: 10px; flex-wrap: wrap;
		margin-top: 12px; padding-left: 62px;
		font-family: var(--font-sans); font-size: 13px; color: var(--color-foreground-subtle);
	}
	.read .t { color: var(--color-foreground); font-size: 13px; }
	.read .v { font-family: var(--font-sans); }
	.read .quiet { color: var(--color-foreground-subtle); }
	.read .dark { margin-left: auto; color: var(--color-foreground-disabled); font-size: 13px; }
	.mono { font-family: var(--font-mono); font-variant-numeric: tabular-nums; }

	@media (prefers-reduced-motion: reduce) {
		.head, .headcap { animation: none; }
	}
	@media (max-width: 640px) {
		.read { padding-left: 0; }
		.read .dark { margin-left: 0; width: 100%; }
	}
</style>
