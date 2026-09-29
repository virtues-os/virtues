<!--
	TimelineView.svelte — our Timeline instrument, map mode.

	Step 1 (scaffold): a MapLibre map on the box's own atlas tiles (Protomaps,
	served from /api/map/vt), rendered inside Virtues via the Timeline room.
	Step 2 (here): draw the day's REAL GPS track from /api/timeline/day's raw
	points and frame the map to it. Our resolved stays and our framing/coverage
	logic follow.

	Our own map (MapLibre v6), not the Leaflet MovementMap: the prototype's map
	logic — the route with signal-gap bridges, the coverage-aware framing — is
	MapLibre-native, and this is where it gets ported.
-->
<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { browser } from '$app/environment';
	import { atlasStyle } from '$lib/map/atlas';
	import { apiGet } from '$lib/api/client';
	import { getLocalDateSlug } from '$lib/utils/dateUtils';
	import type { Map as MlMap } from 'maplibre-gl';
	import type { TimelineDayView } from '$lib/wiki/api';

	let container: HTMLDivElement;
	let map: MlMap | null = null;

	onMount(() => {
		if (!browser) return;
		let cancelled = false;
		(async () => {
			// MapLibre's worker sits where the bundler put it, not next to the
			// module, so set it explicitly — the same way $lib/map/atlas does.
			const [maplibre, workerUrl] = await Promise.all([
				import('maplibre-gl'),
				import('maplibre-gl/dist/maplibre-gl-worker.mjs?worker&url').then((m) => m.default),
				import('maplibre-gl/dist/maplibre-gl.css'),
			]);
			maplibre.setWorkerUrl(workerUrl as string);
			if (cancelled || !container) return;
			// A null style means the box holds no map tiles yet (seeded dev, or the
			// box restarting): draw an empty style so the container still mounts and
			// the track still renders on a blank background. Real tiles need the
			// box's own archives — make dev-real.
			const style = await atlasStyle('light');
			if (cancelled || !container) return;
			const m = new maplibre.Map({
				container,
				style: style ?? { version: 8, sources: {}, layers: [] },
				center: [0, 20],
				zoom: 1.5,
				// v6: attributionControl takes options (or false), not a bare boolean.
				attributionControl: {},
			});
			map = m;
			m.on('load', () => {
				if (!cancelled) void drawTrack(maplibre, m);
			});
		})();
		return () => {
			cancelled = true;
		};
	});

	// Fetch the day's raw GPS points and draw the path. Today for now; date
	// navigation (and our resolved stays) are the next steps.
	async function drawTrack(maplibre: typeof import('maplibre-gl'), m: MlMap) {
		let day: TimelineDayView | null = null;
		try {
			day = await apiGet<TimelineDayView>(`/api/timeline/day/${getLocalDateSlug()}`);
		} catch (e) {
			console.warn('[Timeline] day fetch failed', e);
			return;
		}
		const pts = day?.points ?? [];
		if (pts.length < 2) return; // no path to draw — an honest empty map

		const coords: [number, number][] = pts.map((p) => [p.longitude, p.latitude]);
		const line: GeoJSON.Feature<GeoJSON.LineString> = {
			type: 'Feature',
			properties: {},
			geometry: { type: 'LineString', coordinates: coords },
		};
		m.addSource('track', { type: 'geojson', data: line });
		m.addLayer({
			id: 'track-line',
			type: 'line',
			source: 'track',
			layout: { 'line-join': 'round', 'line-cap': 'round' },
			paint: { 'line-color': '#3b6fd6', 'line-width': 3, 'line-opacity': 0.85 },
		});

		const bounds = new maplibre.LngLatBounds();
		for (const c of coords) bounds.extend(c);
		m.fitBounds(bounds, { padding: 48, maxZoom: 15, duration: 0 });
	}

	onDestroy(() => {
		map?.remove();
		map = null;
	});
</script>

<div class="timeline-map" bind:this={container}></div>

<style>
	.timeline-map {
		position: absolute;
		inset: 0;
	}
</style>
