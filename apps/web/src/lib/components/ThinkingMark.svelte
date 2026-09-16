<script lang="ts">
	/**
	 * THE ∴ IN MOTION — how deep it is going, in how many dimensions it stands.
	 *
	 * The mark is already an argument: two dots, then a third. So while a turn
	 * is working it does not spin like a loader — it gains and sheds dimensions,
	 * and the number of dots on screen means one thing and only one thing:
	 *
	 *   1 dot   a point        the answer landed
	 *   3 dots  a triangle     reasoning here, with what is already loaded
	 *   4 dots  a tetrahedron  gone out to something: the record, a tool, a file
	 *   5 dots  a five-cell    many passes at once, or a turn that is taking a while
	 *
	 * The extra dots are not decoration added on top. Lifting a simplex one
	 * dimension puts the new vertex on the CENTROID of the figure that already
	 * exists and raises it along an axis nothing else uses, while the old
	 * vertices slide back by h/(n+1). That is the real construction, and it is
	 * why the growth reads as the figure deepening rather than a dot appearing.
	 *
	 * Everything starts and ends at rotation zero with the extra dots retracted,
	 * so the last frame of every movement is the logo, exactly as drawn. That is
	 * also why a change of depth waits for the current loop to come home before
	 * it switches: at the boundary the two poses are identical, so the cut is
	 * invisible. The one exception is the landing, which has to be prompt — it
	 * collapses from wherever the mark happens to be (see `landingAt`).
	 *
	 * Geometry is the app icon's, exactly: equilateral, side 15, r 3, on the
	 * 24-unit box `virtues:logo` uses in icons.ts. The viewBox is padded to
	 * 30 so the deeper figures have somewhere to be without clipping.
	 * Rotation is about the CENTROID (12, 13.667) — about the box center
	 * instead, a 120° turn walks the mark 3.75 units off and you see the wobble.
	 */

	interface Props {
		/** 1, 3, 4 or 5 dots. See the table above — this is a claim, not a mood. */
		depth?: 1 | 3 | 4 | 5;
		/** The turn has been going a while: prefer the calmer, endless movement. */
		sustained?: boolean;
		/** Rendered size in px. 16 sits with 14px text. */
		size?: number;
	}

	let { depth = 3, sustained = false, size = 16 }: Props = $props();

	/* ---- the mark, in numbers ------------------------------------------- */

	const CX = 12;
	const CY = 41 / 3; // centroid: (5 + 18 + 18) / 3
	const BASE: [number, number][] = [
		[0, -8.66667], // apex      (12, 5)
		[-7.5, 4.33333], // base left  (4.5, 18)
		[7.5, 4.33333], // base right (19.5, 18)
	];
	const R2 = 8.66667; // circumradius of the triangle
	const R3 = 9.18559; // ... of the tetrahedron, same edge
	const R4 = 9.48683; // ... of the five-cell, same edge
	const H3 = 12.24745; // 15·√(2/3) — how far the fourth vertex rises
	const H4 = 11.85854; // 15·√(5/8) — how far the fifth does
	const DIST = 52; // projection distance, in the same 24 units

	type Track = [number, number][];
	interface Script {
		dur: number;
		tilt: Track;
		spin: Track;
		hyper: Track;
		m3: Track;
		m4: Track;
		/** Positions never move; only radius and opacity, in premise order. */
		pulse?: boolean;
		/** Spin at a constant rate rather than easing in and out. */
		linear?: boolean;
	}

	const FLAT: Track = [
		[0, 0],
		[1, 0],
	];

	const SCRIPTS: Record<string, Script> = {
		// 3 dots, the ordinary case: left premise, right premise, therefore.
		syllogism: { dur: 1.6, tilt: FLAT, spin: FLAT, hyper: FLAT, m3: FLAT, m4: FLAT, pulse: true },

		// 3 dots, sustained: still three, it just stops being flat.
		fold: {
			dur: 4.0,
			tilt: [[0, 0], [0.1, 0], [0.3, 60], [0.76, 60], [0.94, 0], [1, 0]],
			spin: [[0, 0], [0.16, 0], [0.86, 360], [1, 360]],
			hyper: FLAT,
			m3: FLAT,
			m4: FLAT,
		},

		// 4 dots: a vertex rises out of the middle and the mark stands up.
		fourth: {
			dur: 4.4,
			tilt: [[0, 0], [0.1, 0], [0.3, 58], [0.74, 58], [0.9, 0], [1, 0]],
			spin: [[0, 0], [0.18, 0], [0.84, 360], [1, 360]],
			hyper: FLAT,
			m3: [[0, 0], [0.1, 0], [0.29, 1], [0.72, 1], [0.88, 0], [1, 0]],
			m4: FLAT,
		},

		// 4 dots, sustained: a coin spinning on a table, never quite falling.
		precess: {
			dur: 4.2,
			tilt: [[0, 56], [1, 56]],
			spin: FLAT,
			hyper: FLAT,
			m3: [[0, 0], [0.12, 1], [1, 1]],
			m4: FLAT,
			linear: true,
		},

		// 5 dots: up through both dimensions, a turn through w, and back down.
		descent: {
			dur: 7.6,
			tilt: [[0, 0], [0.06, 0], [0.21, 54], [0.64, 54], [0.74, 0], [1, 0]],
			spin: [[0, 0], [0.1, 0], [0.72, 360], [1, 360]],
			hyper: [[0, 0], [0.29, 0], [0.57, 360], [1, 360]],
			m3: [[0, 0], [0.06, 0], [0.18, 1], [0.62, 1], [0.71, 0], [1, 0]],
			m4: [[0, 0], [0.18, 0], [0.3, 1], [0.55, 1], [0.63, 0], [1, 0]],
		},
	};

	/** Premise, premise, conclusion — the apex swells more and holds longer. */
	const PULSE_SCALE: Track = [[0, 1], [0.11, 1.22], [0.36, 1], [1, 1]];
	const PULSE_OPACITY: Track = [[0, 0.34], [0.11, 1], [0.36, 0.5], [1, 0.34]];
	const APEX_SCALE: Track = [[0, 1], [0.11, 1.26], [0.44, 1.05], [0.72, 1], [1, 1]];
	const APEX_OPACITY: Track = [[0, 0.34], [0.11, 1], [0.44, 0.94], [0.72, 0.44], [1, 0.34]];
	/** 140ms of the 1.6s loop. Under ~100ms the three stop reading as an order. */
	const STAGGER = 0.0875;

	const movement = $derived(
		depth === 5
			? "descent"
			: depth === 4
				? sustained
					? "precess"
					: "fourth"
				: sustained
					? "fold"
					: "syllogism",
	);

	/* ---- the frame ------------------------------------------------------- */

	function smooth(t: number): number {
		return t * t * (3 - 2 * t);
	}

	function track(p: number, kf: Track): number {
		if (p <= kf[0][0]) return kf[0][1];
		for (let i = 1; i < kf.length; i++) {
			if (p <= kf[i][0]) {
				const [pa, va] = kf[i - 1];
				const [pb, vb] = kf[i];
				const span = pb - pa;
				return va + (vb - va) * smooth(span <= 0 ? 1 : (p - pa) / span);
			}
		}
		return kf[kf.length - 1][1];
	}

	type Dot = [cx: number, cy: number, r: number, opacity: number, z: number];

	function poseOf(script: Script, p: number): Dot[] {
		if (script.pulse) {
			return BASE.map((b, i) => {
				// The base pair leads; the apex concludes.
				const phase = (p - (i === 1 ? 0 : i === 2 ? STAGGER : STAGGER * 2) + 2) % 1;
				const apex = i === 0;
				const s = track(phase, apex ? APEX_SCALE : PULSE_SCALE);
				const o = track(phase, apex ? APEX_OPACITY : PULSE_OPACITY);
				return [CX + b[0], CY + b[1], 3 * s, o, 0] as Dot;
			}).concat([
				[CX, CY, 0, 0, 0],
				[CX, CY, 0, 0, 0],
			]);
		}

		const m3 = track(p, script.m3);
		const m4 = track(p, script.m4);

		// Lift: the new vertex starts on the centroid, the old ones slide back.
		const pts: number[][] = BASE.map((b) => [b[0], b[1], -H3 * 0.25 * m3, 0]);
		pts.push([0, 0, H3 * 0.75 * m3, 0]);
		const w0 = -H4 * 0.2 * m4;
		for (let i = 0; i < 4; i++) pts[i][3] = w0;
		pts.push([0, 0, 0, H4 * 0.8 * m4]);

		// A deeper figure is a wider one, so hold the apparent size steady:
		// normalise back to the triangle's circumradius as the dimensions rise.
		const radius = (R2 * (1 - m3) + R3 * m3) * (1 - m4) + R4 * m4;
		const fit = R2 / radius;

		const tilt = (track(p, script.tilt) * Math.PI) / 180;
		const spin = ((script.linear ? 360 * p : track(p, script.spin)) * Math.PI) / 180;
		const hyper = (track(p, script.hyper) * Math.PI) / 180;
		const ct = Math.cos(tilt);
		const st = Math.sin(tilt);
		const cs = Math.cos(spin);
		const ss = Math.sin(spin);
		const ch = Math.cos(hyper);
		const sh = Math.sin(hyper);

		return pts.map((pt, i) => {
			let x = pt[0] * fit;
			let y = pt[1] * fit;
			let z = pt[2] * fit;
			let w = pt[3] * fit;
			let t: number;
			// spin about the vertical screen axis — the xz plane
			t = x * cs - z * ss;
			z = x * ss + z * cs;
			x = t;
			// the turn through w — the xw plane
			t = x * ch - w * sh;
			w = x * sh + w * ch;
			x = t;
			// tilt about the horizontal screen axis — the yz plane
			t = y * ct - z * st;
			z = y * st + z * ct;
			y = t;

			const k4 = DIST / Math.max(8, DIST - w);
			const k3 = DIST / Math.max(8, DIST - z * k4);
			const k = k3 * k4;
			// Depth is carried by size and solidity only. No gradients, no
			// shadows — nothing that stops being legible at 15px.
			const grown = i < 3 ? 1 : i === 3 ? m3 : m4;
			return [
				CX + x * k,
				CY + y * k,
				3 * (1 + (k - 1) * 0.62) * grown,
				Math.max(0.3, Math.min(1, 0.45 + (k - 0.62) * 1.45)),
				z,
			] as Dot;
		});
	}

	const REST: Dot[] = poseOf(SCRIPTS.syllogism, 0).map(
		(d, i) => [d[0], d[1], i < 3 ? 3 : 0, i < 3 ? 1 : 0, 0] as Dot,
	);

	/** The landing: fall together from wherever you are, then open as the mark. */
	const LAND_MS = 2000;
	const LAND_CONV: Track = [[0, 0], [0.35, 1], [0.62, 1], [1, 0]];

	function converge(pose: Dot[], amount: number): Dot[] {
		if (amount <= 0) return pose;
		return pose.map((d, i) => [
			d[0] + (CX - d[0]) * amount,
			d[1] + (CY - d[1]) * amount,
			i < 3 ? d[2] + (4.3 - d[2]) * amount : d[2] * (1 - amount),
			d[3] + (1 - d[3]) * amount,
			d[4],
		]) as Dot[];
	}

	/* ---- the loop -------------------------------------------------------- */

	let circles = $state<(SVGCircleElement | null)[]>([null, null, null, null, null]);

	function paint(pose: Dot[]) {
		for (let i = 0; i < 5; i++) {
			const c = circles[i];
			if (!c) continue;
			c.setAttribute("cx", pose[i][0].toFixed(2));
			c.setAttribute("cy", pose[i][1].toFixed(2));
			c.setAttribute("r", Math.max(0, pose[i][2]).toFixed(2));
			c.setAttribute("opacity", pose[i][3].toFixed(2));
		}
	}

	/**
	 * One loop for the life of the component. It does NOT restart when the
	 * depth changes: `movement` and `depth` are read inside the frame, so a
	 * change is picked up without tearing down the clock — which is the whole
	 * point, since the switch is deliberately held until the loop comes home.
	 */
	$effect(() => {
		const query =
			typeof window === "undefined"
				? null
				: window.matchMedia("(prefers-reduced-motion: reduce)");
		let reduced = query?.matches ?? false;

		let frame = 0;
		let last = performance.now();
		let elapsed = 0;
		let running = "syllogism";
		let frozen: Dot[] | null = null;
		let landedMs = -1;

		const step = (now: number) => {
			frame = requestAnimationFrame(step);
			const dt = Math.min(0.05, (now - last) / 1000);
			last = now;

			if (reduced) {
				// Nothing moves. The mark is the mark; the words carry the status.
				paint(REST);
				return;
			}

			if (depth === 1) {
				// The landing has to be prompt, so it is the one change that does
				// not wait for the loop: freeze the pose the turn ended on and
				// collapse out of it. Past halfway every dot is on the centre
				// anyway, so swapping in the resting mark there cannot be seen —
				// and the point opens into ∴ rather than a half-turned pose.
				if (landedMs < 0) {
					frozen = poseOf(SCRIPTS[running], elapsed / SCRIPTS[running].dur);
					landedMs = 0;
				}
				landedMs += dt * 1000;
				const l = Math.min(1, landedMs / LAND_MS);
				paint(converge(l < 0.5 ? (frozen ?? REST) : REST, track(l, LAND_CONV)));
				return;
			}

			landedMs = -1;
			frozen = null;
			elapsed += dt;
			if (elapsed >= SCRIPTS[running].dur) {
				elapsed %= SCRIPTS[running].dur;
				// Home again: the first and last frame of every movement is the
				// mark itself, so this is the one moment a change of depth costs
				// nothing to make.
				running = movement;
			}
			paint(poseOf(SCRIPTS[running], elapsed / SCRIPTS[running].dur));
		};

		const onQuery = (e: MediaQueryListEvent) => {
			reduced = e.matches;
		};
		query?.addEventListener("change", onQuery);
		frame = requestAnimationFrame(step);

		return () => {
			cancelAnimationFrame(frame);
			query?.removeEventListener("change", onQuery);
		};
	});
</script>

<svg
	class="thinking-mark"
	viewBox="-3 -3 30 30"
	width={size}
	height={size}
	aria-hidden="true"
	focusable="false"
>
	<!-- Three dots, always drawn; two more that live at r 0 until a dimension
	     needs them. Order is apex, base left, base right — premise order is the
	     animation's, not the DOM's. -->
	<circle bind:this={circles[0]} cx="12" cy="5" r="3" />
	<circle bind:this={circles[1]} cx="4.5" cy="18" r="3" />
	<circle bind:this={circles[2]} cx="19.5" cy="18" r="3" />
	<circle bind:this={circles[3]} cx="12" cy="13.67" r="0" opacity="0" />
	<circle bind:this={circles[4]} cx="12" cy="13.67" r="0" opacity="0" />
</svg>

<style>
	.thinking-mark {
		display: block;
		flex: none;
		align-self: center;
		color: var(--color-foreground-muted);
	}

	.thinking-mark circle {
		fill: currentColor;
	}
</style>
