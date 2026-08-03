<script lang="ts">
	/**
	 * The ground — where the selected window was spent.
	 *
	 * **Why the location lane needs this.** A density bar answers "how many GPS
	 * pings in March", which is not a question anyone has ever had. The question
	 * about location is always WHERE, and no arrangement of bars will ever
	 * answer it. So location gets a second view keyed to the same window the
	 * lanes are drawn over: brush the timeline, and this is the ground it
	 * covered.
	 *
	 * **No basemap, deliberately.** Not a missing feature. Fetching tiles to
	 * draw someone's own life back at them would hand their coordinate history
	 * to a third party, which contradicts the entire product. The trace IS the
	 * map: three weeks of points draw the road network unaided, and the shape
	 * that emerges is unmistakably the place you live.
	 *
	 * **Stays are clustered by coordinate.** `place_name` is NULL on every visit
	 * a real box holds — the collector has never filled it — so grouping by name
	 * would give one bucket called nothing. The server rounds to ~110 m instead,
	 * which is what the column was supposed to do.
	 */
	import { getGround, type Ground, type Stay } from '$lib/wiki/api';

	interface Props {
		from: number;
		to: number;
		/** Called when a stay is clicked, so the timeline can mark its visits. */
		onpick?: (s: Stay | null) => void;
	}
	let { from, to, onpick }: Props = $props();

	let el = $state<HTMLCanvasElement | null>(null);
	let box = $state<HTMLDivElement | null>(null);
	let data = $state<Ground | null>(null);
	let loading = $state(true);
	let hover = $state<number | null>(null);
	let picked = $state<number | null>(null);
	let w = $state(280);
	const H = 210;

	// Refetch when the window moves, debounced with the same 140ms the lanes
	// use so a drag settles once rather than at every frame.
	let seq = 0;
	let timer: ReturnType<typeof setTimeout> | undefined;
	$effect(() => {
		const a = new Date(from).toISOString();
		const b = new Date(to).toISOString();
		clearTimeout(timer);
		timer = setTimeout(async () => {
			const mine = ++seq;
			const g = await getGround(a, b);
			if (mine !== seq) return;
			data = g;
			loading = false;
			picked = null;
			hover = null;
			draw();
		}, 140);
	});

	/**
	 * Equirectangular, with longitude squeezed by cos(latitude).
	 *
	 * At Austin's latitude a degree of longitude is ~14% shorter than a degree
	 * of latitude; without the correction the city comes out visibly stretched
	 * east-west, and a trace whose whole value is being recognisable cannot
	 * afford to be the wrong shape.
	 */
	const proj = $derived.by(() => {
		const bb = data?.bbox;
		if (!bb) return null;
		const [lat0, lat1, lon0, lon1] = bb;
		const midLat = ((lat0 + lat1) / 2) * (Math.PI / 180);
		const kx = Math.cos(midLat);
		// A pinpoint window would divide by zero; give it a floor of ~50 m.
		const spanY = Math.max(lat1 - lat0, 0.0005);
		const spanX = Math.max((lon1 - lon0) * kx, 0.0005);
		const pad = 14;
		const s = Math.min((w - pad * 2) / spanX, (H - pad * 2) / spanY);
		const ox = (w - spanX * s) / 2;
		const oy = (H - spanY * s) / 2;
		return {
			x: (lon: number) => ox + (lon - lon0) * kx * s,
			// Screen y grows downward; latitude grows north.
			y: (lat: number) => oy + (lat1 - lat) * s
		};
	});

	const maxMinutes = $derived(
		data?.stays.reduce((m, s) => Math.max(m, s.minutes), 0) || 1
	);

	/** Dot area proportional to time, so twice the hours looks like twice. */
	const radius = (m: number) => 2.5 + Math.sqrt(m / maxMinutes) * 11;

	function draw() {
		const c = el;
		const p = proj;
		if (!c || !data) return;
		const dpr = window.devicePixelRatio || 1;
		c.width = Math.round(w * dpr);
		c.height = Math.round(H * dpr);
		c.style.height = `${H}px`;
		const ctx = c.getContext('2d');
		if (!ctx) return;
		ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
		ctx.clearRect(0, 0, w, H);
		if (!p) return;

		const st = getComputedStyle(c);
		const ink = st.getPropertyValue('--color-foreground').trim() || '#171717';
		const blue = st.getPropertyValue('--color-primary').trim() || '#2883de';

		// The trace. Drawn as dots rather than a joined path: consecutive points
		// can be an hour and ten miles apart, and joining those draws a road
		// that was never taken. Dots only ever assert where you were.
		ctx.fillStyle = ink;
		ctx.globalAlpha = 0.22;
		for (const [lat, lon] of data.track) {
			ctx.fillRect(p.x(lon) - 0.5, p.y(lat) - 0.5, 1.4, 1.4);
		}
		ctx.globalAlpha = 1;

		data.stays.forEach((s, i) => {
			const x = p.x(s.lon);
			const y = p.y(s.lat);
			const r = radius(s.minutes);
			const on = i === hover || i === picked;
			ctx.beginPath();
			ctx.arc(x, y, r, 0, Math.PI * 2);
			ctx.fillStyle = on ? blue : ink;
			ctx.globalAlpha = on ? 0.22 : 0.1;
			ctx.fill();
			ctx.globalAlpha = 1;
			ctx.lineWidth = on ? 1.5 : 1;
			ctx.strokeStyle = on ? blue : ink;
			ctx.globalAlpha = on ? 1 : 0.5;
			ctx.stroke();
			ctx.globalAlpha = 1;
		});
	}

	$effect(() => {
		// Re-read the reactive bits so a hover or a resize repaints.
		void [hover, picked, w, data];
		draw();
	});

	$effect(() => {
		if (!box) return;
		const ro = new ResizeObserver(([e]) => (w = Math.max(120, e.contentRect.width)));
		ro.observe(box);
		return () => ro.disconnect();
	});

	function nearest(e: PointerEvent): number | null {
		const p = proj;
		if (!p || !data || !el) return null;
		const r = el.getBoundingClientRect();
		const mx = e.clientX - r.left;
		const my = e.clientY - r.top;
		let best: number | null = null;
		let bestD = Infinity;
		data.stays.forEach((s, i) => {
			const d = Math.hypot(p.x(s.lon) - mx, p.y(s.lat) - my);
			// Within the dot, or close enough that you clearly meant it.
			if (d < Math.max(radius(s.minutes), 8) && d < bestD) {
				bestD = d;
				best = i;
			}
		});
		return best;
	}

	function fmtHours(m: number): string {
		const h = m / 60;
		if (h < 1) return `${Math.round(m)} min`;
		if (h < 24) return `${h.toFixed(1)} h`;
		return `${(h / 24).toFixed(1)} days`;
	}

	const active = $derived(
		data && (hover ?? picked) !== null ? data.stays[(hover ?? picked) as number] : null
	);
