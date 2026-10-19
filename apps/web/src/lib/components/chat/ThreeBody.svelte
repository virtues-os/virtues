<!--
	ThreeBody.svelte

	The empty chat's hidden figure. Send "∴" or "therefore" into a new chat and
	nothing is sent; the mark's three dots come loose instead and orbit one
	another under real gravity (`threeBody.ts`), starting in the figure-eight.

	Drag a body and let go to throw it: the drag becomes a change in its
	velocity, in whatever direction the camera is facing, so a throw can lift
	it out of the plane and the orbit turns chaotic in three dimensions. Drag
	empty space to turn the view. Esc or "Close" puts the mark back.

	Drawn like a figure in an old physics book: ink on the page's own paper,
	no color, a caption in the serif. A reduced-motion preference gets one
	still plate of the orbit instead of the animation.
-->

<script lang="ts">
	import { onMount } from "svelte";
	import { fade } from "svelte/transition";
	import TextAction from "$lib/components/TextAction.svelte";
	import {
		FIGURE_EIGHT_PERIOD,
		advance,
		centerOfMass,
		figureEight,
		type Bodies,
		type Vec3,
	} from "./threeBody";

	interface Props {
		onClose: () => void;
	}
	let { onClose }: Props = $props();

	const SIM_SPEED = 0.7; // time units per second: one loop of the eight ≈ 9s
	const TRAIL = 900;
	const BANDS = 10;
	const GRAB_RADIUS = 28;

	let canvas: HTMLCanvasElement;
	let caption: HTMLDivElement;
	let thrown = $state(false);

	let bodies: Bodies = figureEight();
	let trails: Vec3[][] = [[], [], []];
	let yaw = 0;
	let tilt = 0.95;
	let scale = 0;
	let reach = 1.1;

	function restart() {
		bodies = figureEight();
		trails = [[], [], []];
		thrown = false;
	}

	/** World → camera: turn about z by yaw, then tip back about x by tilt. */
	function toCamera(p: Vec3): Vec3 {
		const cy = Math.cos(yaw), sy = Math.sin(yaw);
		const x1 = p[0] * cy - p[1] * sy;
		const y1 = p[0] * sy + p[1] * cy;
		const ct = Math.cos(tilt), st = Math.sin(tilt);
		return [x1, y1 * ct - p[2] * st, y1 * st + p[2] * ct];
	}

	/** Camera → world, for turning a screen drag into a velocity. */
	function toWorld(c: Vec3): Vec3 {
		const ct = Math.cos(tilt), st = Math.sin(tilt);
		const y1 = c[1] * ct + c[2] * st;
		const z = -c[1] * st + c[2] * ct;
		const cy = Math.cos(yaw), sy = Math.sin(yaw);
		return [c[0] * cy + y1 * sy, -c[0] * sy + y1 * cy, z];
	}

	onMount(() => {
		const ctx = canvas.getContext("2d")!;
		const still = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
		let width = 0;
		let height = 0;
		let raf = 0;
		let last = performance.now();

		let grabbed = -1;
		let turning = false;
		let pointer: [number, number] = [0, 0];
		let lastPointer: [number, number] = [0, 0];

		const resize = () => {
			const dpr = window.devicePixelRatio || 1;
			width = canvas.clientWidth;
			height = canvas.clientHeight;
			canvas.width = Math.round(width * dpr);
			canvas.height = Math.round(height * dpr);
			ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
			if (still) draw();
		};
		const observer = new ResizeObserver(resize);
		observer.observe(canvas);

		// The figure fills the band between the caption and the composer, which
		// floats at the pane's center on a desktop and docks to the bottom on a
		// phone — so it is measured each frame, not assumed.
		let band: [number, number] = [0, 0];
		function measureBand() {
			const top = canvas.getBoundingClientRect().top;
			const ceiling = caption.getBoundingClientRect().bottom - top + 16;
			const composer = canvas
				.closest(".page-container")
				?.querySelector(".chat-input-container")
				?.getBoundingClientRect();
			const floor = (composer ? composer.top - top : height) - 16;
			band = [ceiling, Math.max(floor, ceiling + 120)];
		}
		const center = (): [number, number] => [width / 2, (band[0] + band[1]) / 2];

		function project(p: Vec3, com: Vec3): { x: number; y: number; depth: number; k: number } {
			const c = toCamera([p[0] - com[0], p[1] - com[1], p[2] - com[2]]);
			const k = 5 / (5 - c[2] / reach);
			const [cx, cy] = center();
			return { x: cx + c[0] * scale * k, y: cy - c[1] * scale * k, depth: c[2], k };
		}

		function draw() {
			measureBand();
			const ink = getComputedStyle(canvas).color;
			ctx.clearRect(0, 0, width, height);
			ctx.strokeStyle = ink;
			ctx.fillStyle = ink;

			const com = centerOfMass(bodies);
			let far = 0;
			for (const p of bodies.pos) {
				far = Math.max(far, Math.hypot(p[0] - com[0], p[1] - com[1], p[2] - com[2]));
			}
			// Follow the system out as it spreads, but only so far: once a body
			// is flung clear, the pair left behind stays the subject.
			reach += (Math.min(Math.max(far, 1.1), 6) - reach) * 0.02;
			// Tipped back by `tilt`, the eight's height on screen is its reach
			// foreshortened, whichever way the slow turn has it pointing.
			const target = Math.min(
				(width * 0.36) / reach,
				(band[1] - band[0]) / 2 / (reach * Math.cos(tilt) + 0.15),
			);
			scale = scale === 0 || still ? target : scale + (target - scale) * 0.05;

			ctx.lineWidth = 1.25;
			ctx.lineCap = "round";
			for (const trail of trails) {
				const n = trail.length;
				if (n < 2) continue;
				const pts = trail.map((p) => project(p, com));
				for (let b = 0; b < BANDS; b++) {
					const from = Math.floor((b * (n - 1)) / BANDS);
					const to = Math.floor(((b + 1) * (n - 1)) / BANDS);
					if (to <= from) continue;
					ctx.globalAlpha = 0.05 + (0.45 * (b + 1)) / BANDS;
					ctx.beginPath();
					ctx.moveTo(pts[from].x, pts[from].y);
					for (let i = from + 1; i <= to; i++) ctx.lineTo(pts[i].x, pts[i].y);
					ctx.stroke();
				}
			}

			if (grabbed >= 0) {
				const p = project(bodies.pos[grabbed], com);
				ctx.globalAlpha = 0.5;
				ctx.setLineDash([3, 4]);
				ctx.beginPath();
				ctx.moveTo(p.x, p.y);
				ctx.lineTo(pointer[0], pointer[1]);
				ctx.stroke();
				ctx.setLineDash([]);
			}

			ctx.globalAlpha = 1;
			const dots = bodies.pos.map((p) => project(p, com)).sort((a, b) => a.depth - b.depth);
			for (const d of dots) {
				ctx.beginPath();
				ctx.arc(d.x, d.y, 5 * d.k, 0, Math.PI * 2);
				ctx.fill();
			}
		}

		function record() {
			for (let i = 0; i < 3; i++) {
				trails[i].push([...bodies.pos[i]] as Vec3);
				if (trails[i].length > TRAIL) trails[i].shift();
			}
		}

		function frame(now: number) {
			const dt = Math.min((now - last) / 1000, 1 / 20);
			last = now;
			advance(bodies, dt * SIM_SPEED);
			record();
			if (!turning) yaw += dt * 0.05;
			draw();
			raf = requestAnimationFrame(frame);
		}

		if (still) {
			// One loop of the eight, laid down as a plate.
			const steps = TRAIL;
			for (let s = 0; s < steps; s++) {
				advance(bodies, FIGURE_EIGHT_PERIOD / steps);
				record();
			}
			resize();
		} else {
			resize();
			raf = requestAnimationFrame(frame);
		}

		const onDown = (e: PointerEvent) => {
			if (still) return;
			const rect = canvas.getBoundingClientRect();
			pointer = [e.clientX - rect.left, e.clientY - rect.top];
			lastPointer = pointer;
			const com = centerOfMass(bodies);
			let best = GRAB_RADIUS;
			grabbed = -1;
			bodies.pos.forEach((p, i) => {
				const d = project(p, com);
				const dist = Math.hypot(d.x - pointer[0], d.y - pointer[1]);
				if (dist < best) {
					best = dist;
					grabbed = i;
				}
			});
			turning = grabbed < 0;
			canvas.setPointerCapture(e.pointerId);
		};

		const onMove = (e: PointerEvent) => {
			if (grabbed < 0 && !turning) return;
			const rect = canvas.getBoundingClientRect();
			pointer = [e.clientX - rect.left, e.clientY - rect.top];
			if (turning) {
				yaw += (pointer[0] - lastPointer[0]) * 0.008;
				tilt = Math.min(1.5, Math.max(0, tilt + (pointer[1] - lastPointer[1]) * 0.008));
			}
			lastPointer = pointer;
		};

		const onUp = () => {
			if (grabbed >= 0) {
				const p = project(bodies.pos[grabbed], centerOfMass(bodies));
				// A drag of the eight's own width is roughly an orbital speed.
				const dx = (pointer[0] - p.x) / scale;
				const dy = -(pointer[1] - p.y) / scale;
				const kick = toWorld([dx * 0.6, dy * 0.6, 0]);
				for (let k = 0; k < 3; k++) bodies.vel[grabbed][k] += kick[k];
				thrown = true;
			}
			grabbed = -1;
			turning = false;
		};

		const onKey = (e: KeyboardEvent) => {
			if (e.key === "Escape") onClose();
		};

		canvas.addEventListener("pointerdown", onDown);
		canvas.addEventListener("pointermove", onMove);
		canvas.addEventListener("pointerup", onUp);
		canvas.addEventListener("pointercancel", onUp);
		window.addEventListener("keydown", onKey);

		return () => {
			cancelAnimationFrame(raf);
			observer.disconnect();
			canvas.removeEventListener("pointerdown", onDown);
			canvas.removeEventListener("pointermove", onMove);
			canvas.removeEventListener("pointerup", onUp);
			canvas.removeEventListener("pointercancel", onUp);
			window.removeEventListener("keydown", onKey);
		};
	});
