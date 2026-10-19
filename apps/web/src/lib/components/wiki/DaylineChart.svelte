<!--
	DaylineChart.svelte - the day line.

	The day's events across its own clock, each a block from its start to its
	end, as tall as it was unlike the owner's usual: above the line when it was
	unlike, below when it was more usual than usual. A stretch the record could
	not name stays empty, and an event nobody scored is an outline on the line.
	Under it, one thin lane per stream the box collects, filled where that
	stream recorded something, so a quiet stretch of the line can be read
	against what was listening at the time.

	The plot stretches to its width while its height stays fixed, so every
	label is HTML: text inside a stretched SVG would stretch with it.
-->

<script lang="ts">
	import { onDestroy, onMount } from "svelte";
	import { getLocalDateSlug } from "$lib/utils/dateUtils";
	import type { DayEvent } from "$lib/wiki/types";
	import { getLifeline, type LifelineLane } from "$lib/wiki/api";
	import {
		clockLabel,
		COVERAGE_EXPAND,
		coverageLanes,
		dayFraction,
		dayLineBlocks,
		dayWindow,
		hourTicks,
		mostUnlikeUsual,
		UNLIKE_USUAL_Z,
		usualWords,
		Z_LIMIT,
		type DayLineBlock,
	} from "$lib/wiki/dayLine";

	interface Props {
		events: DayEvent[];
		timezone: string | null;
		pageDate?: Date;
		dayDateSlug?: string;
	}

	let { events, timezone, pageDate, dayDateSlug = "" }: Props = $props();

	const slug = $derived(dayDateSlug || (pageDate ? getLocalDateSlug(pageDate) : ""));
	const win = $derived(dayWindow(slug, timezone));
	const ticks = $derived(slug ? hourTicks(slug, timezone, 3) : []);

	// ── the events ──────────────────────────────────────────────
	const blocks = $derived(slug ? dayLineBlocks(events, win) : []);
	const topId = $derived(mostUnlikeUsual(events));

	/** Plot units across; the SVG stretches them to whatever width it gets. */
	const W = 1000;
	const BAND_H = 104;
	const MID = BAND_H / 2;
	const UNIT = (MID - 4) / Z_LIMIT;
	const LANE_H = 12;

	function blockClass(b: DayLineBlock): string {
		if (b.id === topId) return "top";
		if (b.z === null) return "unscored";
		if (b.z >= UNLIKE_USUAL_Z) return "unlike";
		return b.z > 0 ? "near" : "like";
	}

	// ── coverage lanes ──────────────────────────────────────────
	// Ten-minute cells: a single message is a visible mark, and an hour of
	// recording reads as one run rather than a comb.
	const CELLS = 144;
	let lanes = $state<LifelineLane[]>([]);
	let loadedFor = "";
	let loadSeq = 0;

	$effect(() => {
		const key = `${slug}|${timezone ?? ""}`;
		if (!slug || key === loadedFor) return;
		loadedFor = key;
		const seq = ++loadSeq;
		const { startMs, endMs } = win;
		// A load that fails draws no lanes and leaves the day unloaded, so the next render asks again.
		const failed = () => {
			if (seq !== loadSeq) return;
			loadedFor = "";
			lanes = [];
		};
		getLifeline(CELLS, new Date(startMs).toISOString(), new Date(endMs).toISOString(), COVERAGE_EXPAND)
			.then((data) => {
				if (!data) return failed();
				if (seq === loadSeq) lanes = data.lanes ?? [];
			})
			.catch(failed);
	});

	const coverage = $derived(coverageLanes(lanes, win));

	// ── now ─────────────────────────────────────────────────────
	let nowMs = $state(Date.now());
	let clock: ReturnType<typeof setInterval> | null = null;
	onMount(() => {
		clock = setInterval(() => (nowMs = Date.now()), 60_000);
	});
	onDestroy(() => {
		if (clock) clearInterval(clock);
	});
	const nowAt = $derived(nowMs >= win.startMs && nowMs < win.endMs ? dayFraction(nowMs, win) : null);

	// ── hover ───────────────────────────────────────────────────
	let bandEl = $state<HTMLDivElement | null>(null);
	let hoverId = $state<string | null>(null);
	const hovered = $derived(blocks.find((b) => b.id === hoverId) ?? null);

	function pick(clientX: number) {
		if (!bandEl) return;
		const rect = bandEl.getBoundingClientRect();
		if (rect.width <= 0) return;
		const f = (clientX - rect.left) / rect.width;
		const inside = blocks.find((b) => f >= b.x1 && f < b.x2);
		if (inside) {
			hoverId = inside.id;
			return;
		}
		// A sliver is hard to land on: take the nearest within a hair.
		let best: DayLineBlock | null = null;
		let dist = 0.012;
		for (const b of blocks) {
			const d = f < b.x1 ? b.x1 - f : f - b.x2;
			if (d < dist) {
				dist = d;
				best = b;
			}
		}
		hoverId = best?.id ?? null;
	}

	const tipAt = $derived(hovered ? (hovered.x1 + hovered.x2) / 2 : 0);

	const summary = $derived.by(() => {
		const unlike = blocks.filter((b) => b.z !== null && b.z >= UNLIKE_USUAL_Z).length;
		const n = blocks.length;
		return `The day line: ${n} ${n === 1 ? "event" : "events"}, ${unlike} unlike your usual`;
	});
