<script lang="ts">
	import type { TimelineDayLocationChunk } from "$lib/wiki/api";

	interface Props {
		visits: TimelineDayLocationChunk[];
		dayDate: string; // YYYY-MM-DD
		height?: number;
		/** Bound: epoch ms of cursor position, null when not hovering. */
		hoverTimeMs?: number | null;
	}

	let {
		visits,
		dayDate,
		height = 28,
		hoverTimeMs = $bindable(null),
	}: Props = $props();

	let containerEl: HTMLDivElement | null = $state(null);
	let containerWidth = $state(0);

	$effect(() => {
		if (!containerEl) return;
		const ro = new ResizeObserver((entries) => {
			containerWidth = entries[0]?.contentRect?.width ?? 0;
		});
		ro.observe(containerEl);
		return () => ro.disconnect();
	});

	// Day boundaries: 00:00 → 24:00 of dayDate, in epoch ms (local time).
	const dayStartMs = $derived.by(() => {
		const [y, m, d] = dayDate.split("-").map(Number);
		return new Date(y, m - 1, d, 0, 0, 0, 0).getTime();
	});
	const dayEndMs = $derived(dayStartMs + 24 * 60 * 60 * 1000);
	const dayDurationMs = $derived(dayEndMs - dayStartMs);

	// Subtle place band — one muted segment per visit, no colors, no labels.
	type Band = { x: number; width: number };
	const bands = $derived.by<Band[]>(() => {
		if (!containerWidth || dayDurationMs <= 0) return [];
		return visits
			.map((v): Band | null => {
				const startMs = Date.parse(v.start_time);
				const endMs = Date.parse(v.end_time);
				if (!Number.isFinite(startMs) || !Number.isFinite(endMs))
					return null;
				const a = Math.max(startMs, dayStartMs);
				const b = Math.min(endMs, dayEndMs);
				if (b <= a) return null;
				const x = ((a - dayStartMs) / dayDurationMs) * containerWidth;
				const width =
					((b - a) / dayDurationMs) * containerWidth;
				return { x, width: Math.max(width, 2) };
			})
			.filter((b): b is Band => b !== null);
	});

	// Hour ticks (12-hour labels)
	const TICK_HOURS = [
		{ hour: 6, label: "6am" },
		{ hour: 12, label: "12pm" },
		{ hour: 18, label: "6pm" },
	];
	const ticks = $derived(
		TICK_HOURS.map((t) => ({
			hour: t.hour,
			label: t.label,
			x: containerWidth * (t.hour / 24),
		})),
	);

	// Hover state — local x position for the vertical line + tooltip
	let hoverX = $state<number | null>(null);

	function fmtTime(ms: number): string {
		return new Date(ms).toLocaleTimeString([], {
			hour: "numeric",
			minute: "2-digit",
			hour12: true,
		});
	}

	// If the cursor is inside a visit window, show the place name in the tooltip.
	function visitAt(ms: number): TimelineDayLocationChunk | null {
		for (const v of visits) {
			const a = Date.parse(v.start_time);
			const b = Date.parse(v.end_time);
			if (ms >= a && ms <= b) return v;
		}
		return null;
	}

	const tooltip = $derived.by(() => {
		if (hoverTimeMs == null) return null;
		const visit = visitAt(hoverTimeMs);
		const time = fmtTime(hoverTimeMs);
		if (visit?.place_name) return `${time} · ${visit.place_name}`;
		return time;
	});

	function onMouseMove(e: MouseEvent) {
		if (!containerEl || !containerWidth) return;
		const rect = containerEl.getBoundingClientRect();
		const x = Math.max(0, Math.min(containerWidth, e.clientX - rect.left));
		hoverX = x;
		hoverTimeMs = dayStartMs + (x / containerWidth) * dayDurationMs;
	}

	function onMouseLeave() {
		hoverX = null;
		hoverTimeMs = null;
	}
</script>

<div
	class="day-location-timeline"
	bind:this={containerEl}
	style="height: {height}px;"
	onmousemove={onMouseMove}
	onmouseleave={onMouseLeave}
	role="presentation"
>
	{#if containerWidth > 0}
		<svg
			class="canvas"
			width={containerWidth}
			{height}
			viewBox="0 0 {containerWidth} {height}"
		>
			<!-- Axis line -->
			<line
				x1="0"
				y1={height / 2}
				x2={containerWidth}
				y2={height / 2}
				class="axis"
			/>

			<!-- Subtle place band: one segment per visit, single muted color -->
			{#each bands as band, i (i)}
				<rect
					x={band.x}
					y={height / 2 - 2}
					width={band.width}
					height="4"
					rx="1"
					class="band"
				/>
			{/each}

			<!-- Hour ticks at 6am / 12pm / 6pm -->
			{#each ticks as tick (tick.hour)}
				<line
					x1={tick.x}
					y1={height / 2 - 5}
					x2={tick.x}
					y2={height / 2 + 5}
					class="tick"
				/>
				<text
					x={tick.x + 3}
					y={height - 2}
					class="tick-label"
				>
					{tick.label}
				</text>
			{/each}

			<!-- Hover scrub line -->
			{#if hoverX != null}
				<line
					x1={hoverX}
					y1="0"
					x2={hoverX}
					y2={height}
					class="scrub-line"
				/>
			{/if}
		</svg>

		<!-- Tooltip -->
		{#if hoverX != null && tooltip}
			<div
				class="tooltip"
				style="left: {hoverX}px;"
			>
				{tooltip}
			</div>
		{/if}
	{/if}
</div>

<style>
	.day-location-timeline {
		width: 100%;
		position: relative;
		margin-bottom: 12px;
		cursor: crosshair;
	}

	.canvas {
		display: block;
	}

	.axis {
		stroke: var(--color-border-subtle);
		stroke-width: 1;
	}

	.band {
		fill: var(--color-foreground-muted);
		opacity: 0.35;
	}

	.tick {
		stroke: var(--color-foreground-muted);
		stroke-width: 1;
		opacity: 0.4;
	}

	.tick-label {
		font-size: 9px;
		font-family: var(--font-mono, monospace);
		fill: var(--color-foreground-muted);
	}

	.scrub-line {
		stroke: var(--color-primary);
		stroke-width: 1;
		opacity: 0.7;
		pointer-events: none;
	}

	.tooltip {
		position: absolute;
		bottom: calc(100% + 4px);
		transform: translateX(-50%);
		background: var(--color-surface-overlay, var(--color-surface));
		color: var(--color-foreground);
		font-size: 11px;
		padding: 3px 6px;
		border-radius: 3px;
		border: 1px solid var(--color-border-subtle);
		white-space: nowrap;
		pointer-events: none;
		box-shadow: 0 2px 6px rgba(0, 0, 0, 0.08);
	}
</style>
