<script lang="ts">
	/**
	 * The street map under today's route, drawn from your server's own map
	 * files ($lib/map/atlas), so no tile provider learns where you have been.
	 * It does not move. It is fitted to the day's places inside the route's
	 * frame, and it hands back how it places a point, so the route is drawn
	 * on the same ground. A server with no map files draws nothing here and
	 * hands back null, and the route keeps its own plain fit.
	 */
	import { onMount } from 'svelte';
	import 'leaflet/dist/leaflet.css';
	import type * as Leaflet from 'leaflet';
	import { atlasLayer, compactCredit } from '$lib/map/atlas';
	import { FRAME, type GeoBox, type Project } from '$lib/inbox/route';
	import { isDarkTheme } from '$lib/sidebar/pin-colors';

	let { box, onProject }: { box: GeoBox; onProject: (project: Project | null) => void } = $props();

	/** The closest the map goes, which is as far as your server's most detailed map files go. */
	const MAX_ZOOM = 15;

	let container = $state<HTMLDivElement | null>(null);
	let shown = $state(false);
	let map: Leaflet.Map | null = null;
	let basemap: Leaflet.Layer | null = null;

	function fit() {
		const m = map;
		if (!m || !basemap) return;
		m.fitBounds(
			[
				[box.lat0, box.lng0],
				[box.lat1, box.lng1],
			],
			{ paddingTopLeft: [FRAME.x, FRAME.top], paddingBottomRight: [FRAME.x, FRAME.bottom], maxZoom: MAX_ZOOM, animate: false },
		);
		// A new function each time, so the route is drawn again after the map moves.
		onProject((lat, lng) => {
			const p = m.latLngToContainerPoint([lat, lng]);
			return [p.x, p.y];
		});
	}

	/** Put on the basemap for the current theme, in place of the last one. */
	async function dress() {
		const layer = await atlasLayer(isDarkTheme() ? 'dark' : 'light');
		if (!map) return;
		basemap?.remove();
		basemap = layer;
		shown = !!layer;
		if (layer) {
			layer.addTo(map);
			fit();
		} else {
			onProject(null);
		}
	}

	// The day's ground grows as the day goes; the map follows it.
	const ground = $derived(`${box.lat0},${box.lat1},${box.lng0},${box.lng1}`);
	$effect(() => {
		void ground;
		fit();
	});

	onMount(() => {
		let gone = false;
		void (async () => {
			const leaflet = await import('leaflet');
			const L = ((leaflet as { default?: typeof Leaflet }).default ?? leaflet) as typeof Leaflet;
			if (gone || !container) return;
			map = L.map(container, {
				zoomControl: false,
				// The data credit is a condition of using the map files.
				attributionControl: true,
				// The fit is exact, not rounded to a whole zoom level.
				zoomSnap: 0,
				dragging: false,
				scrollWheelZoom: false,
				doubleClickZoom: false,
				boxZoom: false,
				keyboard: false,
				touchZoom: false,
			});
			map.attributionControl.setPrefix(false);
			compactCredit(map);
			await dress();
		})();
		const restyle = () => void dress();
		window.addEventListener('themechange', restyle);
		return () => {
			gone = true;
			window.removeEventListener('themechange', restyle);
			map?.remove();
			map = null;
			basemap = null;
			onProject(null);
		};
	});
</script>

<div class="ground" class:shown bind:this={container} aria-hidden="true"></div>

<style>
	/* Its own layer, so the map's panes stay under the route drawn over it. */
	.ground {
		position: absolute;
		inset: 0;
		z-index: 0;
		border-radius: var(--radius-xl);
		overflow: hidden;
		opacity: 0;
		transition: opacity var(--motion-state) var(--ease-premium);
	}
	.ground.shown {
		opacity: 1;
	}
	.ground:global(.leaflet-container) {
		background: var(--color-page);
		font-family: var(--font-sans);
		cursor: default;
	}
	/* The credit: there for anyone who looks, and never competing with the route. */
	.ground :global(.leaflet-control-attribution) {
		background: color-mix(in srgb, var(--color-page) 78%, transparent);
		color: var(--color-foreground-muted);
	}
	.ground :global(.leaflet-control-attribution a) {
		color: var(--color-foreground-muted);
		text-decoration: none;
	}

	@media (prefers-reduced-motion: reduce) {
		.ground {
			transition: none;
		}
	}
</style>
