<script lang="ts">
	/**
	 * The home background, filling its parent: the picture the background
	 * setting resolves to today in the current appearance, the bundled lake
	 * by default, or nothing under Plain (lib/stores/background.svelte.ts).
	 * The parent places and sizes it. Watercolors and drawn styles stand
	 * along the parent's bottom edge, `--strip-height` tall under
	 * `--strip-mask`; every other picture fills the parent (`isStrip`).
	 *
	 * A picture from a style has its own dark version. The lake, and a
	 * picture from a pack with one version of each, are dimmed in Dark
	 * through `--window-scene-filter`. A style the app draws itself (Dots)
	 * comes from its renderer, loaded the first time a setting needs it
	 * (lib/backgrounds/renderers.ts), and draws in the theme's ink. A site
	 * of yours draws nothing here: home shows it behind a new chat, where it
	 * can take clicks (ChatView).
	 *
	 * A change of picture crossfades. The new picture is a layer over the old
	 * one, kept clear until it has loaded and decoded, then faded in; the old
	 * layer goes once the fade ends. So stepping through pictures never shows
	 * an empty frame, and quick steps only fade in the last one to arrive.
	 * Drawn styles and Plain are not opaque, so the old layer fades out under
	 * them.
	 */
	import { onMount, untrack } from "svelte";
	import { background, isStrip, type Shown } from "$lib/stores/background.svelte";
	import { reducedMotion } from "$lib/stores/motion.svelte";
	import { loadRenderer } from "$lib/backgrounds/renderers";

	let {
		position = "50% 100%",
		dimLake = true,
	}: {
		/** `object-position` for the lake and for a picture along the bottom. */
		position?: string;
		/** Dim the lake in Dark. Off where the parent animates its own filter. */
		dimLake?: boolean;
	} = $props();

	/** How long a new picture takes to fade in. Matches `--fade` below. */
	const FADE_MS = 700;

	interface Layer {
		id: number;
		key: string;
		shown: Shown;
		ready: boolean;
	}

	let layers = $state<Layer[]>([]);
	let nextId = 0;

	onMount(() => background.start());

	function keyOf(s: Shown): string {
		return s.kind === "lake" || s.kind === "none" ? s.kind : `${s.kind}:${s.key}`;
	}

	const opaque = (s: Shown) => s.kind === "lake" || s.kind === "picture";

	$effect(() => {
		const shown = background.shown;
		const key = keyOf(shown);
		untrack(() => {
			if (layers.at(-1)?.key === key) return;
			const layer: Layer = { id: nextId++, key, shown, ready: false };
			layers.push(layer);
			// Nothing to load for these.
			if (!opaque(shown)) settle(layer.id);
		});
	});

	/**
	 * Fade a layer in, and drop the layers under it once the fade is done.
	 * `drew` is false for a picture that could not load.
	 */
	function settle(id: number, drew = true) {
		const layer = layers.find((l) => l.id === id);
		if (!layer || layer.ready) return;
		layer.ready = true;
		if (layers.at(-1)?.id === id) background.arrived = drew ? layer.shown : { kind: "none" };
		setTimeout(
			() => (layers = layers.filter((l) => l.id >= id)),
			reducedMotion() ? 0 : FADE_MS,
		);
	}

	/** Settle once the image has loaded and decoded, or failed to. */
	function loaded(img: HTMLImageElement, id: number) {
		let done = false;
		const finish = () => {
			if (done) return;
			done = true;
			img.decode().catch(() => {}).finally(() => settle(id, img.naturalWidth > 0));
		};
		if (img.complete && img.naturalWidth > 0) finish();
		img.addEventListener("load", finish);
		img.addEventListener("error", finish);
		return {
			destroy() {
				img.removeEventListener("load", finish);
				img.removeEventListener("error", finish);
			},
		};
	}

	// The newest layer that has arrived. When it is not opaque, the ones
	// under it fade out rather than staying behind it.
	const top = $derived(layers.findLast((l) => l.ready));
	const clearUnder = $derived(top ? !opaque(top.shown) : false);
</script>

<div class="stack" style:--fade="{FADE_MS}ms">
	{#each layers as layer (layer.id)}
		{@const shown = layer.shown}
		<div
			class="layer"
			class:strip={isStrip(shown)}
			class:in={layer.ready && !(clearUnder && top && layer.id < top.id)}
		>
			{#if shown.kind === "lake"}
				<!-- AVIF where the browser takes it (200 KB), WebP on iOS 15 and older
				     macOS (174 KB), the JPEG last (508 KB). -->
				<picture>
					<source srcset="/scenes/lake.avif" type="image/avif" />
					<source srcset="/scenes/lake.webp" type="image/webp" />
					<img use:loaded={layer.id} class:dim={dimLake} src="/scenes/lake.jpg" alt="" aria-hidden="true" decoding="async" style:object-position={position} />
				</picture>
			{:else if shown.kind === "drawn"}
				{#await loadRenderer(shown.renderer) then Drawn}
					<Drawn params={shown.params} appearance={background.appearance} />
				{/await}
			{:else if shown.kind === "picture"}
				<!-- Styles are AVIF; the img's own src is the widest file, which is
				     WebP only for a style built before AVIF only. -->
				<picture>
					<source srcset={shown.avif} sizes="100vw" type="image/avif" />
					<img use:loaded={layer.id} class:dim={shown.dim} src={shown.src} alt="" aria-hidden="true" decoding="async" style:object-position={shown.strip ? position : `${shown.focus[0] * 100}% ${shown.focus[1] * 100}%`} />
				</picture>
			{/if}
		</div>
	{/each}
</div>

<style>
	.stack {
		display: grid;
		grid-template: 100% / 100%;
		width: 100%;
		height: 100%;
	}
	.layer {
		grid-area: 1 / 1;
		height: 100%;
		min-width: 0;
		min-height: 0;
		opacity: 0;
		transition: opacity var(--fade) cubic-bezier(0.33, 0, 0.2, 1);
	}
	.layer.strip {
		align-self: end;
		height: var(--strip-height, 100%);
		-webkit-mask-image: var(--strip-mask, none);
		mask-image: var(--strip-mask, none);
	}
	.layer.in {
		opacity: 1;
	}
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
