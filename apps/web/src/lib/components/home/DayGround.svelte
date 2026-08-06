<!--
	DayGround.svelte — the day's ground track.

	The same GPS fixes the deck's `move` lane measures, drawn in space instead
	of time. It exists to be scrubbed: pointing at an hour on the deck puts a
	dot here, so "where was I at 2pm" is answered by moving the mouse rather
	than by reading two charts and doing the join in your head.

	Equirectangular, cosine-corrected at the track's own latitude. Over a day's
	worth of movement that is exact enough that a projection library would only
	add weight.
-->
<script lang="ts">
	import type { TimelineDayPoint } from "$lib/wiki/api";

	interface Props {
		points: TimelineDayPoint[];
		/** Instant to mark, or null for the live head. */
		scrubMs?: number | null;
		nowMs: number;
	}
	let { points, scrubMs = null, nowMs }: Props = $props();

	const reduce =
		typeof window !== "undefined" &&
		window.matchMedia &&
		window.matchMedia("(prefers-reduced-motion: reduce)").matches;

	let canvas = $state<HTMLCanvasElement | undefined>(undefined);

	function cssvar(n: string): string {
		return getComputedStyle(document.documentElement).getPropertyValue(n).trim() || "#1a2030";
	}
	function rgba(hex: string, a: number): string {
		let h = hex.replace("#", "");
		if (h.length === 3) h = h.split("").map((c) => c + c).join("");
		const n = parseInt(h, 16);
		return `rgba(${(n >> 16) & 255},${(n >> 8) & 255},${n & 255},${a})`;
	}

	/** The fix nearest an instant — what the scrub dot marks. */
	const marked = $derived.by(() => {
		if (!points.length) return null;
		const t = scrubMs ?? nowMs;
		let best = points[0],
			gap = Infinity;
		for (const p of points) {
			const g = Math.abs(Date.parse(p.timestamp) - t);
			if (g < gap) {
				gap = g;
				best = p;
			}
		}
		// Beyond half an hour there is no fix worth claiming as "here".
		return gap > 30 * 60_000 ? null : best;
	});

	// The loop depends on the canvas and the track, and deliberately not on the
	// mark: reading `marked` here would make every pointer move tear down the
	// loop and replay the draw-in, which is precisely what scrubbing does. The
	// mark is read inside `frame`, outside the tracking context.
	$effect(() => {
		const cv = canvas,
			pts = points;
		if (!cv || pts.length < 2) return;
		let raf = 0,
			start = 0;

		function frame(ts: number) {
			const mk = marked;
			if (!start) start = ts;
			const grow = reduce ? 1 : Math.min(1, (ts - start) / 1200);
			const W = cv!.clientWidth,
				H = cv!.clientHeight,
				pad = 14;
			if (W < 8 || H < 8) {
				raf = requestAnimationFrame(frame);
				return;
			}
			const d = Math.min(window.devicePixelRatio || 1, 2);
			if (cv!.width !== Math.round(W * d)) {
				cv!.width = Math.round(W * d);
				cv!.height = Math.round(H * d);
			}
			const c = cv!.getContext("2d");
			if (!c) return;
			c.setTransform(d, 0, 0, d, 0, 0);
			c.clearRect(0, 0, W, H);

			let mnLa = Infinity, mxLa = -Infinity, mnLo = Infinity, mxLo = -Infinity;
			for (const q of pts) {
				mnLa = Math.min(mnLa, q.latitude); mxLa = Math.max(mxLa, q.latitude);
				mnLo = Math.min(mnLo, q.longitude); mxLo = Math.max(mxLo, q.longitude);
			}
			const k = Math.cos((((mnLa + mxLa) / 2) * Math.PI) / 180) || 1;
			const spanX = Math.max((mxLo - mnLo) * k, 1e-4),
				spanY = Math.max(mxLa - mnLa, 1e-4);
			const sc = Math.min((W - 2 * pad) / spanX, (H - 2 * pad) / spanY);
			const offX = (W - spanX * sc) / 2,
				offY = (H - spanY * sc) / 2;
			const X = (lo: number) => offX + (lo - mnLo) * k * sc;
			const Y = (la: number) => offY + (mxLa - la) * sc;

			const step = Math.max(1, Math.floor(pts.length / 420));
			const drawn = pts.filter((_, i) => i % step === 0 || i === pts.length - 1);
			const n = Math.max(2, Math.floor(drawn.length * grow));
			const fg = cssvar("--color-foreground"),
				accent = cssvar("--color-primary");

			c.beginPath();
			for (let i = 0; i < n; i++) {
				const q = drawn[i];
				i ? c.lineTo(X(q.longitude), Y(q.latitude)) : c.moveTo(X(q.longitude), Y(q.latitude));
			}
			c.strokeStyle = rgba(fg, 0.42);
			c.lineWidth = 1.2;
			c.lineJoin = "round";
			c.lineCap = "round";
			c.stroke();

			if (mk && grow >= 1) {
				const hx = X(mk.longitude),
					hy = Y(mk.latitude);
				// The live head breathes; a scrubbed instant sits still, because
				// it is a fact about the past rather than something in progress.
				const pu = scrubMs == null && !reduce ? 0.5 + 0.5 * Math.sin(ts / 760) : 0.5;
				const g = c.createRadialGradient(hx, hy, 0, hx, hy, 7 + pu * 3);
				g.addColorStop(0, rgba(accent, 0.28));
				g.addColorStop(1, rgba(accent, 0));
				c.fillStyle = g;
				c.beginPath();
				c.arc(hx, hy, 7 + pu * 3, 0, 6.29);
				c.fill();
				c.fillStyle = accent;
				c.beginPath();
				c.arc(hx, hy, 2.5, 0, 6.29);
				c.fill();
			}
			raf = requestAnimationFrame(frame);
		}
		raf = requestAnimationFrame(frame);
		return () => {
			if (raf) cancelAnimationFrame(raf);
		};
	});
</script>

<canvas bind:this={canvas} aria-hidden="true"></canvas>

<style>
	canvas { display: block; width: 100%; height: 100%; }
</style>
