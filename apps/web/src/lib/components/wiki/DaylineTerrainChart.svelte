<!--
	DaylineTerrainChart.svelte — Experimental single-line-with-fill terrain chart

	One continuous line through 24 hours:
	- Sleep phase (12am–wake): gentle oscillation showing sleep cycles
	- Waking phase (wake–midnight): novelty z-score as the terrain line
	- Fill between line and baseline encodes autonomic response (thickness = activation)
	- Readiness diamond at the wake transition point

	All mock data — self-contained for prototyping.
-->

<script lang="ts">
	// ── Chart dimensions ────────────────────────────────────────
	const MARGIN = { top: 28, right: 16, bottom: 28, left: 40 };
	const WIDTH = 840;
	const HEIGHT = 360;
	const PLOT_W = WIDTH - MARGIN.left - MARGIN.right;
	const PLOT_H = HEIGHT - MARGIN.top - MARGIN.bottom;
	const Y_MAX = 3;

	function yToSvg(z: number): number {
		const clamped = Math.max(-Y_MAX, Math.min(Y_MAX, z));
		return MARGIN.top + ((Y_MAX - clamped) / (2 * Y_MAX)) * PLOT_H;
	}

	function hourToX(hour: number): number {
		return MARGIN.left + (hour / 24) * PLOT_W;
	}

	function formatHourLabel(hour: number): string {
		if (hour === 0 || hour === 24) return "12am";
		if (hour === 12) return "12pm";
		if (hour < 12) return `${hour}am`;
		return `${hour - 12}pm`;
	}

	const HOUR_TICKS = [0, 3, 6, 9, 12, 15, 18, 21, 24];
	const Y_TICKS = [-3, -2, -1, 0, 1, 2, 3];
	const BASELINE_Y = yToSvg(0);

	// ── Mock data ───────────────────────────────────────────────

	// Sleep phases (12am–6:30am CST = hours 0–6.5)
	// Each phase: { start (hour), end (hour), stage: 'light'|'deep'|'rem'|'awake' }
	const sleepPhases = [
		{ start: 0, end: 0.17, stage: "awake" },      // settling in
		{ start: 0.17, end: 0.75, stage: "light" },    // cycle 1 light
		{ start: 0.75, end: 1.33, stage: "deep" },     // cycle 1 deep
		{ start: 1.33, end: 1.5, stage: "light" },     // transition
		{ start: 1.5, end: 1.75, stage: "rem" },       // cycle 1 REM (short)
		{ start: 1.75, end: 1.8, stage: "awake" },     // brief wake
		{ start: 1.8, end: 2.25, stage: "light" },     // cycle 2 light
		{ start: 2.25, end: 2.75, stage: "deep" },     // cycle 2 deep
		{ start: 2.75, end: 3.0, stage: "light" },     // transition
		{ start: 3.0, end: 3.5, stage: "rem" },        // cycle 2 REM
		{ start: 3.5, end: 3.55, stage: "awake" },     // brief wake
		{ start: 3.55, end: 4.0, stage: "light" },     // cycle 3 light
		{ start: 4.0, end: 4.25, stage: "deep" },      // cycle 3 deep (shorter)
		{ start: 4.25, end: 4.5, stage: "light" },     // transition
		{ start: 4.5, end: 5.17, stage: "rem" },       // cycle 3 REM (longer)
		{ start: 5.17, end: 5.2, stage: "awake" },     // brief wake
		{ start: 5.2, end: 5.5, stage: "light" },      // cycle 4 light
		{ start: 5.5, end: 5.67, stage: "deep" },      // cycle 4 deep (minimal)
		{ start: 5.67, end: 5.83, stage: "light" },    // transition
		{ start: 5.83, end: 6.33, stage: "rem" },      // cycle 4 REM (longest)
		{ start: 6.33, end: 6.5, stage: "awake" },     // waking up
	];

	const WAKE_HOUR = 6.5;
	const READINESS = 68; // 0-100

	// Waking events (same as demo day Feb 13, CST hours)
	const wakingEvents = [
		{ hour: 6.75, label: "Morning routine",       noveltyZ: -2.06, autonomicZ: -0.51 },
		{ hour: 7.5,  label: "Bike commute",          noveltyZ: -1.35, autonomicZ: 1.78 },
		{ hour: 8.0,  label: "Coffee and Slack",      noveltyZ: -1.45, autonomicZ: -0.46 },
		{ hour: 8.625,label: "Design standup",         noveltyZ: 1.44,  autonomicZ: -0.13 },
		{ hour: 10.25,label: "Focused design work",   noveltyZ: 0.09,  autonomicZ: -0.64 },
		{ hour: 12.0, label: "Lunch with Maya",        noveltyZ: 0.86,  autonomicZ: -0.05 },
		{ hour: 13.375,label: "User research session", noveltyZ: 0.75,  autonomicZ: 0.09 },
		{ hour: 14.625,label: "Drive to house showing",noveltyZ: -0.81, autonomicZ: -0.36 },
		{ hour: 15.375,label: "House showing",         noveltyZ: 0.48,  autonomicZ: 0.17 },
		{ hour: 16.0, label: "Unknown",                noveltyZ: 0,     autonomicZ: 0,    isUnknown: true },
		{ hour: 16.5, label: "Drive home",             noveltyZ: -0.50, autonomicZ: -0.40 },
		{ hour: 17.125,label: "Run",                   noveltyZ: -0.32, autonomicZ: 3.0 },
		{ hour: 17.875,label: "Dinner prep",           noveltyZ: -0.23, autonomicZ: -0.26 },
		{ hour: 18.875,label: "Dinner and TV",         noveltyZ: -0.57, autonomicZ: -0.60 },
		{ hour: 20.25,label: "Reading",                noveltyZ: -0.32, autonomicZ: -0.77 },
	];

	// ── Sleep curve: map phases to Y values ─────────────────────
	// deep = -2σ, light = -0.8σ, rem = -0.3σ, awake = +0.3σ
	const STAGE_Y: Record<string, number> = {
		deep: -2.0,
		light: -0.8,
		rem: -0.3,
		awake: 0.3,
	};

	// Build sleep points (sample at phase boundaries + midpoints for smoothness)
	interface ChartPoint {
		hour: number;
		x: number;
		noveltyY: number;    // the terrain line (sleep phase or novelty z)
		autonomicY: number;  // the fill boundary (autonomic z)
		label: string;
		isSleep: boolean;
		isUnknown: boolean;
	}

	function buildChartPoints(): ChartPoint[] {
		const points: ChartPoint[] = [];

		// Sleep points
		for (const phase of sleepPhases) {
			const stageZ = STAGE_Y[phase.stage] ?? 0;
			const mid = (phase.start + phase.end) / 2;
			// Add start and midpoint for smoothness
			points.push({
				hour: phase.start,
				x: hourToX(phase.start),
				noveltyY: yToSvg(stageZ),
				autonomicY: yToSvg(stageZ), // during sleep, autonomic = same as terrain
				label: phase.stage,
				isSleep: true,
				isUnknown: false,
			});
			if (phase.end - phase.start > 0.3) {
				points.push({
					hour: mid,
					x: hourToX(mid),
					noveltyY: yToSvg(stageZ),
					autonomicY: yToSvg(stageZ),
					label: phase.stage,
					isSleep: true,
					isUnknown: false,
				});
			}
		}

		// Readiness transition point (wake)
		const readinessZ = ((READINESS - 50) / 50) * Y_MAX;
		points.push({
			hour: WAKE_HOUR,
			x: hourToX(WAKE_HOUR),
			noveltyY: yToSvg(readinessZ),
			autonomicY: yToSvg(readinessZ),
			label: `Readiness ${READINESS}%`,
			isSleep: false,
			isUnknown: false,
		});

		// Waking event points
		for (const evt of wakingEvents) {
			points.push({
				hour: evt.hour,
				x: hourToX(evt.hour),
				noveltyY: yToSvg(evt.noveltyZ),
				autonomicY: yToSvg(evt.autonomicZ),
				label: evt.label,
				isSleep: false,
				isUnknown: evt.isUnknown ?? false,
			});
		}

		return points;
	}

	const chartPoints = buildChartPoints();

	// ── Curve building ──────────────────────────────────────────
	// Compute tangents for cubic bezier (central difference)
	function computeTangents(pts: { x: number; y: number }[]): { tx: number; ty: number }[] {
		return pts.map((_, i) => {
			if (pts.length < 2) return { tx: 0, ty: 0 };
			if (i === 0) return { tx: pts[1].x - pts[0].x, ty: pts[1].y - pts[0].y };
			if (i === pts.length - 1) return { tx: pts[i].x - pts[i - 1].x, ty: pts[i].y - pts[i - 1].y };
			return { tx: (pts[i + 1].x - pts[i - 1].x) / 2, ty: (pts[i + 1].y - pts[i - 1].y) / 2 };
		});
	}

	// Build a full SVG path from points
	function buildCurvePath(pts: { x: number; y: number }[]): string {
		if (pts.length < 2) return "";
		const tan = computeTangents(pts);
		const TENSION = 1 / 3;
		let d = `M ${pts[0].x},${pts[0].y}`;
		for (let i = 0; i < pts.length - 1; i++) {
			const a = pts[i], b = pts[i + 1];
			const cp1x = a.x + tan[i].tx * TENSION;
			const cp1y = a.y + tan[i].ty * TENSION;
			const cp2x = b.x - tan[i + 1].tx * TENSION;
			const cp2y = b.y - tan[i + 1].ty * TENSION;
			d += ` C ${cp1x},${cp1y} ${cp2x},${cp2y} ${b.x},${b.y}`;
		}
		return d;
	}

	// Build the fill area between novelty line and autonomic line
	function buildFillPath(pts: ChartPoint[]): string {
		if (pts.length < 2) return "";
		const noveltyPts = pts.map(p => ({ x: p.x, y: p.noveltyY }));
		const autonomicPts = pts.map(p => ({ x: p.x, y: p.autonomicY }));

		// Forward path along novelty line
		const noveltyTan = computeTangents(noveltyPts);
		const autonomicTan = computeTangents(autonomicPts);
		const TENSION = 1 / 3;

		let d = `M ${noveltyPts[0].x},${noveltyPts[0].y}`;
		for (let i = 0; i < noveltyPts.length - 1; i++) {
			const a = noveltyPts[i], b = noveltyPts[i + 1];
			const cp1x = a.x + noveltyTan[i].tx * TENSION;
			const cp1y = a.y + noveltyTan[i].ty * TENSION;
			const cp2x = b.x - noveltyTan[i + 1].tx * TENSION;
			const cp2y = b.y - noveltyTan[i + 1].ty * TENSION;
			d += ` C ${cp1x},${cp1y} ${cp2x},${cp2y} ${b.x},${b.y}`;
		}

		// Reverse path along autonomic line (bottom of fill)
		const last = autonomicPts.length - 1;
		d += ` L ${autonomicPts[last].x},${autonomicPts[last].y}`;
		for (let i = last; i > 0; i--) {
			const a = autonomicPts[i], b = autonomicPts[i - 1];
			const cp1x = a.x - autonomicTan[i].tx * TENSION;
			const cp1y = a.y - autonomicTan[i].ty * TENSION;
			const cp2x = b.x + autonomicTan[i - 1].tx * TENSION;
			const cp2y = b.y + autonomicTan[i - 1].ty * TENSION;
			d += ` C ${cp1x},${cp1y} ${cp2x},${cp2y} ${b.x},${b.y}`;
		}

		d += " Z";
		return d;
	}

	// Split points into sleep and waking for different rendering
	const sleepPoints = chartPoints.filter(p => p.isSleep);
	const wakeTransitionIdx = chartPoints.findIndex(p => !p.isSleep);
	const wakingPoints = chartPoints.slice(wakeTransitionIdx);

	// Novelty line path (waking only — the terrain)
	const noveltyPath = buildCurvePath(wakingPoints.map(p => ({ x: p.x, y: p.noveltyY })));

	// Sleep line path
	const sleepPath = buildCurvePath(sleepPoints.map(p => ({ x: p.x, y: p.noveltyY })));

	// Fill between novelty and autonomic (waking only)
	const fillPath = buildFillPath(wakingPoints);

	// Autonomic line path (waking only — for reference)
	const autonomicPath = buildCurvePath(wakingPoints.map(p => ({ x: p.x, y: p.autonomicY })));

	// ── Crosshair ───────────────────────────────────────────────
	let svgEl: SVGSVGElement | undefined = $state();
	let hoverX = $state<number | null>(null);

	function handleMouseMove(e: MouseEvent) {
		if (!svgEl) return;
		const rect = svgEl.getBoundingClientRect();
		const scaleX = WIDTH / rect.width;
		const svgX = (e.clientX - rect.left) * scaleX;
		if (svgX < MARGIN.left || svgX > MARGIN.left + PLOT_W) { hoverX = null; return; }
		hoverX = svgX;
	}

	function xToHour(x: number): number {
		return ((x - MARGIN.left) / PLOT_W) * 24;
	}

	const hoverEvent = $derived(() => {
		if (hoverX === null) return null;
		const hour = xToHour(hoverX);
		// Check sleep phases
		for (const phase of sleepPhases) {
			if (hour >= phase.start && hour < phase.end) {
				return { label: phase.stage.charAt(0).toUpperCase() + phase.stage.slice(1) + " sleep", isSleep: true, hour: (phase.start + phase.end) / 2, noveltyZ: STAGE_Y[phase.stage], autonomicZ: null };
			}
		}
		// Check waking events
		let closest = wakingEvents[0];
		let minDist = Infinity;
		for (const evt of wakingEvents) {
			const dist = Math.abs(hour - evt.hour);
			if (dist < minDist) { minDist = dist; closest = evt; }
		}
		if (closest) return { label: closest.label, isSleep: false, hour: closest.hour, noveltyZ: closest.noveltyZ, autonomicZ: closest.autonomicZ };
		return null;
	});

	// Wake point for readiness diamond
	const wakeX = hourToX(WAKE_HOUR);
	const readinessZ = ((READINESS - 50) / 50) * Y_MAX;
	const wakeY = yToSvg(readinessZ);