</script>

<div class="three-body" transition:fade={{ duration: 300 }}>
	<canvas
		bind:this={canvas}
		role="img"
		aria-label="Three equal masses orbiting one another in a figure-eight"
	></canvas>
	<div class="caption" bind:this={caption}>
		{#if thrown}
			<p>Chaotic now. The smallest change in a throw gives a different future.</p>
		{:else}
			<p>
				Three equal masses, one figure-eight orbit (Chenciner and Montgomery, 2000). Drag a
				body to throw it.
			</p>
		{/if}
		<div class="actions">
			{#if thrown}
				<TextAction onclick={restart}>Start over</TextAction>
			{/if}
			<TextAction quiet onclick={onClose}>Close</TextAction>
		</div>
	</div>
</div>

<style>
	.three-body {
		position: absolute;
		inset: 0;
		z-index: 2;
		color: var(--color-foreground);
	}

	canvas {
		display: block;
		width: 100%;
		height: 100%;
		touch-action: none;
		cursor: grab;
	}

	canvas:active {
		cursor: grabbing;
	}

	.caption {
		position: absolute;
		top: 1.5rem;
		left: 0;
		right: 0;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.5rem;
		padding: 0 1rem;
		text-align: center;
		pointer-events: none;
	}

	.caption p {
		max-width: 28rem;
		margin: 0;
		font-family: var(--font-serif);
		font-size: 0.9375rem;
		color: var(--color-foreground-muted);
	}

	.actions {
		display: flex;
		gap: 1rem;
		pointer-events: auto;
	}
</style>
