<!--
	RecordField.svelte — the whole record, as a field of light.

	Every hour this box has ever seen you awake, plotted as time-of-day against
	date: nine years across, one midnight-to-midnight down. Intensity is how
	much happened in that hour.

	It is not decoration. It is the only place in the product where the scale of
	what the box holds is visible — four hundred thousand records, eight and a
	half years — and it is legible as a *life*: the dark band across the small
	hours is sleep, the bright plateau is the working day, a vertical tear is a
	week the collectors were down, a sideways shift is a move or a flight.
	Nothing here is generated or interpreted; every lit pixel is a count.

	Drawn as a 1200×24 raster upscaled with smoothing rather than as tens of
	thousands of shapes: the interpolation is what makes a bar chart into an
	atmosphere, and it costs one drawImage. A second, unsmoothed pass on top
	returns the grain, so the image has both bloom and detail. That is also why
	there is no WebGL here — this is a raster, not a scene, and three.js would
	be half a megabyte to draw a picture canvas already draws.

	The plate is ink-dark on purpose. The page around it is white and quiet; the
	boldness is spent here and nowhere else.
-->
<script lang="ts">
	import { getLifeline, getClock } from "$lib/wiki/api";
	import type { StreamHealth } from "$lib/api/client";

	interface Props {
		/** Stream totals, for the true size of the record. */
		health: Record<string, StreamHealth>;
		tz: string;
	}
	let { health, tz }: Props = $props();

	const reduce =
		typeof window !== "undefined" &&
		window.matchMedia &&
		window.matchMedia("(prefers-reduced-motion: reduce)").matches;

	/** Columns of the raster. One per ~2.6 days over a nine-year record. */
	const COLUMNS = 1200;

	let cells = $state<number[] | null>(null);
	let columns = $state(0);
	let span = $state<{ from: number; to: number } | null>(null);
	let failed = $state(false);

	let canvas = $state<HTMLCanvasElement | undefined>(undefined);
	let W = $state(1000);
	let H = $state(420);

	// ── the data ────────────────────────────────────────────────
	$effect(() => {
		let dropped = false;
		(async () => {
			// One bucket over no window asks the server where the record starts
			// and ends — the corpus decides its own span, not a default year.
			const probe = await getLifeline(1).catch(() => null);
			if (dropped) return;
			if (!probe) {
				failed = true;
				return;
			}
			const c = await getClock(probe.from, probe.to, COLUMNS, tz).catch(() => null);
			if (dropped) return;
			if (!c) {
				failed = true;
				return;
			}
			cells = c.cells;
			columns = c.columns;
			span = { from: Date.parse(c.from), to: Date.parse(c.to) };
		})();
		return () => {
			dropped = true;
		};
	});

	// ── the size of it, said plainly ────────────────────────────
	const totals = $derived.by(() => {
		const rows = Object.values(health);
		const records = rows.reduce((a, r) => a + (r.total || 0), 0);
		return { records, streams: rows.filter((r) => r.total > 0).length };
	});
	const days = $derived(span ? Math.round((span.to - span.from) / 86_400_000) : 0);
	const int = new Intl.NumberFormat();
	const startLabel = $derived(
		span ? new Date(span.from).toLocaleDateString(undefined, { month: "long", year: "numeric" }) : "",
	);

	// ── the drawing ─────────────────────────────────────────────
	/** The raster, built once per data change and reused every frame. */
	const raster = $derived.by(() => {
		const c = cells;
		if (!c || !columns) return null;

		// Counts span three orders of magnitude — a linear ramp would show the
		// peak and hide the life. Log against the 98th percentile keeps the
		// ordinary hours visible and lets the extremes clip.
		const nz = c.filter((v) => v > 0).sort((a, b) => a - b);
		if (!nz.length) return null;
		const hi = Math.max(2, nz[Math.floor(nz.length * 0.98)]);
		const k = 1 / Math.log(1 + hi);

		const off = document.createElement("canvas");
		off.width = columns;
		off.height = 24;
		const octx = off.getContext("2d");
		if (!octx) return null;
		const img = octx.createImageData(columns, 24);
		const d = img.data;
		for (let col = 0; col < columns; col++) {
			for (let h = 0; h < 24; h++) {
				const v = c[col * 24 + h] ?? 0;
				if (v <= 0) continue;
				const t = Math.min(1, Math.log(1 + v) * k);
				const i = (h * columns + col) * 4;
				// Cool at the quiet end, warm-white at the loud end: a busy hour
				// reads hotter than a sparse one without inventing a category.
				d[i] = 118 + t * 137;
				d[i + 1] = 158 + t * 92;
				d[i + 2] = 232 + t * 23;
				d[i + 3] = Math.round((0.18 + 0.82 * t) * 255);
			}
		}
		octx.putImageData(img, 0, 0);
		return off;
	});

	const HOURS = [0, 6, 12, 18];
	function hourLabel(h: number): string {
		return h === 0 ? "12a" : h === 12 ? "12p" : h < 12 ? `${h}a` : `${h - 12}p`;
	}
	/** Year boundaries inside the span, for the bottom axis. */
	const years = $derived.by(() => {
		if (!span) return [] as Array<{ x: number; label: string }>;
		const out: Array<{ x: number; label: string }> = [];
		const a = new Date(span.from).getFullYear(),
			b = new Date(span.to).getFullYear();
		for (let y = a + 1; y <= b; y++) {
			const t = Date.parse(`${y}-01-01T00:00:00`);
			const p = (t - span.from) / (span.to - span.from);
			if (p > 0.02 && p < 0.99) out.push({ x: p, label: String(y) });
		}
		return out;
	});

	// The reveal is a one-shot, deliberately separate from drawing. Tying it to
	// the canvas size meant every resize observation restarted the sweep — and
	// a plate that replays its own arrival whenever the window moves never
	// finishes drawing itself.
	let progress = $state(0);
	let started = false;
	$effect(() => {
		if (!raster || started) return;
		started = true;
		if (reduce || document.hidden) {
			progress = 1;
			return;
		}
		let raf = 0,
			start = 0;
		// A backstop, because a tab that is never looked at gets no frames at
		// all: the image must be whole whenever it is finally seen.
		const done = setTimeout(() => {
			progress = 1;
			if (raf) cancelAnimationFrame(raf);
		}, 2400);
		function step(ts: number) {
			if (!start) start = ts;
			const p = Math.min(1, (ts - start) / 1900);
			// Ease out, so the sweep arrives at today rather than stopping there.
			progress = 1 - Math.pow(1 - p, 3);
			if (p < 1) raf = requestAnimationFrame(step);
			else clearTimeout(done);
		}
		raf = requestAnimationFrame(step);
		return () => {
			clearTimeout(done);
			if (raf) cancelAnimationFrame(raf);
		};
	});

	// Redraw on size or progress. Cheap: two drawImage calls off a cached
	// raster, so a resize costs nothing and does not disturb the reveal.
	$effect(() => {
		const cv = canvas,
			r = raster,
			w = W,
			h = H,
			e = progress;
		if (!cv || !r) return;
		const dpr = Math.min(window.devicePixelRatio || 1, 2);
		if (cv.width !== Math.round(w * dpr)) {
			cv.width = Math.round(w * dpr);
			cv.height = Math.round(h * dpr);
		}
		const ctx = cv.getContext("2d");
		if (!ctx) return;
		ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
		ctx.clearRect(0, 0, w, h);
		ctx.save();
		ctx.beginPath();
		ctx.rect(0, 0, w * e, h);
		ctx.clip();
		// Bloom, then grain.
		ctx.imageSmoothingEnabled = true;
		ctx.imageSmoothingQuality = "high";
		ctx.globalAlpha = 1;
		ctx.drawImage(r, 0, 0, r.width, 24, 0, 0, w, h);
		ctx.imageSmoothingEnabled = false;
		ctx.globalAlpha = 0.42;
		ctx.drawImage(r, 0, 0, r.width, 24, 0, 0, w, h);
		ctx.restore();
		ctx.globalAlpha = 1;
	});
