<script lang="ts">
	/**
	 * The dots style, moving: a canvas that fills its parent and draws the
	 * field (lib/backgrounds/dots.ts) in the current theme's ink.
	 *
	 * It costs as little as it can, because the window's glass blurs whatever
	 * is under it and redraws that blur with every frame: at most 20 frames a
	 * second, none while the window is hidden, and one still frame under
	 * Reduce motion.
	 */
	import { drawDots, dotsOpacity } from "$lib/backgrounds/dots";
	import type { Appearance, DotsParams } from "$lib/backgrounds/resolve";
	import { motion } from "$lib/stores/motion.svelte";

	let { params, appearance }: { params: DotsParams; appearance: Appearance } = $props();

	/** The longest a frame is shown before the next: 20 frames a second. */
	const FRAME_MS = 50;
	/** The still frame's moment, a little into the motion so it is not the grid at rest. */
	const STILL_AT = 0.3;

	let canvas = $state<HTMLCanvasElement | null>(null);
	let width = $state(0);
	let height = $state(0);
	const start = performance.now();

	// The canvas's pixels follow its size on screen, up to twice for sharp displays.
	$effect(() => {
		if (!canvas || width === 0 || height === 0) return;
		const dpr = Math.min(window.devicePixelRatio || 1, 2);
		canvas.width = Math.round(width * dpr);
		canvas.height = Math.round(height * dpr);
	});

	// Draw, and keep drawing while the field may move. Runs again for a new
	// size, theme or picture.
	$effect(() => {
		const el = canvas;
		const [w, h, p, dark] = [width, height, params, appearance === "dark"];
		if (!el || w === 0 || h === 0) return;
		const ink = getComputedStyle(el).getPropertyValue("--color-foreground").trim() || (dark ? "#ededef" : "#1a2030");
		let raf = 0;
		let last = 0;

		const draw = (now: number) => {
			const ctx = el.getContext("2d");
			if (!ctx) return;
			const t = motion.reduced ? p.period * STILL_AT : (now - start) / 1000;
			ctx.setTransform(el.width / w, 0, 0, el.height / h, 0, 0);
			drawDots(ctx, w, h, p, t, ink, dotsOpacity(dark));
		};
		const frame = (now: number) => {
			raf = 0;
			if (document.hidden || motion.reduced) return;
			if (now - last >= FRAME_MS) {
				last = now;
				draw(now);
			}
			raf = requestAnimationFrame(frame);
		};
		const stop = () => {
			if (raf) cancelAnimationFrame(raf);
			raf = 0;
		};
		const run = () => {
			draw(performance.now());
			if (!raf && !document.hidden && !motion.reduced) raf = requestAnimationFrame(frame);
		};
		const onVisibility = () => (document.hidden ? stop() : run());

		document.addEventListener("visibilitychange", onVisibility);
		const offMotion = motion.watch(() => {
			stop();
			run();
		});
		run();
		return () => {
			stop();
			document.removeEventListener("visibilitychange", onVisibility);
			offMotion();
		};
	});
</script>

<div class="field" bind:clientWidth={width} bind:clientHeight={height}>
	<canvas bind:this={canvas} aria-hidden="true"></canvas>
</div>

<style>
	.field {
		width: 100%;
		height: 100%;
		pointer-events: none;
	}

	canvas {
		display: block;
		width: 100%;
		height: 100%;
	}
</style>
