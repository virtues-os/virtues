<!--
	DaylineChart.svelte — Shape of Your Day

	SVG chart showing dayline metrics over a 24-hour period.
	X-axis: time (midnight to midnight, left to right)
	Y-axis: novelty z-score (negative = routine, positive = novel)
	Midline: dotted baseline at y=0 (the "normal" line)

	Pill selector at top for switching metric views (only Dayline in V1).
-->

<script lang="ts">
	import { onMount, onDestroy } from "svelte";
	import { browser } from "$app/environment";
	import { getLocalDateSlug } from "$lib/utils/dateUtils";
	import type { DayEvent, ScoredSleepCycle } from "$lib/wiki/types";
	import type { TimelineDayLocationChunk } from "$lib/wiki/api";
	import MovementMap from "$lib/components/timeline/MovementMap.svelte";
	import DayLocationTimeline from "$lib/components/timeline/DayLocationTimeline.svelte";
	import DaylineStrip from "./DaylineStrip.svelte";

	interface Props {
		events: DayEvent[];
		timezone: string | null;
		pageDate?: Date;
		readinessScore?: number | null;
		sleepCycles?: ScoredSleepCycle[];
		// Location data
		movementStops?: TimelineDayLocationChunk[];
		movementTrack?: { lat: number; lng: number; timeMs: number }[];
		dedupedMarkers?: { lat: number; lng: number; label: string; timeMs: number; placeId: string | null }[];
		dayDateSlug?: string;
		hasLocationData?: boolean;
	}

	let {
		events, timezone, pageDate, readinessScore, sleepCycles = [],
		movementStops = [], movementTrack = [], dedupedMarkers = [],
		dayDateSlug = "", hasLocationData = false,
	}: Props = $props();

	// Shared hover state for location timeline ↔ map sync
	let movementHoverTimeMs = $state<number | null>(null);

	// ── Live clock (updates every second for the "now" marker) ──
	let nowTime = $state(new Date());
	let clockInterval: ReturnType<typeof setInterval> | null = null;

	const isToday = $derived.by(() => {
		if (!pageDate || !browser) return false;
		return getLocalDateSlug(pageDate) === getLocalDateSlug(new Date());
	});

	onMount(() => {
		clockInterval = setInterval(() => {
			nowTime = new Date();
		}, 1000);
	});

	onDestroy(() => {
		if (clockInterval) clearInterval(clockInterval);
	});

	// ── Chart dimensions ────────────────────────────────────────
	// SVG viewBox coordinates (not pixels — scales responsively)
	const MARGIN = { top: 32, right: 16, bottom: 28, left: 40 };
	const WIDTH = 840;
	const HEIGHT = 360; // ~21:9 aspect ratio
	const PLOT_W = WIDTH - MARGIN.left - MARGIN.right;
	const PLOT_H = HEIGHT - MARGIN.top - MARGIN.bottom;

	// ── Y-axis: novelty z-score ─────────────────────────────────
	const Y_MAX = 3; // ±3 standard deviations
	const Y_TICKS = [-3, -2, -1, 0, 1, 2, 3];

	function yToSvg(z: number): number {
		// z = +3 → top of plot, z = -3 → bottom of plot
		const clamped = Math.max(-Y_MAX, Math.min(Y_MAX, z));
		const pct = (Y_MAX - clamped) / (2 * Y_MAX); // 0 at top, 1 at bottom
		return MARGIN.top + pct * PLOT_H;
	}

	// const MIDLINE_Y = yToSvg(0); // available if needed

	// ── X-axis: 24-hour time ────────────────────────────────────
	const HOUR_TICKS = [0, 3, 6, 9, 12, 15, 18, 21, 24];

	function hourToX(hour: number): number {
		return MARGIN.left + (hour / 24) * PLOT_W;
	}

	function formatHourLabel(hour: number): string {
		if (hour === 0 || hour === 24) return "12am";
		if (hour === 12) return "12pm";
		if (hour < 12) return `${hour}am`;
		return `${hour - 12}pm`;
	}

	// ── Y-axis label formatting ─────────────────────────────────
	function formatYLabel(z: number): string {
		if (z === 0) return "0";
		if (z > 0) return `+${z}σ`;
		return `${z}σ`;
	}

	// ── Event data points ──────────────────────────────────────
	// Each event becomes a dot at its midpoint time (X) and novelty_z (Y).
	// Sleep, hidden, and unknown events are excluded.

	function getHourOfDay(date: Date): number {
		if (timezone) {
			const fmt = new Intl.DateTimeFormat("en-US", {
				timeZone: timezone,
				hourCycle: "h23",
				hour: "2-digit",
				minute: "2-digit",
			});
			const parts = fmt.formatToParts(date);
			const h = parseInt(
				parts.find((p) => p.type === "hour")?.value || "0",
			);
			const m = parseInt(
				parts.find((p) => p.type === "minute")?.value || "0",
			);
			return h + m / 60;
		}
		return date.getHours() + date.getMinutes() / 60;
	}

	interface SubDot {
		name: string;
		rawId: string; // original entity_id (for threading same entities across events)
		z: number; // novelty z-score
		y: number; // SVG y coordinate
		kind: "topic" | "entity";
		/** X offset from event center (hours). Used for entities with known timestamps. */
		xHourOverride: number | null;
	}

	interface EventPoint {
		id: string;
		x: number; // SVG x coordinate
		y: number; // SVG y coordinate (novelty, or autonomicZ for sleep)
		autonomicY: number | null; // SVG y coordinate (autonomic, null if no data)
		noveltyZ: number;
		autonomicZ: number | null;
		isSleep: boolean;
		label: string;
		startHour: number;
		endHour: number;
		isUnknown: boolean;
		subDots: SubDot[];
	}

	const eventPoints = $derived.by<EventPoint[]>(() => {
		// Exclude sleep and hidden from the novelty curve — sleep only appears on autonomic
		const sorted = events
			.filter((e) => !e.userHidden && !e.isSleep)
			.sort((a, b) => a.startTime.getTime() - b.startTime.getTime());

		// First pass: create points with raw noveltyZ (null for unknown)
		const raw = sorted.map((e) => {
			const startHour = getHourOfDay(e.startTime);
			const endHour = getHourOfDay(e.endTime);
			const midHour = (startHour + endHour) / 2;
			const isUnknown = e.isUnknown ?? false;

			// Build sub-dots from topic/entity novelty scores
			const subDots: SubDot[] = [];
			if (e.topicNovelty) {
				for (const [name, z] of Object.entries(e.topicNovelty)) {
					// Topics stay at event center — no per-topic timestamp exists
					subDots.push({
						name,
						rawId: name,
						z,
						y: yToSvg(z),
						kind: "topic",
						xHourOverride: null,
					});
				}
			}
			if (e.entityNovelty) {
				for (const [entityId, z] of Object.entries(e.entityNovelty)) {
					// Clean up entity ID for display: "person_demo_maya" → "Maya"
					const displayName = entityId
						.replace(/^(person|place|org)_demo_/, "")
						.replace(/_/g, " ")
						.replace(/\b\w/g, (c) => c.toUpperCase());
					// Use entity-specific timestamp if available
					const tsIso = e.entityTimestamps?.[entityId];
					const xHourOverride = tsIso
						? getHourOfDay(new Date(tsIso))
						: null;
					subDots.push({
						name: displayName,
						rawId: entityId,
						z,
						y: yToSvg(z),
						kind: "entity",
						xHourOverride,
					});
				}
			}

			return {
				id: e.id,
				noveltyZ: isUnknown ? null : (e.noveltyZ ?? 0),
				autonomicZ: isUnknown ? null : (e.autonomicZ ?? null),
				isSleep: false,
				label: isUnknown ? "Unknown" : (e.eventSummary ?? e.autoLabel),
				startHour,
				endHour,
				midHour,
				isUnknown,
				subDots,
			};
		});

		// Second pass: interpolate unknown events between known neighbors
		return raw.map((p, i) => {
			let nz: number;
			if (p.noveltyZ !== null) {
				nz = p.noveltyZ;
			} else {
				// Find previous known value
				let prev = 0;
				for (let j = i - 1; j >= 0; j--) {
					if (raw[j].noveltyZ !== null) {
						prev = raw[j].noveltyZ!;
						break;
					}
				}
				// Find next known value
				let next = 0;
				for (let j = i + 1; j < raw.length; j++) {
					if (raw[j].noveltyZ !== null) {
						next = raw[j].noveltyZ!;
						break;
					}
				}
				nz = (prev + next) / 2;
			}

			return {
				id: p.id,
				x: hourToX(p.midHour),
				y: yToSvg(nz),
				autonomicY: p.autonomicZ !== null ? yToSvg(p.autonomicZ) : null,
				noveltyZ: nz,
				autonomicZ: p.autonomicZ,
				isSleep: p.isSleep,
				label: p.label,
				startHour: p.startHour,
				endHour: p.endHour,
				isUnknown: p.isUnknown,
				subDots: p.subDots,
			};
		});
	});

	// ── Entity threading: connect dots of same entity across events ──
	interface ThreadSegment {
		x1: number;
		y1: number;
		x2: number;
		y2: number;
	}
	interface EntityThread {
		entityId: string;
		segments: ThreadSegment[];
	}

	const entityThreads = $derived.by((): EntityThread[] => {
		const RELATIVE_SCALE = 0.4;
		// Group entity-dot positions by entity_id across all events
		const byEntity = new Map<string, { x: number; y: number }[]>();
		for (const point of eventPoints) {
			if (point.isUnknown) continue;
			for (const dot of point.subDots) {
				if (dot.kind !== "entity") continue;
				const offset = dot.z * RELATIVE_SCALE;
				const dotY = Math.max(
					MARGIN.top,
					Math.min(
						MARGIN.top + PLOT_H,
						point.y - offset * (PLOT_H / (2 * Y_MAX)),
					),
				);
				const dotX =
					dot.xHourOverride !== null
						? hourToX(dot.xHourOverride)
						: point.x;
				let list = byEntity.get(dot.rawId);
				if (!list) {
					list = [];
					byEntity.set(dot.rawId, list);
				}
				list.push({ x: dotX, y: dotY });
			}
		}
		// For each entity appearing 2+ times, build line segments between consecutive dots
		const threads: EntityThread[] = [];
		for (const [entityId, dots] of byEntity.entries()) {
			if (dots.length < 2) continue;
			// Sort by x (chronological)
			dots.sort((a, b) => a.x - b.x);
			const segments: ThreadSegment[] = [];
			for (let i = 0; i < dots.length - 1; i++) {
				segments.push({
					x1: dots[i].x,
					y1: dots[i].y,
					x2: dots[i + 1].x,
					y2: dots[i + 1].y,
				});
			}
			threads.push({ entityId, segments });
		}
		return threads;
	});

	// ── "Now" marker position ───────────────────────────────────
	const nowHour = $derived.by(() => {
		if (!isToday) return null;
		return getHourOfDay(nowTime);
	});

	const nowX = $derived.by(() => {
		const h = nowHour;
		if (h === null) return null;
		return hourToX(h);
	});

	const nowTimeLabel = $derived.by(() => {
		if (!isToday) return "";
		return nowTime.toLocaleTimeString("en-US", {
			hour: "numeric",
			minute: "2-digit",
			second: "2-digit",
			hour12: true,
			timeZone: timezone ?? undefined,
		});
	});

	// ── Curve helpers ──────────────────────────────────────────
	// All chart points: readiness anchor at wake + event points
	const chartPoints = $derived.by(() => {
		const pts = eventPoints;
		if (pts.length === 0) return [];

		// Start from readiness at wake time (or baseline if no readiness/wake)
		const wH = wakeHour;
		const rZ = readinessScore != null ? ((readinessScore - 50) / 50) * Y_MAX : 0;
		const anchorX = wH !== null ? hourToX(wH) : hourToX(0);
		const anchorY = wH !== null ? yToSvg(rZ) : yToSvg(0);

		return [
			{
				x: anchorX,
				y: anchorY,
				isUnknown: false,
				label: wH !== null ? `Readiness ${readinessScore ?? 0}%` : "Start of day",
			},
			...pts,
		];
	});

	// Tangent at each chart point (central difference for interior,
	// forward/backward for endpoints). Gives C1 continuity.
	const chartTangents = $derived.by(() => {
		const pts = chartPoints;
		return pts.map((_, i) => {
			if (pts.length < 2) return { tx: 0, ty: 0 };
			if (i === 0) {
				return {
					tx: pts[1].x - pts[0].x,
					ty: pts[1].y - pts[0].y,
				};
			}
			if (i === pts.length - 1) {
				return {
					tx: pts[i].x - pts[i - 1].x,
					ty: pts[i].y - pts[i - 1].y,
				};
			}
			return {
				tx: (pts[i + 1].x - pts[i - 1].x) / 2,
				ty: (pts[i + 1].y - pts[i - 1].y) / 2,
			};
		});
	});

	// Cubic bezier segment: control points placed at ±1/3 tangent
	function cubicSegmentPath(segIndex: number): string {
		const pts = chartPoints;
		const tan = chartTangents;
		const a = pts[segIndex];
		const b = pts[segIndex + 1];
		const TENSION = 1 / 3;
		const cp1x = a.x + tan[segIndex].tx * TENSION;
		const cp1y = a.y + tan[segIndex].ty * TENSION;
		const cp2x = b.x - tan[segIndex + 1].tx * TENSION;
		const cp2y = b.y - tan[segIndex + 1].ty * TENSION;
		return `M ${a.x},${a.y} C ${cp1x},${cp1y} ${cp2x},${cp2y} ${b.x},${b.y}`;
	}

	// ── Autonomic curve helpers ──────────────────────────────
	// The autonomic curve includes ALL events (sleep + waking) with autonomic data.
	// It's an independent curve from novelty — sleep events only appear here.

	const autonomicChartPoints = $derived.by(() => {
		// All events (sleep + waking) with autonomic data, one dot each
		return events
			.filter((e) => !e.userHidden && e.autonomicZ != null)
			.sort((a, b) => a.startTime.getTime() - b.startTime.getTime())
			.map((e) => {
				const startHour = getHourOfDay(e.startTime);
				const endHour = getHourOfDay(e.endTime);
				const midHour = (startHour + endHour) / 2;
				return {
					x: hourToX(midHour),
					y: yToSvg(e.autonomicZ!),
					isUnknown: false,
					isSleep: e.isSleep ?? false,
				};
			});
	});

	const autonomicTangents = $derived.by(() => {
		const pts = autonomicChartPoints;
		return pts.map((_, i) => {
			if (pts.length < 2) return { tx: 0, ty: 0 };
			if (i === 0) {
				return { tx: pts[1].x - pts[0].x, ty: pts[1].y - pts[0].y };
			}
			if (i === pts.length - 1) {
				return { tx: pts[i].x - pts[i - 1].x, ty: pts[i].y - pts[i - 1].y };
			}
			return {
				tx: (pts[i + 1].x - pts[i - 1].x) / 2,
				ty: (pts[i + 1].y - pts[i - 1].y) / 2,
			};
		});
	});

	function autonomicCubicSegmentPath(segIndex: number): string {
		const pts = autonomicChartPoints;
		const tan = autonomicTangents;
		const a = pts[segIndex];
		const b = pts[segIndex + 1];
		const TENSION = 1 / 3;
		const cp1x = a.x + tan[segIndex].tx * TENSION;
		const cp1y = a.y + tan[segIndex].ty * TENSION;
		const cp2x = b.x - tan[segIndex + 1].tx * TENSION;
		const cp2y = b.y - tan[segIndex + 1].ty * TENSION;
		return `M ${a.x},${a.y} C ${cp1x},${cp1y} ${cp2x},${cp2y} ${b.x},${b.y}`;
	}

	/** Whether any event has autonomic data */
	const hasAutonomicData = $derived.by(() => {
		return events.some((e) => !e.userHidden && e.autonomicZ != null);
	});

	// ── Readiness marker at wake time ───────────────────────────
	const wakeHour = $derived.by(() => {
		// Find the last sleep mini-event's end time = wake time
		const sleepEvents = events.filter((e) => e.isSleep);
		if (sleepEvents.length === 0) return null;
		const lastSleep = sleepEvents.sort(
			(a, b) => b.endTime.getTime() - a.endTime.getTime(),
		)[0];
		return getHourOfDay(lastSleep.endTime);
	});

	const wakeX = $derived.by(() => {
		const h = wakeHour;
		return h !== null ? hourToX(h) : null;
	});

	// ── Crosshair interaction ───────────────────────────────────
	let svgEl: SVGSVGElement | undefined = $state();
	let hoverX = $state<number | null>(null); // SVG x coordinate under cursor
	let pinnedX = $state<number | null>(null); // Clicked/locked position

	function handleMouseMove(e: MouseEvent) {
		if (pinnedX !== null) return; // Don't update hover while pinned
		if (!svgEl) return;
		const rect = svgEl.getBoundingClientRect();
		const scaleX = WIDTH / rect.width;
		const svgX = (e.clientX - rect.left) * scaleX;
		// Clamp to plot area
		if (svgX < MARGIN.left || svgX > MARGIN.left + PLOT_W) {
			hoverX = null;
			return;
		}
		hoverX = svgX;
	}

	function handleMouseLeave() {
		if (pinnedX !== null) return; // Don't clear while pinned
		hoverX = null;
	}

	function handleClick(e: MouseEvent) {
		if (!svgEl) return;
		const rect = svgEl.getBoundingClientRect();
		const scaleX = WIDTH / rect.width;
		const svgX = (e.clientX - rect.left) * scaleX;
		if (svgX < MARGIN.left || svgX > MARGIN.left + PLOT_W) {
			pinnedX = null;
			hoverX = null;
			return;
		}
		if (pinnedX !== null) {
			// Unpin
			pinnedX = null;
		} else {
			// Pin at current position
			pinnedX = svgX;
			hoverX = svgX;
		}
	}

	// Convert SVG x back to hour of day
	function xToHour(x: number): number {
		return ((x - MARGIN.left) / PLOT_W) * 24;
	}

	// All events for crosshair lookup (including sleep + hidden)
	interface HoverableEvent {
		id: string;
		label: string;
		startHour: number;
		endHour: number;
		midHour: number;
		isSleep: boolean;
		isUnknown: boolean;
		noveltyZ: number | null;
		autonomicZ: number | null;
		// Link to the curve point (if this event is on the curve)
		curvePoint: EventPoint | null;
	}

	const allHoverEvents = $derived.by<HoverableEvent[]>(() => {
		const sorted = events
			.filter((e) => !e.userHidden)
			.sort((a, b) => a.startTime.getTime() - b.startTime.getTime());
		const pts = eventPoints;
		return sorted.map((e) => {
			const startHour = getHourOfDay(e.startTime);
			const endHour = getHourOfDay(e.endTime);
			const curvePoint = pts.find((p) => p.id === e.id) ?? null;
			return {
				id: e.id,
				label: e.userLabel ?? e.autoLabel,
				startHour,
				endHour,
				midHour: (startHour + endHour) / 2,
				isSleep: e.isSleep,
				isUnknown: e.isUnknown ?? false,
				noveltyZ: e.noveltyZ ?? null,
				autonomicZ: e.autonomicZ ?? null,
				curvePoint,
			};
		});
	});

	// Find the event whose time range contains the cursor hour
	const hoverEvent = $derived.by(() => {
		if (hoverX === null) return null;
		const hour = xToHour(hoverX);
		const evts = allHoverEvents;
		// Find event whose start-end range contains this hour
		for (const e of evts) {
			if (hour >= e.startHour && hour < e.endHour) return e;
		}
		// Fallback: find nearest event by midpoint
		let closest: (typeof evts)[0] | null = null;
		let minDist = Infinity;
		for (const e of evts) {
			const mid = e.midHour;
			const dist = Math.abs(hour - mid);
			if (dist < minDist) {
				minDist = dist;
				closest = e;
			}
		}
		return closest;
	});

	// Format crosshair time label
	const hoverTimeLabel = $derived.by(() => {
		if (hoverX === null) return "";
		const hour = xToHour(hoverX);
		const h = Math.floor(hour);
		const m = Math.floor((hour - h) * 60);
		const ampm = h < 12 ? "am" : "pm";
		const h12 = h === 0 ? 12 : h > 12 ? h - 12 : h;
		return `${h12}:${m.toString().padStart(2, "0")}${ampm}`;
	});

	// ── Active metric pill ──────────────────────────────────────
	// "Dayline" is the mini lifeline — the day in the same visual language as
	// the life-scale console. The novelty z-score curve that used to own the
	// word keeps its chart under its own name.
	type MetricView = "dayline" | "novelty" | "location" | "sleep" | "autonomic" | "dimensions";
	let activeMetric = $state<MetricView>("dayline");

	const hasSleepData = $derived.by(() => sleepCycles.length > 0);

	const metrics = $derived<{ id: MetricView; label: string; ready: boolean }[]>([
		{ id: "dayline", label: "Dayline", ready: true },
		{ id: "novelty", label: "Novelty", ready: true },
		{ id: "location", label: "Location", ready: hasLocationData },
		{ id: "sleep", label: "Sleep", ready: true },
		{ id: "autonomic", label: "Autonomic", ready: false },
		{ id: "dimensions", label: "Dimensions", ready: false },
	]);