</script>

<div class="terrain-container">
	<div class="terrain-title">Terrain (experimental)</div>

	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<svg
		bind:this={svgEl}
		viewBox="0 0 {WIDTH} {HEIGHT}"
		preserveAspectRatio="xMidYMid meet"
		class="terrain-svg"
		onmousemove={handleMouseMove}
		onmouseleave={() => hoverX = null}
	>
		<!-- Plot background -->
		<rect x={MARGIN.left} y={MARGIN.top} width={PLOT_W} height={PLOT_H}
			fill="var(--color-surface, #fafafa)" rx="2" />

		<!-- Y-axis grid lines -->
		{#each Y_TICKS as z}
			{@const y = yToSvg(z)}
			{#if z === 0}
				<line x1={MARGIN.left} y1={y} x2={MARGIN.left + PLOT_W} y2={y}
					stroke="var(--color-foreground-muted, #999)" stroke-width="0.5" stroke-dasharray="4,4" stroke-opacity="0.3" />
			{:else}
				<line x1={MARGIN.left} y1={y} x2={MARGIN.left + PLOT_W} y2={y}
					stroke="var(--color-border, #e5e5e5)" stroke-width="0.5" stroke-opacity="0.3" />
			{/if}
		{/each}

		<!-- X-axis labels -->
		{#each HOUR_TICKS as hour}
			{@const x = hourToX(hour)}
			{#if hour > 0 && hour < 24}
				<line x1={x} y1={MARGIN.top} x2={x} y2={MARGIN.top + PLOT_H}
					stroke="var(--color-border, #e5e5e5)" stroke-width="0.5" stroke-opacity="0.3" />
			{/if}
			<text x={x} y={MARGIN.top + PLOT_H + 16}
				text-anchor={hour === 0 ? "start" : hour === 24 ? "end" : "middle"}
				class="axis-label">{formatHourLabel(hour)}</text>
		{/each}

		<!-- Sleep region background tint -->
		<rect x={MARGIN.left} y={MARGIN.top} width={wakeX - MARGIN.left} height={PLOT_H}
			fill="var(--color-primary, #4f46e5)" fill-opacity="0.03" />

		<!-- Fill between novelty and autonomic (waking portion) -->
		<path d={fillPath} fill="var(--color-primary, #4f46e5)" fill-opacity="0.08" />

		<!-- Sleep phase curve -->
		<path d={sleepPath} fill="none"
			stroke="var(--color-foreground-muted, #888)" stroke-width="1.5"
			stroke-opacity="0.4" stroke-linecap="round" />

		<!-- Sleep phase dots (at stage transitions) -->
		{#each sleepPhases as phase}
			{@const stageZ = STAGE_Y[phase.stage]}
			{@const cx = hourToX((phase.start + phase.end) / 2)}
			{@const cy = yToSvg(stageZ)}
			{#if phase.stage === "deep"}
				<circle {cx} {cy} r="2" fill="var(--color-primary, #4f46e5)" opacity="0.5" />
			{:else if phase.stage === "rem"}
				<circle {cx} {cy} r="2" fill="var(--color-foreground-muted, #888)" opacity="0.4" />
			{/if}
		{/each}

		<!-- Novelty line (waking — primary terrain) -->
		<path d={noveltyPath} fill="none"
			stroke="var(--color-primary, #4f46e5)" stroke-width="2"
			stroke-linecap="round" />

		<!-- Autonomic line (waking — subtle) -->
		<path d={autonomicPath} fill="none"
			stroke="var(--color-foreground-muted, #888)" stroke-width="1"
			stroke-linecap="round" stroke-opacity="0.5" />

		<!-- Waking event dots on novelty line -->
		{#each wakingEvents as evt}
			{@const cx = hourToX(evt.hour)}
			{@const cy = yToSvg(evt.noveltyZ)}
			{@const isTopNovel = Math.abs(evt.noveltyZ) === Math.max(...wakingEvents.filter(e => !e.isUnknown).map(e => Math.abs(e.noveltyZ)))}
			{#if evt.isUnknown}
				<circle {cx} {cy} r="3" fill="var(--color-background, #fff)"
					stroke="var(--color-foreground-subtle, #aaa)" stroke-width="1" stroke-dasharray="2,2" />
			{:else}
				<circle {cx} {cy} r={isTopNovel ? 5.5 : 3.5}
					fill="var(--color-primary, #4f46e5)"
					stroke="var(--color-background, #fff)" stroke-width={isTopNovel ? 2 : 1.5}
					opacity={isTopNovel ? 1 : 0.8} />
			{/if}
		{/each}

		<!-- Wake/readiness diamond -->
		<g transform="translate({wakeX}, {wakeY})">
			<polygon points="0,-7 6,0 0,7 -6,0"
				fill="var(--color-primary, #4f46e5)"
				stroke="var(--color-background, #fff)" stroke-width="1.5" />
		</g>
		<!-- Wake time vertical line (subtle) -->
		<line x1={wakeX} y1={MARGIN.top} x2={wakeX} y2={MARGIN.top + PLOT_H}
			stroke="var(--color-primary, #4f46e5)" stroke-width="0.5" stroke-opacity="0.2" stroke-dasharray="2,4" />

		<!-- Sleep stage labels (inside plot, left side) -->
		<text x={MARGIN.left + 6} y={yToSvg(-2.0) + 3} class="sleep-stage-label">Deep</text>
		<text x={MARGIN.left + 6} y={yToSvg(-0.8) + 3} class="sleep-stage-label">Light</text>
		<text x={MARGIN.left + 6} y={yToSvg(-0.3) + 3} class="sleep-stage-label">REM</text>

		<!-- Waking labels -->
		<text x={MARGIN.left + PLOT_W - 6} y={MARGIN.top + 12} text-anchor="end" class="semantic-label primary-color">Novel</text>
		<text x={MARGIN.left + PLOT_W - 6} y={MARGIN.top + PLOT_H - 6} text-anchor="end" class="semantic-label primary-color">Routine</text>

		<!-- Readiness label -->
		<text x={wakeX + 8} y={wakeY - 10} class="readiness-label">
			{READINESS}%
		</text>

		<!-- Plot border -->
		<rect x={MARGIN.left} y={MARGIN.top} width={PLOT_W} height={PLOT_H}
			fill="none" stroke="var(--color-border, #e5e5e5)" stroke-width="0.75" rx="2" />

		<!-- Crosshair -->
		{#if hoverX !== null}
			{@const evt = hoverEvent()}
			<line x1={hoverX} y1={MARGIN.top} x2={hoverX} y2={MARGIN.top + PLOT_H}
				stroke="var(--color-foreground-subtle, #aaa)" stroke-width="0.75" stroke-dasharray="3,3" />
			{#if evt}
				{@const tipX = hoverX > MARGIN.left + PLOT_W / 2 ? hoverX - 8 : hoverX + 8}
				{@const tipAnchor = hoverX > MARGIN.left + PLOT_W / 2 ? "end" : "start"}
				<text x={tipX} y={MARGIN.top + 14} text-anchor={tipAnchor} class="crosshair-label">
					{evt.label}
				</text>
				{#if !evt.isSleep && evt.autonomicZ !== null}
					<text x={tipX} y={MARGIN.top + 28} text-anchor={tipAnchor} class="crosshair-score primary-color">
						Novelty {evt.noveltyZ >= 0 ? "+" : ""}{evt.noveltyZ.toFixed(1)}σ
					</text>
					<text x={tipX} y={MARGIN.top + 40} text-anchor={tipAnchor} class="crosshair-score muted-color">
						Autonomic {evt.autonomicZ >= 0 ? "+" : ""}{evt.autonomicZ.toFixed(1)}σ
					</text>
				{/if}
			{/if}
		{/if}
	</svg>
</div>

<style>
	.terrain-container {
		width: 100%;
		position: relative;
		margin-top: 0.5rem;
	}

	.terrain-title {
		font-size: 0.6875rem;
		font-weight: 500;
		color: var(--color-foreground-subtle);
		letter-spacing: 0.02em;
		margin-bottom: 0.25rem;
		opacity: 0.5;
	}

	.terrain-svg {
		width: 100%;
		height: auto;
		cursor: crosshair;
	}

	.axis-label {
		font-size: 9px;
		font-weight: 400;
		fill: var(--color-foreground-subtle, #999);
		font-family: var(--font-mono, monospace);
	}

	.sleep-stage-label {
		font-size: 7px;
		font-weight: 500;
		fill: var(--color-foreground-subtle, #999);
		font-family: var(--font-sans, system-ui, sans-serif);
		opacity: 0.5;
	}

	.semantic-label {
		font-size: 9px;
		font-weight: 500;
		font-family: var(--font-sans, system-ui, sans-serif);
		letter-spacing: 0.02em;
		opacity: 0.7;
	}

	.primary-color {
		fill: var(--color-primary, #4f46e5);
	}

	.muted-color {
		fill: var(--color-foreground-muted, #888);
	}

	.readiness-label {
		font-size: 9px;
		font-weight: 600;
		fill: var(--color-primary, #4f46e5);
		font-family: var(--font-sans, system-ui, sans-serif);
	}

	.crosshair-label {
		font-size: 9px;
		font-weight: 600;
		fill: var(--color-foreground, #333);
		font-family: var(--font-sans, system-ui, sans-serif);
	}

	.crosshair-score {
		font-size: 8px;
		font-weight: 500;
		font-family: var(--font-mono, monospace);
	}
</style>
