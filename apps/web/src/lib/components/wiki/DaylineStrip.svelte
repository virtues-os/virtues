<!--
	DaylineStrip.svelte — the day as a mini lifeline.

	One left-to-right strip, midnight to midnight: sleep cycles and the day's
	events as gantt blocks, then the raw record underneath as per-lane density
	bars from the same endpoint the Lifeline console draws. The lifeline shows
	the shape of a life; this shows the shape of one day, in the same visual
	language, so moving between the two costs nothing.

	Every x-position is (instant − local midnight) — the day's timezone decides
	where midnight falls, and everything after that is linear arithmetic. On a
	DST day the axis is off by the shifted hour; the lifeline console accepts
	the same simplification.
-->

<script lang="ts">
	import { onMount, onDestroy } from "svelte";
	import type { DayEvent, ScoredSleepCycle } from "$lib/wiki/types";
	import { getLifeline, type LifelineLane } from "$lib/wiki/api";

	interface Props {
		events: DayEvent[];
		timezone: string | null;
		dayDateSlug: string;
		sleepCycles?: ScoredSleepCycle[];
	}

	let { events, timezone, dayDateSlug, sleepCycles = [] }: Props = $props();

	// ── the day's window ────────────────────────────────────────
	/**
	 * The UTC instant of local midnight on `slug` in `tz`. Two Intl passes
	 * converge across a DST boundary; without a timezone the browser's is the
	 * only honest guess.
	 */
	function zonedMidnightMs(slug: string, tz: string | null): number {
		if (!tz) return new Date(`${slug}T00:00:00`).getTime();
		const utcGuess = Date.parse(`${slug}T00:00:00Z`);
		let t = utcGuess;
		for (let i = 0; i < 2; i++) {
			const parts = new Intl.DateTimeFormat("en-CA", {
				timeZone: tz,
				hour12: false,
				year: "numeric",
				month: "2-digit",
				day: "2-digit",
				hour: "2-digit",
				minute: "2-digit",
			}).formatToParts(new Date(t));
			const g = (k: string) => parts.find((p) => p.type === k)?.value ?? "00";
			const hh = g("hour") === "24" ? "00" : g("hour");
			const local = Date.parse(`${g("year")}-${g("month")}-${g("day")}T${hh}:${g("minute")}:00Z`);
			t += utcGuess - local;
		}
		return t;
	}

	const dayStartMs = $derived(zonedMidnightMs(dayDateSlug, timezone));
	const DAY_MS = 24 * 3_600_000;

	/** Hours into the day, clamped to the strip. */
	function hourOf(t: number): number {
		return Math.min(24, Math.max(0, (t - dayStartMs) / 3_600_000));
	}

	// ── layout ──────────────────────────────────────────────────
	const WIDTH = 840;
	const ML = 96; // label gutter
	const MR = 16;
	const MT = 10;
	const AXIS_H = 26;
	const PLOT_W = WIDTH - ML - MR;

	const SLEEP_H = 20;
	const EVENT_ROW_H = 24;
	const LANE_H = 18;
	const GROUP_GAP = 8;

	function xOf(hour: number): number {
		return ML + (hour / 24) * PLOT_W;
	}

	const HOUR_TICKS = [0, 3, 6, 9, 12, 15, 18, 21, 24];
	function hourLabel(h: number): string {
		if (h === 0 || h === 24) return "12am";
		if (h === 12) return "12pm";
		return h < 12 ? `${h}am` : `${h - 12}pm`;
	}

	// ── sleep blocks ────────────────────────────────────────────
	// Scored cycles when any actually intersect this day's window; otherwise
	// the sleep events themselves, so a day without usable cycle data still
	// shows when you slept. (Clamping decides intersection: a cycle wholly
	// outside the window flattens to zero width and drops out.)
	const sleepBlocks = $derived.by(() => {
		const clamp = (s: number, e: number, stage: string) => ({
			x1: hourOf(s),
			x2: hourOf(e),
			stage,
		});
		const fromCycles = sleepCycles
			.map((c) => clamp(c.startTime.getTime(), c.endTime.getTime(), c.dominantStage))
			.filter((b) => b.x2 - b.x1 > 0.01);
		if (fromCycles.length) return fromCycles;
		return events
			.filter((e) => e.isSleep && !e.userHidden)
			.map((e) => clamp(e.startTime.getTime(), e.endTime.getTime(), "sleep"))
			.filter((b) => b.x2 - b.x1 > 0.01);
	});

	// ── event blocks, packed into sub-rows on overlap ───────────
	interface EventBlock {
		id: string;
		x1: number;
		x2: number;
		row: number;
		label: string;
		tip: string;
		unknown: boolean;
	}

	function fmtHour(h: number): string {
		const hh = Math.floor(h) % 24;
		const mm = Math.round((h - Math.floor(h)) * 60);
		const ampm = hh < 12 ? "am" : "pm";
		const h12 = hh === 0 ? 12 : hh > 12 ? hh - 12 : hh;
		return `${h12}:${String(mm).padStart(2, "0")}${ampm}`;
	}

	const eventBlocks = $derived.by<EventBlock[]>(() => {
		const rowEnds: number[] = [];
		return events
			.filter((e) => !e.userHidden && !e.isSleep)
			.map((e) => ({
				e,
				x1: hourOf(e.startTime.getTime()),
				x2: hourOf(e.endTime.getTime()),
			}))
			.filter((b) => b.x2 - b.x1 > 0.01)
			.sort((a, b) => a.x1 - b.x1)
			.map(({ e, x1, x2 }) => {
				let row = rowEnds.findIndex((end) => end <= x1 + 0.02);
				if (row === -1) {
					row = rowEnds.length;
					rowEnds.push(0);
				}
				rowEnds[row] = x2;
				const label = e.userLabel ?? e.autoLabel;
				return {
					id: e.id,
					x1,
					x2,
					row,
					label,
					tip: `${fmtHour(x1)}–${fmtHour(x2)} · ${e.eventSummary ?? label}`,
					unknown: e.isUnknown ?? false,
				};
			});
	});

	const eventRows = $derived(Math.max(1, ...eventBlocks.map((b) => b.row + 1)));

	// ── raw-record lanes, from the lifeline endpoint ────────────
	// 288 buckets = five minutes each: fine enough that a single message is a
	// visible tick, coarse enough that the response stays small.
	const BUCKETS = 288;
	const LANE_ORDER = [
		"communication",
		"location",
		"activity",
		"audio",
		"calendar",
		"health",
		"finance",
		"content",
	];

	let lanes = $state<LifelineLane[]>([]);
	let lanesLoaded = $state(false);

	async function loadLanes() {
		const data = await getLifeline(
			BUCKETS,
			new Date(dayStartMs).toISOString(),
			new Date(dayStartMs + DAY_MS).toISOString()
		);
		// Only lanes with something in this day: the console owes the honest
		// empty-lane account; the mini view is a summary of what happened.
		lanes = (data?.lanes ?? [])
			.filter((l) => l.density.some((d) => d > 0))
			.sort((a, b) => {
				const ia = LANE_ORDER.indexOf(a.id);
				const ib = LANE_ORDER.indexOf(b.id);
				return (ia === -1 ? 99 : ia) - (ib === -1 ? 99 : ib) || a.id.localeCompare(b.id);
			});
		lanesLoaded = true;
	}

	let lastLoadedDay = "";
	$effect(() => {
		if (dayDateSlug !== lastLoadedDay) {
			lastLoadedDay = dayDateSlug;
			void loadLanes();
		}
	});

	function laneLabel(id: string): string {
		return id.charAt(0).toUpperCase() + id.slice(1);
	}

	// ── now marker ──────────────────────────────────────────────
	let nowMs = $state(Date.now());
	let clock: ReturnType<typeof setInterval> | null = null;
	onMount(() => {
		clock = setInterval(() => (nowMs = Date.now()), 30_000);
	});
	onDestroy(() => {
		if (clock) clearInterval(clock);
	});
	const nowHour = $derived((nowMs - dayStartMs) / 3_600_000);
	const showNow = $derived(nowHour > 0 && nowHour < 24);

	// ── vertical layout, computed off what exists ───────────────
	const sleepTop = $derived(MT);
	const sleepBand = $derived(sleepBlocks.length > 0 ? SLEEP_H : 0);
	const eventsTop = $derived(sleepTop + sleepBand + (sleepBand ? 4 : 0));
	const eventsBand = $derived(eventBlocks.length > 0 ? eventRows * (EVENT_ROW_H + 2) : 0);
	const lanesTop = $derived(eventsTop + eventsBand + (lanes.length ? GROUP_GAP : 0));
	const lanesBand = $derived(lanes.length * LANE_H);
	const axisTop = $derived(lanesTop + lanesBand + 4);
	const HEIGHT = $derived(axisTop + AXIS_H);
