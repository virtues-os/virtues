<script lang="ts">
	/**
	 * The ASCII style, moving: a scene from lib/backgrounds/ascii.ts drawn as
	 * characters in the shared canvas (DrawnField), in the current theme's ink.
	 */
	import { ASCII_STILL_AT, asciiOpacity, drawAscii } from "$lib/backgrounds/ascii";
	import type { AsciiParams, Appearance } from "$lib/backgrounds/resolve";
	import DrawnField from "./DrawnField.svelte";

	let { params, appearance }: { params: AsciiParams; appearance: Appearance } = $props();

	const draw = $derived((ctx: CanvasRenderingContext2D, w: number, h: number, t: number, ink: string, dark: boolean) =>
		drawAscii(ctx, w, h, params, t, ink, asciiOpacity(dark)),
	);
</script>

<DrawnField {draw} stillAt={ASCII_STILL_AT / params.speed} dark={appearance === "dark"} />
