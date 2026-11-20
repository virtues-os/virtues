<script lang="ts">
	/**
	 * The home background, filling its parent: the picture the background
	 * setting resolves to today in the current appearance, the bundled lake
	 * by default, or nothing under Plain (lib/stores/background.svelte.ts).
	 * The parent places and sizes it, and owns its mask and animation.
	 *
	 * A picture from a style has its own dark version, so only the lake is
	 * dimmed in Dark, through `--window-scene-filter`. The dots style is
	 * drawn by the app in the theme's ink (DotsField).
	 */
	import { onMount } from "svelte";
	import { background } from "$lib/stores/background.svelte";
	import DotsField from "$lib/components/backgrounds/DotsField.svelte";

	let {
		position = "50% 100%",
		dimLake = true,
	}: {
		/** `object-position` for the picture. */
		position?: string;
		/** Dim the lake in Dark. Off where the parent animates its own filter. */
		dimLake?: boolean;
	} = $props();

	onMount(() => background.start());

	const shown = $derived(background.shown);
</script>

{#if shown.kind === "lake"}
	<!-- AVIF where the browser takes it (200 KB), WebP on iOS 15 and older
	     macOS (174 KB), the JPEG last (508 KB). -->
	<picture>
		<source srcset="/scenes/lake.avif" type="image/avif" />
		<source srcset="/scenes/lake.webp" type="image/webp" />
		<img class:dim={dimLake} src="/scenes/lake.jpg" alt="" aria-hidden="true" decoding="async" style:object-position={position} />
	</picture>
{:else if shown.kind === "dots"}
	<DotsField params={shown.params} appearance={background.appearance} />
{:else if shown.kind === "picture"}
	{#key shown.key}
		<picture>
			<source srcset={shown.avif} sizes="100vw" type="image/avif" />
			<source srcset={shown.webp} sizes="100vw" type="image/webp" />
			<img src={shown.src} alt="" aria-hidden="true" decoding="async" style:object-position={position} />
		</picture>
	{/key}
{/if}

<style>
	picture {
		display: contents;
	}
	img {
		display: block;
		width: 100%;
		height: 100%;
		object-fit: cover;
		pointer-events: none;
		user-select: none;
	}
	img.dim {
		filter: var(--window-scene-filter);
	}
</style>
