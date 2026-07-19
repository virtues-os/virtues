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
		/** Optional cursor time (epoch ms) — renders a moving dot interpolated
		 * along the track at this timestamp. Set null to hide. */
		hoverTimeMs?: number | null;
		/** When false, the map is a static display — no drag/zoom/controls — so
		 * clicks pass through to an enclosing card. */
		interactive?: boolean;
		/** Center zoom used for a single stop (fitBounds would zoom to max). */
		zoom?: number;
	}

	let {
		track = [],
		stops = [],
		height = 260,
		hoverTimeMs = null,
		interactive = true,
		zoom = 13,
	}: Props = $props();

	let container: HTMLDivElement | null = null;
	let map: any = null;
	let L: any = null;
	let layer: any = null;
	// Persistent hover marker that gets re-positioned (not re-created) on
	// every hoverTimeMs change. Lives outside `layer` so it survives renders.
	let hoverMarker: any = null;

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

		// If we only have stops (no track), fit to stops bounds — but a single
		// stop has zero-size bounds (fitBounds → max zoom), so center it instead.
		if (!hasTrack && hasStops) {
			try {
				if (stops.length === 1) {
					map.setView([stops[0].lat, stops[0].lng], zoom);
				} else {
					const bounds = L.latLngBounds(stops.map((p) => [p.lat, p.lng]));
					map.fitBounds(bounds, { padding: [16, 16] });
				}
			} catch {
				// ignore
			}
		}

		// Re-add the hover marker (it lives outside `layer` so a fresh
		// render() doesn't strand it). null reset cleans up.
		if (hoverMarker) {
			try {
				hoverMarker.remove();
			} catch {
				// ignore
			}
			hoverMarker = null;
		}
	}

	/// Find the lat/lng on the track corresponding to a target timestamp.
	/// Linear interpolation between the two nearest points. Returns null if
	/// the timestamp falls outside the track's time range or the track is
	/// too sparse.
	function interpolateTrack(
		timeMs: number,
	): [number, number] | null {
		if (track.length < 2) return null;
		// Track is sorted by timeMs ascending (we sorted upstream).
		const first = track[0];
		const last = track[track.length - 1];
		if (
			first.timeMs == null ||
			last.timeMs == null ||
			timeMs < first.timeMs ||
			timeMs > last.timeMs
		) {
			return null;
		}
		// Binary search for the bracket
		let lo = 0;
		let hi = track.length - 1;
		while (lo < hi - 1) {
			const mid = (lo + hi) >> 1;
			const midT = track[mid].timeMs!;
			if (midT <= timeMs) lo = mid;
			else hi = mid;
		}
		const a = track[lo];
		const b = track[hi];
		const at = a.timeMs!;
		const bt = b.timeMs!;
		if (bt === at) return [a.lat, a.lng];
		const t = (timeMs - at) / (bt - at);
		return [a.lat + (b.lat - a.lat) * t, a.lng + (b.lng - a.lng) * t];
	}

	function updateHoverMarker() {
		if (!map || !L) return;
		// Hide
		if (hoverTimeMs == null) {
			if (hoverMarker) {
				try {
					hoverMarker.remove();
				} catch {
					// ignore
				}
				hoverMarker = null;
			}
			return;
		}
		const pos = interpolateTrack(hoverTimeMs);
		if (!pos) {
			// Outside track range — hide
			if (hoverMarker) {
				try {
					hoverMarker.remove();
				} catch {
					// ignore
				}
				hoverMarker = null;
			}
			return;
		}
		if (!hoverMarker) {
			hoverMarker = L.circleMarker(pos, {
				radius: 5,
				weight: 2,
				color: "var(--color-primary)",
				fillColor: "var(--color-primary)",
				fillOpacity: 1,
			}).addTo(map);
		} else {
			hoverMarker.setLatLng(pos);
		}
	}

	onMount(async () => {
		if (!browser || !container) return;

		const leaflet = await import("leaflet");
		L = (leaflet as any).default ?? leaflet;

		map = L.map(container, {
			zoomControl: interactive,
			attributionControl: false,
			scrollWheelZoom: false,
			dragging: interactive,
			doubleClickZoom: interactive,
			boxZoom: interactive,
			keyboard: interactive,
			touchZoom: interactive,
			tap: interactive,
		});

		// Tiles are served + cached by the box itself (see docs/map-atlas-plan.md):
		// the browser never talks to a third-party tile provider, and cached areas
		// keep working offline. Upstream (CartoDB Positron) attribution is preserved.
		L.tileLayer("/api/map/tiles/light/{z}/{x}/{y}", {
			maxZoom: 19,
			attribution: "&copy; OpenStreetMap contributors &copy; CARTO",
			// Blank tile when the box is offline / upstream fails → grey gaps, not broken images.
			errorTileUrl: "data:image/gif;base64,R0lGODlhAQABAAAAACwAAAAAAQABAAA=",
		}).addTo(map);

		render();
	});

	$effect(() => {
		if (!browser) return;
		// Re-render when track/stops change
		// (deps: track, stops — referenced inside render())
		track;
		stops;
		render();
	});

	$effect(() => {
		if (!browser) return;
		// React only to hoverTimeMs — don't re-run the full render() on every
		// cursor move (that would re-fit bounds and clear stops).
		hoverTimeMs;
		updateHoverMarker();
	});

	onDestroy(() => {
		if (hoverMarker) {
			try {
				hoverMarker.remove();
			} catch {
				// ignore
			}
			hoverMarker = null;
		}
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
