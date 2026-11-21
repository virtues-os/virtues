<script lang="ts">
	/**
	 * The dots style, moving: the field from lib/backgrounds/dots.ts drawn in
	 * the shared canvas (DrawnField), in the current theme's ink.
	 */
	import { drawDots, dotsOpacity } from "$lib/backgrounds/dots";
	import type { Appearance, DotsParams } from "$lib/backgrounds/resolve";
	import DrawnField from "./DrawnField.svelte";

	let { params, appearance }: { params: DotsParams; appearance: Appearance } = $props();

	/** The still frame's moment, a little into the motion so it is not the grid at rest. */
	const STILL_AT = 0.3;

	const draw = $derived((ctx: CanvasRenderingContext2D, w: number, h: number, t: number, ink: string, dark: boolean) =>
		drawDots(ctx, w, h, params, t, ink, dotsOpacity(dark)),
	);
</script>

<DrawnField {draw} stillAt={params.period * STILL_AT} dark={appearance === "dark"} />