</script>

<figure class="dayline" aria-label={summary}>
	<div class="chart">
		<div class="labels" aria-hidden="true">
			<div class="band-label" style="height: {BAND_H}px">
				<span style="top: {MID}px">Usual</span>
			</div>
			{#each coverage as lane (lane.label)}
				<div class="lane-label" style="height: {LANE_H}px">{lane.label}</div>
			{/each}
		</div>

		<div class="plots">
			<div class="gridlines" aria-hidden="true">
				{#each ticks as t (t.label + t.at)}
					<span style="left: {t.at * 100}%"></span>
				{/each}
				{#if nowAt !== null}
					<span class="now" style="left: {nowAt * 100}%"></span>
				{/if}
			</div>

			<!-- svelte-ignore a11y_no_static_element_interactions -->
			<div
				class="band"
				style="height: {BAND_H}px"
				bind:this={bandEl}
				onpointermove={(e) => pick(e.clientX)}
				onpointerdown={(e) => pick(e.clientX)}
				onpointerleave={(e) => {
					// A touch leaves as its finger lifts; its tip stays until the next tap.
					if (e.pointerType === "mouse") hoverId = null;
				}}
			>
				<svg viewBox="0 0 {W} {BAND_H}" preserveAspectRatio="none" aria-hidden="true">
					<line x1="0" x2={W} y1={MID} y2={MID} class="baseline" />
					{#each blocks as b (b.id)}
						{@const x = b.x1 * W}
						{@const w = Math.max(1.5, (b.x2 - b.x1) * W - 1)}
						{#if b.z === null}
							<rect
								{x}
								y={MID - 3}
								width={w}
								height="6"
								class="block unscored"
								class:on={b.id === hoverId}
							/>
						{:else}
							{@const y = MID - b.z * UNIT}
							<rect
								{x}
								y={Math.min(MID, y)}
								width={w}
								height={Math.max(1.5, Math.abs(y - MID))}
								class="block {blockClass(b)}"
								class:on={b.id === hoverId}
							/>
						{/if}
					{/each}
				</svg>

				{#if hovered}
					<div
						class="tip"
						class:left={tipAt > 0.7}
						class:right={tipAt < 0.3}
						style="left: {tipAt * 100}%"
						role="status"
					>
						<p class="tip-title">{hovered.label}</p>
						<p class="tip-when">
							{clockLabel(hovered.startMs, win.zone)} – {clockLabel(hovered.endMs, win.zone)}
						</p>
						<p class="tip-usual" class:top={hovered.id === topId}>
							{hovered.id === topId ? "Most unlike your usual" : usualWords(hovered.z)}
						</p>
					</div>
				{/if}
			</div>

			{#each coverage as lane (lane.label)}
				<div class="lane" style="height: {LANE_H}px">
					<svg viewBox="0 0 {W} {LANE_H}" preserveAspectRatio="none" aria-hidden="true">
						<line x1="0" x2={W} y1={LANE_H / 2} y2={LANE_H / 2} class="track" />
						{#each lane.runs as [a, b]}
							<rect x={a * W} y={LANE_H / 2 - 3} width={Math.max(1, (b - a) * W)} height="6" class="run" />
						{/each}
					</svg>
				</div>
			{/each}

			<div class="axis" aria-hidden="true">
				{#each ticks as t, i (t.label + t.at)}
					<span class:first={i === 0} class:last={i === ticks.length - 1} style="left: {t.at * 100}%">
						{t.label}
					</span>
				{/each}
			</div>
		</div>
	</div>

	{#if blocks.length === 0}
		<figcaption class="note">No events for this day yet.</figcaption>
	{:else}
		<figcaption class="note">
			The higher an event rises above the line, the less it was like your usual. An outline is an event
			your server hasn't scored.
		</figcaption>
	{/if}
</figure>

<style>
	.dayline {
		margin: 1rem 0 0.5rem;
	}

	.chart {
		display: grid;
		grid-template-columns: 4rem 1fr;
		column-gap: 0.5rem;
	}

	/* ── labels ─────────────────────────────────────────────── */

	.labels {
		display: flex;
		flex-direction: column;
		gap: 4px;
		font-family: var(--font-sans, system-ui, sans-serif);
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
		text-align: right;
	}

	.band-label {
		position: relative;
		margin-bottom: 6px;
	}

	.band-label span {
		position: absolute;
		right: 0;
		transform: translateY(-50%);
	}

	.lane-label {
		display: flex;
		align-items: center;
		justify-content: flex-end;
		line-height: 1;
	}

	/* ── plots ──────────────────────────────────────────────── */

	.plots {
		position: relative;
		display: flex;
		flex-direction: column;
		gap: 4px;
		min-width: 0;
	}

	.gridlines {
		position: absolute;
		inset: 0 0 1.25rem 0;
		pointer-events: none;
	}

	.gridlines span {
		position: absolute;
		top: 0;
		bottom: 0;
		width: 1px;
		background: var(--color-border-subtle, var(--color-border));
	}

	.gridlines span.now {
		background: var(--color-primary);
		opacity: 0.5;
	}

	.band {
		position: relative;
		margin-bottom: 6px;
		touch-action: pan-y;
	}

	svg {
		display: block;
		width: 100%;
		height: 100%;
		overflow: visible;
	}

	.baseline {
		stroke: var(--color-border-strong);
		stroke-width: 1;
		stroke-dasharray: 3 3;
		vector-effect: non-scaling-stroke;
	}

	.block {
		vector-effect: non-scaling-stroke;
		transition: fill-opacity 0.12s ease;
	}

	.block.unlike {
		fill: var(--color-foreground);
		fill-opacity: 0.55;
	}

	.block.near {
		fill: var(--color-foreground);
		fill-opacity: 0.28;
	}

	.block.like {
		fill: var(--color-foreground);
		fill-opacity: 0.16;
	}

	.block.top {
		fill: var(--color-secondary);
		fill-opacity: 0.85;
	}

	.block.unscored {
		fill: none;
		stroke: var(--color-foreground-subtle);
		stroke-width: 1;
		stroke-opacity: 0.7;
	}

	.block.on:not(.unscored) {
		fill-opacity: 0.9;
	}

	.block.unscored.on {
		stroke-opacity: 1;
		stroke: var(--color-foreground);
	}

	.track {
		stroke: var(--color-border-subtle, var(--color-border));
		stroke-width: 1;
		vector-effect: non-scaling-stroke;
	}

	.run {
		fill: var(--color-foreground-muted);
		fill-opacity: 0.45;
	}

	/* ── axis ───────────────────────────────────────────────── */

	.axis {
		position: relative;
		height: 1.25rem;
		font-family: var(--font-sans, system-ui, sans-serif);
		font-size: 0.6875rem;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground-subtle);
	}

	.axis span {
		position: absolute;
		top: 0.25rem;
		transform: translateX(-50%);
		white-space: nowrap;
	}

	.axis span.first {
		transform: none;
	}

	.axis span.last {
		transform: translateX(-100%);
	}

	/* ── tooltip ────────────────────────────────────────────── */

	.tip {
		position: absolute;
		bottom: calc(100% + 6px);
		transform: translateX(-50%);
		z-index: 2;
		max-width: 16rem;
		padding: 0.4rem 0.6rem;
		background: var(--color-surface-overlay, var(--color-surface));
		border: 1px solid var(--color-border);
		border-radius: 6px;
		pointer-events: none;
		font-family: var(--font-sans, system-ui, sans-serif);
	}

	.tip.left {
		transform: translateX(-100%);
	}

	.tip.right {
		transform: none;
	}

	.tip p {
		margin: 0;
		line-height: 1.4;
	}

	.tip-title {
		font-size: 0.8125rem;
		font-weight: 500;
		color: var(--color-foreground);
	}

	.tip-when {
		font-size: 0.75rem;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground-subtle);
	}

	.tip-usual {
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
	}

	.tip-usual.top {
		color: var(--color-secondary);
	}

	.note {
		margin: 0.5rem 0 0;
		font-size: 0.75rem;
		line-height: 1.5;
		color: var(--color-foreground-subtle);
	}

	@media (max-width: 500px) {
		.chart {
			grid-template-columns: 3.25rem 1fr;
		}
	}
</style>