</script>

<figure class="field">
	<div
		class="plate"
		bind:clientWidth={W}
		bind:clientHeight={H}
		role="img"
		aria-label="Every hour of activity this box has recorded, {startLabel} to now: time of day down, date across."
	>
		<canvas bind:this={canvas} aria-hidden="true"></canvas>

		{#if raster}
			<div class="axes" aria-hidden="true">
				{#each HOURS as h}
					<span class="hr mono" style="top:{(h / 24) * 100}%">{hourLabel(h)}</span>
				{/each}
				{#each years as y}
					<span class="yr mono" style="left:{y.x * 100}%">{y.label}</span>
				{/each}
				<span class="now" style="left:100%"></span>
				<span class="nowlab mono" style="left:100%">today</span>
			</div>
		{:else if !failed}
			<p class="loading mono">reading the record…</p>
		{/if}
	</div>

	<figcaption>
		<span class="cap">
			<span class="figno mono">Fig. 1</span>
			Time of day against date. Every lit hour is a count, never an estimate.
		</span>
		{#if totals.records}
			<!-- The margin: the size of the thing, stated once, without adjectives. -->
			<span class="scale mono">
				{int.format(totals.records)} records · {int.format(days)} days · since {startLabel}
			</span>
		{/if}
	</figcaption>
</figure>

<style>
	.field { margin: 0; }
	.mono { font-family: var(--font-mono); font-variant-numeric: tabular-nums; }

	/*
	 * A plate, in the printed sense: ink-dark, inset in a white page. Fixed
	 * rather than themed — it is an image, and an image does not invert when
	 * the room does.
	 */
	.plate {
		position: relative;
		height: clamp(300px, 44vh, 460px);
		background: #0a0c11;
		border-radius: 3px;
		overflow: hidden;
		isolation: isolate;
	}
	.plate canvas { display: block; width: 100%; height: 100%; }

	.axes { position: absolute; inset: 0; pointer-events: none; }
	.hr {
		position: absolute; left: 10px; transform: translateY(-50%);
		font-size: 9px; letter-spacing: 0.06em; color: rgba(255, 255, 255, 0.38);
	}
	.yr {
		position: absolute; bottom: 7px; transform: translateX(-50%);
		font-size: 9px; letter-spacing: 0.06em; color: rgba(255, 255, 255, 0.3);
	}
	.now {
		position: absolute; top: 0; bottom: 0; width: 1px;
		background: rgba(255, 255, 255, 0.55); transform: translateX(-1px);
	}
	.nowlab {
		position: absolute; bottom: 7px; transform: translateX(calc(-100% - 7px));
		font-size: 9px; letter-spacing: 0.06em; color: rgba(255, 255, 255, 0.62);
	}
	.loading {
		position: absolute; inset: 0; display: grid; place-items: center;
		font-size: 10.5px; letter-spacing: 0.06em; color: rgba(255, 255, 255, 0.3); margin: 0;
	}

	figcaption {
		display: flex; align-items: baseline; gap: 20px; flex-wrap: wrap;
		margin-top: 12px;
		font-family: var(--font-sans); font-size: 12.5px; line-height: 1.5;
		color: var(--color-foreground-subtle);
	}
	.figno { font-size: 10.5px; color: var(--color-foreground-muted); margin-right: 8px; }
	.scale { margin-left: auto; font-size: 11px; letter-spacing: 0.02em; color: var(--color-foreground-muted); }
	@media (max-width: 720px) {
		.scale { margin-left: 0; width: 100%; }
	}
</style>