</script>

<div class="ground" bind:this={box}>
	{#if loading}
		<p class="quiet">Reading the ground…</p>
	{:else if !data?.bbox}
		<p class="quiet">No location recorded in this window.</p>
	{:else}
		<canvas
			bind:this={el}
			aria-label="Places visited in the selected window, drawn from your own trace."
			onpointermove={(e) => (hover = nearest(e))}
			onpointerleave={() => (hover = null)}
			onclick={(e) => {
				const i = nearest(e as unknown as PointerEvent);
				picked = i === picked ? null : i;
				onpick?.(i !== null && data ? data.stays[i] : null);
			}}
		></canvas>

		<p class="caption">
			{#if active}
				<span class="num">{fmtHours(active.minutes)}</span> over
				{active.visits}
				{active.visits === 1 ? 'visit' : 'visits'}
			{:else}
				{data.stays.length}
				{data.stays.length === 1 ? 'place' : 'places'} ·
				<span class="num">{data.track_total.toLocaleString()}</span> points
			{/if}
		</p>
	{/if}
</div>

<style>
	@reference "../../../app.css";

	.ground {
		margin-bottom: 0.75rem;
	}

	canvas {
		display: block;
		width: 100%;
		border: 1px solid var(--color-border);
		border-radius: 7px;
		cursor: crosshair;
	}

	.caption {
		margin: 0.3125rem 0 0;
		font-size: 0.625rem;
		color: var(--color-foreground-subtle);
	}

	.num {
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground);
	}

	.quiet {
		margin: 0 0 0.5rem;
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
	}
</style>
