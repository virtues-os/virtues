<script lang="ts">
	import { browser } from "$app/environment";
	import { onDestroy, onMount } from "svelte";
	import "leaflet/dist/leaflet.css";

	export type MapPoint = {
		lat: number;
		lng: number;
		label?: string;
		timeMs?: number;
	};

	interface Props {
		/** High-frequency track (polyline) */
		track?: MapPoint[];
		/** Lower-frequency stops (markers), e.g. location chunks */
		stops?: MapPoint[];
		height?: number;
	}

	let { track = [], stops = [], height = 260 }: Props = $props();

	let container: HTMLDivElement | null = null;
	let map: any = null;
	let L: any = null;
	let layer: any = null;

	function clearLayer() {
		if (layer) {
			try {
				layer.remove();
			} catch {
				// ignore
			}
			layer = null;
		}
	}

	function render() {
		if (!map || !L) return;

		clearLayer();
		layer = L.layerGroup().addTo(map);

		const hasTrack = track.length >= 2;
		const hasStops = stops.length >= 1;

		// Default view: Rome-ish, so empty state isn't weird.
		if (!hasTrack && !hasStops) {
			map.setView([41.9037, 12.4793], 14);
			return;
		}

		const latlngs: [number, number][] = [];

		if (hasTrack) {
			for (const p of track) latlngs.push([p.lat, p.lng]);
			const poly = L.polyline(latlngs, {
				color: "var(--color-primary)",
				weight: 2,
				opacity: 0.9,
			}).addTo(layer);

			// Fit to polyline bounds
			try {
				map.fitBounds(poly.getBounds(), { padding: [16, 16] });
			} catch {
				// ignore
			}
		}

		// Stops as circle markers (avoid Leaflet default marker assets)
		for (let i = 0; i < stops.length; i++) {
			const p = stops[i];
			const isFirst = i === 0;
			const isLast = i === stops.length - 1;

			const marker = L.circleMarker([p.lat, p.lng], {
				radius: isFirst || isLast ? 3 : 2,
				weight: 1.5,
				color: isFirst
					? "var(--color-success)"
					: isLast
						? "var(--color-error)"
						: "var(--color-border-strong)",
				fillColor: "var(--color-background)",
				fillOpacity: 1,
			}).addTo(layer);

			if (p.label)
				marker.bindTooltip(p.label, { direction: "top", opacity: 0.9 });
		}

		// If we only have stops (no track), fit to stops bounds
		if (!hasTrack && hasStops) {
			try {
				const bounds = L.latLngBounds(stops.map((p) => [p.lat, p.lng]));
				map.fitBounds(bounds, { padding: [16, 16] });
			} catch {
				// ignore
			}
		}
	}

	onMount(async () => {
		if (!browser || !container) return;

		const leaflet = await import("leaflet");
		L = (leaflet as any).default ?? leaflet;

		map = L.map(container, {
			zoomControl: true,
			attributionControl: false,
			scrollWheelZoom: false,
		});

		// Use CartoDB Positron (light, muted) tiles for a grayscale/muted look
		L.tileLayer(
			"https://{s}.basemaps.cartocdn.com/light_all/{z}/{x}/{y}{r}.png",
			{
				maxZoom: 19,
				attribution: "&copy; OpenStreetMap contributors &copy; CARTO",
				subdomains: "abcd",
			},
		).addTo(map);

		render();
	});

	$effect(() => {
		if (!browser) return;
		// Re-render when props change
		render();
	});

	onDestroy(() => {
		clearLayer();
		if (map) {
			try {
				map.remove();
			} catch {
				// ignore
			}
		}
		map = null;
		L = null;
	});
</script>

<div class="movement-map" style="height: {height}px;">
	<div class="map-inner" bind:this={container}></div>
</div>

<style>
	.movement-map {
		width: 100%;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		overflow: hidden;
		background: var(--color-surface);
	}

	.map-inner {
		width: 100%;
		height: 100%;
	}

	/* Leaflet theme tweaks */
	.movement-map :global(.leaflet-container) {
		font-family: var(
			--font-sans,
			ui-sans-serif,
			system-ui,
			-apple-system,
			sans-serif
		);
		background: var(--color-surface);
	}

	/* Make map tiles more grayscale/muted, but preserve light blue water */
	/* Preserve blue tones (water) by using a selective filter */
	/* This makes everything muted/grayscale while keeping water light blue */
	/* .movement-map :global(.leaflet-tile-container img) {
		filter: grayscale(0.7) brightness(1.05) contrast(0.95);
	} */

	.movement-map :global(.leaflet-control-zoom) {
		border: 1px solid var(--color-border);
		box-shadow: none;
	}

	.movement-map :global(.leaflet-control-zoom a) {
		background: var(--color-surface-elevated);
		color: var(--color-foreground);
		border-bottom: 1px solid var(--color-border);
		width: 24px;
		height: 24px;
		line-height: 22px;
		font-size: 16px;
	}

	.movement-map :global(.leaflet-tooltip) {
		background: var(--color-surface-elevated);
		border: 1px solid var(--color-border);
		color: var(--color-foreground);
		box-shadow: none;
	}
</style>
