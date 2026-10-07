<!--
	A chart drawn from a query's rows: the first column is the x axis, the
	rest are one to three series. Bars start at zero; a line takes the range
	its values span. One accent: the first series is the primary and the
	others are ink, told apart by weight and dash, because a second hue would
	claim a kinship the record does not have (design-grammar §5).
-->
<script lang="ts">
	import {
		columnLabel,
		formatNumber,
		formatX,
		isDateColumn,
		isTimeColumn,
		labelIndices,
		niceTicks,
		parseTime,
		type Row,
	} from "./show";

	interface Props {
		columns: string[];
		rows: Row[];
		mark: "bar" | "line";
	}
	let { columns, rows, mark }: Props = $props();

	const HEIGHT = 200;
	const PAD = { top: 8, right: 8, bottom: 22, left: 40 };

	let width = $state(0);
	let hover = $state<number | null>(null);

	const xKey = $derived(columns[0]);
	const series = $derived(columns.slice(1));
	const xs = $derived(rows.map((r) => r[xKey]));
	const dates = $derived(isDateColumn(xs) || isTimeColumn(xs));
	// A line over real times is spaced by time, so a missing week reads as a
	// gap; anything else is spaced evenly.
	// Only when every x is a time: a null x has no place on a time axis.
	const times = $derived.by(() => {
		if (mark !== "line" || !dates) return null;
		const ts = xs.map((v) => parseTime(v)?.getTime());
		return ts.every((t) => t !== undefined) ? (ts as number[]) : null;
	});
	const tLo = $derived(times ? Math.min(...times) : 0);
	const tHi = $derived(times ? Math.max(...times) : 0);
	/** Row indices in drawing order: by time on a time axis, whatever order
	 *  the query returned them in. */
	const order = $derived(
		times ? rows.map((_, i) => i).sort((a, b) => times[a] - times[b]) : rows.map((_, i) => i),
	);

	const values = $derived(
		series.flatMap((s) => rows.map((r) => r[s])).filter((v): v is number => typeof v === "number"),
	);
	const ticks = $derived.by(() => {
		if (!values.length) return [0, 1];
		const lo = Math.min(...values);
		const hi = Math.max(...values);
		return mark === "bar" ? niceTicks(Math.min(0, lo), Math.max(0, hi)) : niceTicks(lo, hi);
	});
	const yMin = $derived(ticks[0]);
	const yMax = $derived(ticks[ticks.length - 1]);

	const plotW = $derived(Math.max(0, width - PAD.left - PAD.right));
	const plotH = HEIGHT - PAD.top - PAD.bottom;
	const y = (v: number) => PAD.top + plotH - ((v - yMin) / (yMax - yMin || 1)) * plotH;

	const band = $derived(plotW / Math.max(1, rows.length));
	const xAt = (i: number): number => {
		if (mark === "bar") return PAD.left + band * (i + 0.5);
		if (times) {
			return PAD.left + (tHi === tLo ? plotW / 2 : ((times[i] - tLo) / (tHi - tLo)) * plotW);
		}
		return PAD.left + (rows.length === 1 ? plotW / 2 : (i / (rows.length - 1)) * plotW);
	};

	/** Bars side by side within a band, 2px apart, never wider than 40px. */
	const GAP = 2;
	const barW = $derived(Math.max(1, Math.min(40, (band - GAP * (series.length + 1)) / series.length)));
	const barX = (i: number, s: number) =>
		xAt(i) - (barW * series.length + GAP * (series.length - 1)) / 2 + s * (barW + GAP);

	/** A bar with its data end rounded and its baseline end square. */
	function barPath(x: number, v: number): string {
		const y0 = y(Math.max(yMin, Math.min(0, yMax)));
		const y1 = y(v);
		const h = Math.abs(y1 - y0);
		const r = Math.min(4, barW / 2, h);
		if (v >= 0) {
			const top = y1;
			return `M${x},${y0}V${top + r}Q${x},${top} ${x + r},${top}H${x + barW - r}Q${x + barW},${top} ${x + barW},${top + r}V${y0}Z`;
		}
		const bottom = y1;
		return `M${x},${y0}V${bottom - r}Q${x},${bottom} ${x + r},${bottom}H${x + barW - r}Q${x + barW},${bottom} ${x + barW},${bottom - r}V${y0}Z`;
	}

	/** One path per series; a null breaks the line rather than bridging it. */
	function linePath(s: string): string {
		let d = "";
		let pen = false;
		order.forEach((i) => {
			const v = rows[i][s];
			if (typeof v !== "number") {
				pen = false;
				return;
			}
			d += `${pen ? "L" : "M"}${xAt(i).toFixed(1)},${y(v).toFixed(1)}`;
			pen = true;
		});
		return d;
	}

	// A short date label is about 44px of 9.5px mono; give each one 52.
	const LABEL_W = 52;
	const labelled = $derived(
		labelIndices(rows.length, Math.max(2, Math.floor(plotW / LABEL_W))).map((n) => order[n]),
	);
	/** A category name cut to the room a label has (about 6px a character). */
	const fit = (text: string) => {
		const room = Math.max(4, Math.floor(Math.max(LABEL_W, band) / 6) - 1);
		return text.length > room ? `${text.slice(0, room - 1)}…` : text;
	};

	function onMove(e: PointerEvent) {
		const box = (e.currentTarget as SVGElement).getBoundingClientRect();
		const px = e.clientX - box.left;
		let best = 0;
		let dist = Infinity;
		for (let i = 0; i < rows.length; i++) {
			const d = Math.abs(xAt(i) - px);
			if (d < dist) [best, dist] = [i, d];
		}
		hover = best;
	}

	const readout = $derived(
		hover === null
			? null
			: {
					x: formatX(xs[hover], dates),
					values: series.map((s) => {
						const v = rows[hover!][s];
						return { name: s, value: typeof v === "number" ? formatNumber(v) : "-" };
					}),
				},
	);
