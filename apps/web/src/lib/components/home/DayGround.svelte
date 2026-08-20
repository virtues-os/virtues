<!--
	DayGround.svelte — the day's ground track, on the map it was walked over.

	The same GPS fixes the deck's `move` lane measures, drawn in space instead
	of time. It exists to be scrubbed: pointing at an hour on the deck puts a
	dot here, so "where was I at 2pm" is answered by moving the mouse rather
	than by reading two charts and doing the join in your head.

	Tiles come from the box's own atlas (`/api/map/tiles`, see
	docs/map-atlas-plan.md): cached on the box after first fetch, so the
	browser never hands the day's coordinates to a third-party tile server and
	areas you actually live in keep working offline. The panel is display-only
	— no drag, no zoom — because it answers the deck's scrub, not the mouse.
-->
<script lang="ts">
	import { onDestroy, onMount } from "svelte";
	import "leaflet/dist/leaflet.css";
	import { backendUrl } from "$lib/config/backend";
	import type { TimelineDayPoint } from "$lib/wiki/api";

	interface Props {
		points: TimelineDayPoint[];
		/** Instant to mark, or null for the live head. */
		scrubMs?: number | null;
		nowMs: number;
	}
	let { points, scrubMs = null, nowMs }: Props = $props();

	let container = $state<HTMLDivElement | undefined>(undefined);
	let map: any = null;
	let L: any = null;
	let trackLayer: any = null;
	let markDot: any = null;
	let tiles: any = null;

	/** The fix nearest an instant — what the dot marks. */
	const marked = $derived.by(() => {
		if (!points.length) return null;
		const t = scrubMs ?? nowMs;
		let best = points[0],
			gap = Infinity;
		for (const p of points) {
			const g = Math.abs(Date.parse(p.timestamp) - t);
			if (g < gap) {
				gap = g;
				best = p;
			}
		}
		// Beyond half an hour there is no fix worth claiming as "here".
		return gap > 30 * 60_000 ? null : best;
	});

	/** The atlas caches both Carto styles; follow the app's own scheme. */
	function tileStyle(): "light" | "dark" {
		const flag = getComputedStyle(document.documentElement).getPropertyValue("--identity-dark").trim();
		return flag === "1" ? "dark" : "light";
	}

	// `backendUrl`, not a bare path: tiles load from <img src>, which the
	// mobile shell's fetch proxy never sees.
	function setTiles() {
		if (!map || !L) return;
		tiles?.remove();
		tiles = L.tileLayer(backendUrl(`/api/map/tiles/${tileStyle()}/{z}/{x}/{y}`), {
			maxZoom: 19,
			// Blank tile when the box is offline / upstream fails — grey gaps,
			// not broken images.
			errorTileUrl: "data:image/gif;base64,R0lGODlhAQABAAAAACwAAAAAAQABAAA=",
		}).addTo(map);
	}

	function renderTrack() {
		if (!map || !L || points.length < 2) return;
		trackLayer?.remove();
		trackLayer = L.polyline(
			points.map((p) => [p.latitude, p.longitude]),
			{ color: "var(--color-foreground)", opacity: 0.45, weight: 1.5, interactive: false }
		).addTo(map);
		try {
			// Capped: a day spent at home is a tiny bbox, and fitBounds would
			// otherwise dive to rooftop zoom.
			map.fitBounds(trackLayer.getBounds(), { padding: [10, 10], maxZoom: 16 });
		} catch {
			// ignore
		}
	}

	function renderMark() {
		if (!map || !L) return;
		const mk = marked;
		if (!mk) {
			markDot?.remove();
			markDot = null;
			return;
		}
		const pos: [number, number] = [mk.latitude, mk.longitude];
		if (!markDot) {
			markDot = L.circleMarker(pos, {
				radius: 4,
				weight: 2,
				color: "var(--color-background)",
				fillColor: "var(--color-primary)",
				fillOpacity: 1,
				interactive: false,
			}).addTo(map);
		} else {
			markDot.setLatLng(pos);
		}
	}

	function onTheme() {
		setTiles();
	}

	onMount(async () => {
		if (!container) return;
		const leaflet = await import("leaflet");
		L = (leaflet as any).default ?? leaflet;
		map = L.map(container, {
			zoomControl: false,
			attributionControl: false,
			scrollWheelZoom: false,
			dragging: false,
			doubleClickZoom: false,
			boxZoom: false,
			keyboard: false,
			touchZoom: false,
			tap: false,
		});
		setTiles();
		renderTrack();
		renderMark();
		window.addEventListener("themechange", onTheme);
	});

	$effect(() => {
		points;
		renderTrack();
	});

	$effect(() => {
		marked;
		renderMark();
	});

	onDestroy(() => {
		window.removeEventListener("themechange", onTheme);
		markDot?.remove();
		trackLayer?.remove();
		try {
			map?.remove();
		} catch {
			// ignore
		}
		map = null;
		L = null;
	});
</script>

<div class="map" bind:this={container} aria-hidden="true"></div>

<style>
	.map {
		width: 100%;
		height: 100%;
		border: 1px solid var(--color-border);
		border-radius: 8px;
		overflow: hidden;
		background: var(--color-surface);
	}
	.map :global(.leaflet-container) {
		background: var(--color-surface);
		font-family: var(--font-sans);
		cursor: default;
	}
</style>