</script>

<svg viewBox="0 0 {WIDTH} {HEIGHT}" preserveAspectRatio="xMidYMid meet" class="dayline-strip">
	<!-- hour grid, hairline -->
	{#each HOUR_TICKS as h}
		<line
			x1={xOf(h)}
			y1={MT}
			x2={xOf(h)}
			y2={axisTop}
			stroke="var(--color-border, #e5e5e5)"
			stroke-width="0.5"
		/>
		<text x={xOf(h)} y={axisTop + 16} text-anchor="middle" class="tick-label">
			{hourLabel(h)}
		</text>
	{/each}

	<!-- sleep -->
	{#if sleepBand}
		<text x={ML - 8} y={sleepTop + SLEEP_H / 2} text-anchor="end" dominant-baseline="middle" class="lane-label">
			Sleep
		</text>
		{#each sleepBlocks as b}
			<rect
				x={xOf(b.x1)}
				y={sleepTop + 2}
				width={Math.max(1.5, xOf(b.x2) - xOf(b.x1))}
				height={SLEEP_H - 4}
				rx="2"
				class="sleep-block"
			>
				<title>{fmtHour(b.x1)}–{fmtHour(b.x2)} · {b.stage}</title>
			</rect>
		{/each}
	{/if}

	<!-- events -->
	{#if eventsBand}
		<text
			x={ML - 8}
			y={eventsTop + eventsBand / 2}
			text-anchor="end"
			dominant-baseline="middle"
			class="lane-label"
		>
			Events
		</text>
		{#each eventBlocks as b}
			{@const bx = xOf(b.x1)}
			{@const bw = Math.max(2, xOf(b.x2) - xOf(b.x1))}
			{@const by = eventsTop + b.row * (EVENT_ROW_H + 2)}
			<rect
				x={bx}
				y={by}
				width={bw}
				height={EVENT_ROW_H}
				rx="3"
				class="event-block"
				class:unknown={b.unknown}
			>
				<title>{b.tip}</title>
			</rect>
			{#if bw > 46}
				<!-- Only label what there is room to label — a clipped word is
				     worse than none; the tooltip carries the rest. -->
				<text x={bx + 6} y={by + EVENT_ROW_H / 2} dominant-baseline="middle" class="event-label">
					{b.label.length > Math.floor(bw / 7) ? b.label.slice(0, Math.floor(bw / 7) - 1) + "…" : b.label}
				</text>
			{/if}
		{/each}
	{/if}

	<!-- raw-record lanes -->
	{#each lanes as lane, li}
		{@const top = lanesTop + li * LANE_H}
		{@const peak = Math.max(...lane.density, 1)}
		{@const bw = PLOT_W / BUCKETS}
		<text x={ML - 8} y={top + LANE_H / 2} text-anchor="end" dominant-baseline="middle" class="lane-label">
			{laneLabel(lane.id)}
		</text>
		<line x1={ML} y1={top + LANE_H - 2} x2={ML + PLOT_W} y2={top + LANE_H - 2}
			stroke="var(--color-border, #e5e5e5)" stroke-width="0.5" />
		{#each lane.density as d, i}
			{#if d > 0}
				<rect
					x={ML + i * bw}
					y={top + LANE_H - 2 - Math.max(1.5, (d / peak) * (LANE_H - 5))}
					width={Math.max(1, bw - 0.4)}
					height={Math.max(1.5, (d / peak) * (LANE_H - 5))}
					class="density-bar"
				/>
			{/if}
		{/each}
	{/each}

	{#if lanesLoaded && lanes.length === 0}
		<text x={ML} y={lanesTop + 12} class="lane-label">No raw data recorded for this day</text>
	{/if}

	<!-- now -->
	{#if showNow}
		<line x1={xOf(nowHour)} y1={MT - 2} x2={xOf(nowHour)} y2={axisTop} class="now-line" />
		<circle cx={xOf(nowHour)} cy={MT - 2} r="2.5" class="now-dot" />
	{/if}
</svg>

<style>
	.dayline-strip {
		width: 100%;
		height: auto;
		display: block;
	}

	.tick-label {
		font-size: 10px;
		fill: var(--color-foreground-subtle, #999);
		font-family: var(--font-sans, sans-serif);
	}

	.lane-label {
		font-size: 10px;
		fill: var(--color-foreground-muted, #888);
		font-family: var(--font-sans, sans-serif);
	}

	.sleep-block {
		fill: var(--color-foreground-muted, #888);
		fill-opacity: 0.28;
	}

	.event-block {
		fill: var(--color-primary, #4f46e5);
		fill-opacity: 0.14;
		stroke: var(--color-primary, #4f46e5);
		stroke-opacity: 0.45;
		stroke-width: 0.75;
	}

	.event-block.unknown {
		fill: var(--color-foreground-muted, #888);
		fill-opacity: 0.08;
		stroke: var(--color-border-strong, #ccc);
		stroke-dasharray: 3 2;
	}

	.event-label {
		font-size: 11px;
		fill: var(--color-foreground, #222);
		font-family: var(--font-sans, sans-serif);
		pointer-events: none;
	}

	.density-bar {
		fill: var(--color-foreground-muted, #888);
		fill-opacity: 0.55;
	}

	.now-line {
		stroke: var(--color-primary, #4f46e5);
		stroke-width: 1;
	}

	.now-dot {
		fill: var(--color-primary, #4f46e5);
	}
</style>