</script>

<div class="chart" bind:clientWidth={width}>
	<div class="readout" aria-live="polite">
		{#if readout}
			<span class="when">{readout.x}</span>
			{#each readout.values as v, s}
				<span class="figure"><i class="swatch s{s} {mark}"></i>{series.length > 1 ? `${columnLabel(v.name)} ` : ""}<b>{v.value}</b></span>
			{/each}
		{:else if series.length > 1}
			{#each series as name, s}
				<span class="figure"><i class="swatch s{s} {mark}"></i>{columnLabel(name)}</span>
			{/each}
		{:else}
			<span class="figure">{columnLabel(series[0])}</span>
		{/if}
	</div>
	{#if width > 0}
		<svg
			{width}
			height={HEIGHT}
			role="img"
			aria-label="{mark === 'bar' ? 'Bar' : 'Line'} chart of {series.join(', ')} by {xKey}"
			onpointermove={onMove}
			onpointerleave={() => (hover = null)}
		>
			{#each ticks as t}
				<line class="grid" class:zero={t === 0} x1={PAD.left} x2={width - PAD.right} y1={y(t)} y2={y(t)} />
				<text class="tick" x={PAD.left - 6} y={y(t)} text-anchor="end" dominant-baseline="middle">{formatNumber(t)}</text>
			{/each}
			{#each labelled as i}
				<text
					class="tick"
					x={xAt(i)}
					y={HEIGHT - 6}
					text-anchor={mark !== "line" ? "middle" : i === labelled[labelled.length - 1] && labelled.length > 1 ? "end" : i === labelled[0] ? "start" : "middle"}
					>{dates ? formatX(xs[i], dates) : fit(formatX(xs[i], dates))}</text
				>
			{/each}

			{#if hover !== null}
				<line class="cross" x1={xAt(hover)} x2={xAt(hover)} y1={PAD.top} y2={PAD.top + plotH} />
			{/if}

			{#if mark === "bar"}
				{#each rows as r, i}
					{#each series as s, si}
						{#if typeof r[s] === "number"}
							<path class="bar s{si}" class:dim={hover !== null && hover !== i} d={barPath(barX(i, si), r[s] as number)} />
						{/if}
					{/each}
				{/each}
			{:else}
				{#each series as s, si}
					<path class="line s{si}" d={linePath(s)} />
				{/each}
				{#if hover !== null}
					{#each series as s, si}
						{#if typeof rows[hover][s] === "number"}
							<circle class="dot s{si}" cx={xAt(hover)} cy={y(rows[hover][s] as number)} r="4" />
						{/if}
					{/each}
				{/if}
			{/if}
		</svg>
	{/if}
</div>

<style>
	.chart {
		width: 100%;
		touch-action: pan-y;
	}
	.readout {
		display: flex;
		flex-wrap: wrap;
		gap: 0.25rem 1rem;
		min-height: 1.25rem;
		margin-bottom: 0.375rem;
		font-family: var(--font-sans);
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
	}
	.readout .when {
		color: var(--color-foreground);
	}
	.readout b {
		font-weight: 500;
		color: var(--color-foreground);
	}
	.figure {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
	}
	.swatch {
		display: inline-block;
		width: 12px;
		height: 2px;
		background: var(--color-primary);
	}
	.swatch.bar {
		height: 8px;
		width: 8px;
		border-radius: 2px;
	}
	.swatch.s1 {
		background: color-mix(in srgb, var(--color-foreground) 60%, transparent);
	}
	.swatch.s2 {
		background: var(--color-foreground-subtle);
	}
	svg {
		display: block;
		overflow: visible;
	}
	.grid {
		stroke: var(--color-border-subtle);
		stroke-width: 1;
	}
	.grid.zero {
		stroke: var(--color-border-strong);
	}
	.tick {
		font-family: var(--font-mono);
		font-size: 9.5px;
		fill: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
	}
	.cross {
		stroke: var(--color-border-strong);
		stroke-width: 1;
	}
	.bar {
		fill: var(--color-primary);
		transition: opacity 120ms;
	}
	.bar.s1 {
		fill: color-mix(in srgb, var(--color-foreground) 60%, transparent);
	}
	.bar.s2 {
		fill: var(--color-foreground-subtle);
	}
	.bar.dim {
		opacity: 0.45;
	}
	.line {
		fill: none;
		stroke: var(--color-primary);
		stroke-width: 2;
		stroke-linejoin: round;
		stroke-linecap: round;
	}
	.line.s1 {
		stroke: color-mix(in srgb, var(--color-foreground) 60%, transparent);
	}
	.line.s2 {
		stroke: var(--color-foreground-subtle);
		stroke-dasharray: 4 3;
	}
	.dot {
		fill: var(--color-primary);
		stroke: var(--color-background);
		stroke-width: 2;
	}
	.dot.s1 {
		fill: color-mix(in srgb, var(--color-foreground) 60%, transparent);
	}
	.dot.s2 {
		fill: var(--color-foreground-subtle);
	}
</style>