</script>

<div class="dayline-container">
	<!-- Pill selector -->
	<div class="metric-pills">
		{#each metrics as metric}
			<button
				class="metric-pill"
				class:active={activeMetric === metric.id}
				class:disabled={!metric.ready}
				disabled={!metric.ready}
				onclick={() => {
					if (metric.ready) activeMetric = metric.id;
				}}
				type="button"
			>
				{metric.label}
			</button>
		{/each}
	</div>

	{#if activeMetric === "dayline"}
	<!-- The day as a mini lifeline: sleep + events as a gantt, the raw record
	     as per-lane density underneath — same endpoint the Lifeline draws.
	     The wrapper clears the absolutely-positioned pill row; the old SVG got
	     the same clearance from its own top margin. -->
	<div class="strip-wrap">
		<DaylineStrip {events} {timezone} {dayDateSlug} {sleepCycles} />
	</div>
	{:else if activeMetric === "novelty"}
	<!-- Chart -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<svg
		bind:this={svgEl}
		viewBox="0 0 {WIDTH} {HEIGHT}"
		preserveAspectRatio="xMidYMid meet"
		class="dayline-svg"
		onmousemove={handleMouseMove}
		onmouseleave={handleMouseLeave}
		onclick={handleClick}
	>
		<!-- Plot area background -->
		<rect
			x={MARGIN.left}
			y={MARGIN.top}
			width={PLOT_W}
			height={PLOT_H}
			fill="var(--color-surface, #fafafa)"
			rx="2"
		/>

		<!-- Y-axis grid lines and labels -->
		{#each Y_TICKS as z}
			{@const y = yToSvg(z)}
			{#if z === 0}
				<!-- Midline: dotted baseline (subtle) -->
				<line
					x1={MARGIN.left}
					y1={y}
					x2={MARGIN.left + PLOT_W}
					y2={y}
					stroke="var(--color-foreground-muted, #999)"
					stroke-width="0.5"
					stroke-dasharray="4,4"
					stroke-opacity="0.3"
				/>
			{:else}
				<!-- Faint grid line -->
				<line
					x1={MARGIN.left}
					y1={y}
					x2={MARGIN.left + PLOT_W}
					y2={y}
					stroke="var(--color-border, #e5e5e5)"
					stroke-width="0.5"
				/>
			{/if}

			<!-- Y-axis label -->
			<text
				x={MARGIN.left - 6}
				{y}
				text-anchor="end"
				dominant-baseline={z === 3
					? "hanging"
					: z === -3
						? "auto"
						: "middle"}
				class="axis-label y-label"
			>
				{formatYLabel(z)}
			</text>
		{/each}

		<!-- X-axis grid lines and labels -->
		{#each HOUR_TICKS as hour}
			{@const x = hourToX(hour)}

			<!-- Vertical grid line -->
			{#if hour > 0 && hour < 24}
				<line
					x1={x}
					y1={MARGIN.top}
					x2={x}
					y2={MARGIN.top + PLOT_H}
					stroke="var(--color-border, #e5e5e5)"
					stroke-width="0.5"
				/>
			{/if}

			<!-- X-axis label -->
			<text
				{x}
				y={MARGIN.top + PLOT_H + 16}
				text-anchor={hour === 0
					? "start"
					: hour === 24
						? "end"
						: "middle"}
				class="axis-label x-label"
			>
				{formatHourLabel(hour)}
			</text>
		{/each}

		<!-- Sleep region background tint -->
		{#if wakeX !== null}
			{@const wx = wakeX!}
			<rect x={MARGIN.left} y={MARGIN.top} width={wx - MARGIN.left} height={PLOT_H}
				fill="var(--color-primary, #4f46e5)" fill-opacity="0.03" />
		{/if}

		<!-- Curved segments (solid for known, dotted for unknown) + hover targets -->
		{#each chartPoints.slice(0, -1) as _, segIdx}
			{@const a = chartPoints[segIdx]}
			{@const b = chartPoints[segIdx + 1]}
			{@const isDotted = a.isUnknown || b.isUnknown}
			{@const d = cubicSegmentPath(segIdx)}

			<!-- Visible curved segment (novelty = primary color) -->
			<path
				{d}
				fill="none"
				stroke="var(--color-primary, #4f46e5)"
				stroke-width="1.5"
				stroke-linecap="round"
				stroke-dasharray={isDotted ? "4,4" : "none"}
				stroke-opacity={isDotted ? 0.5 : 1}
				pointer-events="none"
			/>

			<!-- Fat invisible hover target -->
			<path
				{d}
				fill="none"
				stroke="transparent"
				stroke-width="12"
				stroke-linecap="round"
			/>
		{/each}

		<!-- Autonomic curve (Stress ↑ / Recovery ↓) -->
		{#if hasAutonomicData}
			{#each autonomicChartPoints.slice(0, -1) as _, segIdx}
				{@const a = autonomicChartPoints[segIdx]}
				{@const b = autonomicChartPoints[segIdx + 1]}
				{@const isDotted = a.isUnknown || b.isUnknown}
				<path
					d={autonomicCubicSegmentPath(segIdx)}
					fill="none"
					stroke="var(--color-foreground-muted, #888)"
					stroke-width="1.5"
					stroke-linecap="round"
					stroke-dasharray={isDotted ? "4,4" : "none"}
					stroke-opacity={isDotted ? 0.25 : 0.35}
					pointer-events="none"
				/>
			{/each}
		{/if}

		<!-- Readiness diamond at wake time (replaces midnight anchor) -->
		{#if readinessScore != null && wakeX !== null}
			{@const wx = wakeX!}
			{@const rZ = ((readinessScore - 50) / 50) * Y_MAX}
			<g transform="translate({wx}, {yToSvg(rZ)})">
				<polygon points="0,-6 5,0 0,6 -5,0"
					fill="var(--color-primary, #4f46e5)"
					stroke="var(--color-background, #fff)" stroke-width="1.5" />
			</g>
		{:else if eventPoints.length > 0}
			<!-- Fallback: small dot at curve start -->
			<circle
				cx={hourToX(0)}
				cy={yToSvg(0)}
				r="3"
				fill="var(--color-primary, #4f46e5)"
				stroke="var(--color-background, #fff)"
				stroke-width="1"
				opacity="0.5"
			/>
		{/if}

		<!-- Sleep dots on autonomic curve -->
		{#if hasAutonomicData}
			{#each autonomicChartPoints as point}
				{#if point.isSleep}
					<circle
						cx={point.x}
						cy={point.y}
						r="2.5"
						fill="var(--color-foreground-muted, #888)"
						stroke="var(--color-background, #fff)"
						stroke-width="1"
						opacity="0.6"
					/>
				{/if}
			{/each}
		{/if}

		<!-- Event changepoint dots (novelty) -->
		{#each eventPoints as point}
			{#if point.isUnknown}
				<circle
					cx={point.x}
					cy={point.y}
					r="3.5"
					fill="var(--color-background, #fff)"
					stroke="var(--color-foreground-subtle, #aaa)"
					stroke-width="1.5"
					stroke-dasharray="2,2"
				/>
			{:else}
				<circle
					cx={point.x}
					cy={point.y}
					r="3"
					class="event-dot"
					class:novel={point.noveltyZ >= 1.0}
					class:routine={point.noveltyZ <= -0.3}
				/>
			{/if}
		{/each}

		<!-- Entity threading + topic/entity sub-dots (commented out for clarity while testing two-line chart)
		{#each entityThreads as thread (thread.entityId)}
			{#each thread.segments as seg}
				<line x1={seg.x1} y1={seg.y1} x2={seg.x2} y2={seg.y2} class="entity-thread" />
			{/each}
		{/each}
		{#each eventPoints as point}
			{#if !point.isUnknown && point.subDots.length > 0}
				{@const RELATIVE_SCALE = 0.4}
				{#each point.subDots as dot}
					{@const offset = dot.z * RELATIVE_SCALE}
					{@const dotY = Math.max(MARGIN.top, Math.min(MARGIN.top + PLOT_H, point.y - offset * (PLOT_H / (2 * Y_MAX))))}
					{@const dotX = dot.xHourOverride !== null ? hourToX(dot.xHourOverride) : point.x}
					<circle cx={dotX} cy={dotY} r={dot.kind === "entity" ? 2.5 : 2}
						class="sub-dot" class:sub-topic={dot.kind === "topic"} class:sub-entity={dot.kind === "entity"}
						class:sub-novel={dot.z > 1.0} class:sub-routine={dot.z < -1.0}>
						<title>{dot.name} ({dot.z >= 0 ? "+" : ""}{dot.z.toFixed(1)}σ)</title>
					</circle>
				{/each}
			{/if}
		{/each}
		-->

		<!-- "Now" marker (vertical line + time badge) -->
		{#if nowX !== null}
			{@const x = nowX}
			<line
				x1={x}
				y1={MARGIN.top}
				x2={x}
				y2={MARGIN.top + PLOT_H}
				stroke="var(--color-success, #22c55e)"
				stroke-width="1"
				stroke-opacity="0.6"
			/>
			<circle
				cx={x}
				cy={MARGIN.top - 1}
				r="3"
				fill="var(--color-success, #22c55e)"
			/>
			<text
				{x}
				y={MARGIN.top - 10}
				text-anchor="middle"
				class="now-label"
			>
				{nowTimeLabel}
			</text>
		{/if}

		<!-- (readiness diamond now rendered with the anchor dot above) -->

		<!-- Plot border -->
		<rect
			x={MARGIN.left}
			y={MARGIN.top}
			width={PLOT_W}
			height={PLOT_H}
			fill="none"
			stroke="var(--color-border, #e5e5e5)"
			stroke-width="0.75"
			rx="2"
		/>

		<!-- Y-axis semantic labels (inside plot, top-left / bottom-left) -->
		<!-- Y-axis semantic labels (stacked vertically: novelty on top, autonomic below) -->
		<text x={MARGIN.left + 6} y={MARGIN.top + 12} class="axis-semantic-label novelty-label">Novel</text>
		{#if hasAutonomicData}
			<text x={MARGIN.left + 6} y={MARGIN.top + 26} class="axis-semantic-label autonomic-label">Stress</text>
		{/if}
		<text x={MARGIN.left + 6} y={MARGIN.top + PLOT_H - 18} class="axis-semantic-label novelty-label">Routine</text>
		{#if hasAutonomicData}
			<text x={MARGIN.left + 6} y={MARGIN.top + PLOT_H - 4} class="axis-semantic-label autonomic-label">Recovery</text>
		{/if}

		<!-- Legend (top-right, inside plot area) -->
		<g transform="translate({MARGIN.left + PLOT_W - 10}, {MARGIN.top + 14})">
			<!-- Novelty line legend -->
			<line x1="-52" y1="0" x2="-40" y2="0" stroke="var(--color-primary, #4f46e5)" stroke-width="1.5" />
			<text x="-36" y="0" class="legend-label novelty-legend" dominant-baseline="middle">Novelty</text>
			{#if hasAutonomicData}
				<!-- Autonomic line legend -->
				<line x1="-52" y1="16" x2="-40" y2="16" stroke="var(--color-foreground-muted, #888)" stroke-width="1.5" />
				<text x="-36" y="16" class="legend-label autonomic-legend" dominant-baseline="middle">Autonomic</text>
			{/if}
		</g>

		<!-- Crosshair scrubber (follows mouse) -->
		{#if hoverX !== null}
			{@const evt = hoverEvent}
			<!-- Vertical dashed line -->
			<line
				x1={hoverX}
				y1={MARGIN.top}
				x2={hoverX}
				y2={MARGIN.top + PLOT_H}
				stroke="var(--color-foreground-subtle, #aaa)"
				stroke-width="0.75"
				stroke-dasharray="3,3"
				pointer-events="none"
			/>

			{#if evt}
				{@const evtX1 = hourToX(evt.startHour)}
				{@const evtX2 = hourToX(evt.endHour)}
				{@const isMuted = evt.isUnknown}
				{@const highlightColor = isMuted
					? "var(--color-foreground-subtle, #aaa)"
					: "var(--color-primary, #4f46e5)"}
				{@const evtMidX = hourToX(evt.midHour)}

				<!-- Event range highlight -->
				<rect
					x={evtX1}
					y={MARGIN.top}
					width={Math.max(2, evtX2 - evtX1)}
					height={PLOT_H}
					fill={highlightColor}
					fill-opacity={isMuted ? 0.04 : 0.06}
					pointer-events="none"
				/>
				<!-- Range edge lines -->
				<line
					x1={evtX1}
					y1={MARGIN.top}
					x2={evtX1}
					y2={MARGIN.top + PLOT_H}
					stroke={highlightColor}
					stroke-width="0.5"
					stroke-opacity="0.2"
					pointer-events="none"
				/>
				<line
					x1={evtX2}
					y1={MARGIN.top}
					x2={evtX2}
					y2={MARGIN.top + PLOT_H}
					stroke={highlightColor}
					stroke-width="0.5"
					stroke-opacity="0.2"
					pointer-events="none"
				/>

				<!-- Highlight dot on novelty curve (only for events on the curve) -->
				{#if evt.curvePoint}
					<circle
						cx={evt.curvePoint.x}
						cy={evt.curvePoint.y}
						r="6"
						fill="var(--color-primary, #4f46e5)"
						stroke="var(--color-background, #fff)"
						stroke-width="2"
						pointer-events="none"
					/>
				{/if}

				<!-- Tooltip -->
				{@const tooltipX =
					evtMidX > MARGIN.left + PLOT_W / 2
						? evtMidX - 8
						: evtMidX + 8}
				{@const tooltipAnchor =
					evtMidX > MARGIN.left + PLOT_W / 2 ? "end" : "start"}
				<g pointer-events="none">
					<!-- Event label -->
					<text
						x={tooltipX}
						y={MARGIN.top + 15}
						text-anchor={tooltipAnchor}
						class="crosshair-label"
						fill={isMuted
							? "var(--color-foreground-subtle, #aaa)"
							: "var(--color-foreground, #333)"}
					>
						{evt.isUnknown
								? "Unknown"
								: evt.label.length > 40
									? evt.label.slice(0, 40) + "…"
									: evt.label}
					</text>
					<!-- Scores (for all non-unknown events) -->
					{#if evt.isSleep && evt.autonomicZ !== null}
						<text
							x={tooltipX}
							y={MARGIN.top + 33}
							text-anchor={tooltipAnchor}
							class="crosshair-score autonomic-legend"
						>
							Autonomic {evt.autonomicZ >= 0
								? "+"
								: ""}{evt.autonomicZ.toFixed(1)}σ
						</text>
					{:else if !isMuted && evt.noveltyZ !== null}
						<text
							x={tooltipX}
							y={MARGIN.top + 33}
							text-anchor={tooltipAnchor}
							class="crosshair-score novelty-legend"
						>
							Novelty {evt.noveltyZ >= 0
								? "+"
								: ""}{evt.noveltyZ.toFixed(1)}σ
						</text>
						{#if evt.autonomicZ !== null}
							<text
								x={tooltipX}
								y={MARGIN.top + 50}
								text-anchor={tooltipAnchor}
								class="crosshair-score autonomic-legend"
							>
								Autonomic {evt.autonomicZ >= 0
									? "+"
									: ""}{evt.autonomicZ.toFixed(1)}σ
							</text>
						{/if}
					{/if}
				</g>
			{/if}
		{/if}
	</svg>
	{:else if activeMetric === "location"}
	<!-- Location view: timeline + map -->
	{#if hasLocationData}
		<div class="location-view">
			{#if movementStops.length > 0}
				<DayLocationTimeline
					visits={movementStops}
					dayDate={dayDateSlug}
					bind:hoverTimeMs={movementHoverTimeMs}
				/>
			{/if}
			<MovementMap
				track={movementTrack}
				stops={dedupedMarkers}
				height={240}
				hoverTimeMs={movementHoverTimeMs}
			/>
		</div>
	{:else}
		<div class="sleep-empty">
			<p class="empty-placeholder">No location data for this day</p>
		</div>
	{/if}
	{:else if activeMetric === "sleep"}
	<!-- Sleep architecture view (from scored sleep cycles, not wiki_events) -->
	{#if sleepCycles.length > 0}
		{@const firstStart = sleepCycles[0].startTime}
		{@const lastEnd = sleepCycles[sleepCycles.length - 1].endTime}
		{@const totalMs = lastEnd.getTime() - firstStart.getTime()}
		{@const SLEEP_H = 200}
		{@const SLEEP_W = 840}
		{@const SM = { top: 28, right: 16, bottom: 28, left: 50 }}
		{@const plotW = SLEEP_W - SM.left - SM.right}
		{@const plotH = SLEEP_H - SM.top - SM.bottom}
		<svg viewBox="0 0 {SLEEP_W} {SLEEP_H}" preserveAspectRatio="xMidYMid meet" class="dayline-svg">
			<rect x={SM.left} y={SM.top} width={plotW} height={plotH}
				fill="var(--color-surface, #fafafa)" rx="2" />
			<!-- Stage depth labels -->
			{#each [
				{ label: "Awake", y: SM.top + plotH * 0.05 },
				{ label: "REM", y: SM.top + plotH * 0.28 },
				{ label: "Light", y: SM.top + plotH * 0.55 },
				{ label: "Deep", y: SM.top + plotH * 0.85 },
			] as row}
				<text x={SM.left - 6} y={row.y} text-anchor="end" dominant-baseline="middle"
					class="axis-label y-label">{row.label}</text>
				<line x1={SM.left} y1={row.y} x2={SM.left + plotW} y2={row.y}
					stroke="var(--color-border, #e5e5e5)" stroke-width="0.5" />
			{/each}
			<!-- Cycle bars -->
			{#each sleepCycles as cycle, i}
				{@const x1 = SM.left + ((cycle.startTime.getTime() - firstStart.getTime()) / totalMs) * plotW}
				{@const x2 = SM.left + ((cycle.endTime.getTime() - firstStart.getTime()) / totalMs) * plotW}
				{@const barW = Math.max(2, x2 - x1)}
				{@const az = cycle.autonomicZ ?? -1}
				<!-- Map autonomic_z to depth: more negative = deeper recovery = taller bar from bottom -->
				{@const depth = Math.min(1, Math.max(0, (-az + 0.5) / 2.5))}
				{@const barH = plotH * 0.3 + depth * plotH * 0.6}
				{@const barY = SM.top + plotH - barH}
				<rect x={x1} y={barY} width={barW} height={barH} rx="3"
					fill="var(--color-primary, #4f46e5)"
					fill-opacity={0.15 + depth * 0.35} />
				<!-- Cycle label -->
				<text x={(x1 + x2) / 2} y={SM.top + plotH + 16} text-anchor="middle" class="axis-label x-label">
					Cycle {i + 1}
				</text>
				<!-- Autonomic score inside bar -->
				<text x={(x1 + x2) / 2} y={barY + 14} text-anchor="middle"
					class="axis-label" fill="var(--color-primary, #4f46e5)" opacity="0.8">
					{az >= 0 ? "+" : ""}{az.toFixed(1)}σ
				</text>
			{/each}
			<!-- Border -->
			<rect x={SM.left} y={SM.top} width={plotW} height={plotH}
				fill="none" stroke="var(--color-border, #e5e5e5)" stroke-width="0.75" rx="2" />
			<!-- Readiness badge -->
			{#if readinessScore != null}
				<text x={SM.left + plotW - 4} y={SM.top + 14} text-anchor="end"
					class="axis-label" fill="var(--color-foreground-muted, #888)">
					Readiness {readinessScore}%
				</text>
			{/if}
		</svg>
	{:else}
		<div class="sleep-empty">
			<p class="empty-placeholder">No sleep data for this day</p>
		</div>
	{/if}
	{/if}
</div>

<style>
	.dayline-container {
		width: 100%;
		position: relative;
		display: flex;
		flex-direction: column;
		gap: 0;
		margin-top: 1.5rem;
		margin-bottom: 1rem;
	}

	/* ── Pill selector ──────────────────────────────────────── */

	.metric-pills {
		position: absolute;
		top: 0;
		right: 0;
		z-index: 2;
		display: flex;
		gap: 0.25rem;
		padding: 0;
	}

	.strip-wrap {
		padding-top: 2rem;
	}

	.metric-pill {
		padding: 0.25rem 0.5rem;
		border: none;
		background: none;
		font-size: 0.6875rem;
		font-weight: 500;
		color: var(--color-foreground-subtle);
		cursor: pointer;
		border-radius: 3px;
		transition:
			color 0.15s ease,
			background 0.15s ease;
		letter-spacing: 0.02em;
	}

	.metric-pill:hover:not(.disabled) {
		color: var(--color-foreground-muted);
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
	}

	.metric-pill.active {
		color: var(--color-foreground);
	}

	.metric-pill.disabled {
		opacity: 0.3;
		cursor: default;
	}

	/* ── SVG chart ──────────────────────────────────────────── */

	.dayline-svg {
		width: 100%;
		height: auto;
		cursor: crosshair;
	}

	/* ── Event dots ─────────────────────────────────────────── */

	.event-dot {
		fill: var(--color-primary, #4f46e5);
		stroke: var(--color-background, #fff);
		stroke-width: 1.5;
	}

	.event-dot.novel {
		fill: var(--color-primary, #4f46e5);
	}

	.event-dot.routine {
		fill: color-mix(
			in srgb,
			var(--color-primary, #4f46e5) 50%,
			transparent
		);
	}

	/* ── Topic/entity sub-dots ──────────────────────────────── */

	.sub-dot {
		opacity: 0.45;
		stroke: none;
	}

	.sub-topic {
		fill: var(--color-foreground-muted, #888);
	}

	.sub-entity {
		fill: var(--color-primary, #4f46e5);
	}

	/* ── Entity threading lines ─────────────────────────────── */

	.entity-thread {
		stroke: var(--color-primary, #4f46e5);
		stroke-width: 0.75;
		stroke-opacity: 0.15;
		fill: none;
	}

	.sub-dot.sub-novel {
		opacity: 0.75;
	}

	.sub-dot.sub-routine {
		opacity: 0.25;
	}

	.now-label {
		font-size: 8px;
		font-weight: 500;
		fill: var(--color-success, #22c55e);
		font-family: var(--font-mono, monospace);
	}

	.axis-label {
		font-size: 9px;
		font-weight: 400;
		fill: var(--color-foreground-subtle, #999);
		font-family: var(--font-mono, monospace);
	}

	.axis-semantic-label {
		font-size: 9px;
		font-weight: 500;
		font-family: var(--font-sans, system-ui, sans-serif);
		letter-spacing: 0.02em;
		opacity: 0.7;
	}

	.axis-semantic-label.novelty-label {
		fill: var(--color-primary, #4f46e5);
	}

	.axis-semantic-label.autonomic-label {
		fill: var(--color-foreground-muted, #888);
	}

	.legend-label {
		font-size: 8px;
		font-weight: 500;
		font-family: var(--font-sans, system-ui, sans-serif);
		letter-spacing: 0.02em;
		opacity: 0.6;
	}

	.legend-label.novelty-legend {
		fill: var(--color-primary, #4f46e5);
	}

	.legend-label.autonomic-legend {
		fill: var(--color-foreground-muted, #888);
	}

	/* ── Crosshair tooltip ─────────────────────────────────── */

	.crosshair-label {
		font-size: 11px;
		font-weight: 500;
		fill: var(--color-foreground, #333);
		font-family: var(--font-sans, system-ui, sans-serif);
		letter-spacing: 0.01em;
	}

	.crosshair-score {
		font-size: 10px;
		font-weight: 500;
		font-family: var(--font-mono, monospace);
	}

	.crosshair-score.novelty-legend {
		fill: var(--color-primary, #4f46e5);
	}

	.crosshair-score.autonomic-legend {
		fill: var(--color-foreground-muted, #888);
	}

	/* ── Location view ─────────────────────────────────────── */

	.location-view {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		padding-top: 0.5rem;
	}
</style>
