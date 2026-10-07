<!--
	Places from a query's lat, lon and label columns, as dots on the box's own
	atlas (lib/map/atlas): the tiles come from the box, so no tile provider
	learns where anyone has been. Scroll passes through to the chat; the map
	moves with two fingers, or with the modifier key on a wheel.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import type { Map as MlMap, StyleSpecification } from "maplibre-gl";
	import { atlasStyle } from "$lib/map/atlas";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { rowRef, type Row } from "./show";

	interface Props {
		rows: Row[];
	}
	let { rows }: Props = $props();

	let container: HTMLDivElement | undefined = $state();
	let label = $state<string | null>(null);

	const points = $derived(
		rows.map((r, i) => ({
			type: "Feature" as const,
			id: i,
			properties: { label: typeof r.label === "string" ? r.label : "", ref: rowRef(r) ?? "" },
			geometry: { type: "Point" as const, coordinates: [r.lon as number, r.lat as number] },
		})),
	);

	const isDark = () =>
		getComputedStyle(document.documentElement).getPropertyValue("--identity-dark").trim() === "1";
	const token = (name: string) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();

	/** The page's plain ground, when the box holds no map files or is restarting. */
	const bare = (): StyleSpecification => ({
		version: 8,
		sources: {},
		layers: [{ id: "ground", type: "background", paint: { "background-color": token("--color-surface") } }],
	});

	onMount(() => {
		let map: MlMap | undefined;
		let cancelled = false;
		let resizer: ResizeObserver | undefined;

		async function build() {
			const [ml, workerUrl] = await Promise.all([
				import("maplibre-gl"),
				import("maplibre-gl/dist/maplibre-gl-worker.mjs?worker&url").then((m) => m.default),
				import("maplibre-gl/dist/maplibre-gl.css"),
			]);
			ml.setWorkerUrl(workerUrl as string);
			let style: StyleSpecification | null = null;
			try {
				style = await atlasStyle(isDark() ? "dark" : "light");
			} catch {
				style = null;
			}
			if (cancelled || !container) return;
			map?.remove();
			const m = new ml.Map({
				container,
				style: style ?? bare(),
				center: [0, 20],
				zoom: 1,
				attributionControl: { compact: true },
				cooperativeGestures: true,
				dragRotate: false,
				pitchWithRotate: false,
			});
			map = m;
			m.touchZoomRotate.disableRotation();
			resizer?.disconnect();
			resizer = new ResizeObserver(() => m.resize());
			resizer.observe(container);
			m.on("load", () => {
				if (cancelled) return;
				m.addSource("places", { type: "geojson", data: { type: "FeatureCollection", features: points } });
				m.addLayer({
					id: "places",
					type: "circle",
					source: "places",
					paint: {
						"circle-radius": ["case", ["boolean", ["feature-state", "hover"], false], 7, 5],
						"circle-color": token("--color-primary"),
						"circle-stroke-color": token("--color-background"),
						"circle-stroke-width": 2,
					},
				});
				const coords = points.map((p) => p.geometry.coordinates as [number, number]);
				if (coords.length === 1) {
					m.jumpTo({ center: coords[0], zoom: 13 });
				} else {
					const b = new ml.LngLatBounds(coords[0], coords[0]);
					for (const c of coords) b.extend(c);
					m.fitBounds(b, { padding: 32, maxZoom: 15, duration: 0 });
				}
				let hovered: number | null = null;
				const setHover = (id: number | null) => {
					if (hovered !== null) m.setFeatureState({ source: "places", id: hovered }, { hover: false });
					hovered = id;
					if (id !== null) m.setFeatureState({ source: "places", id }, { hover: true });
				};
				m.on("mousemove", "places", (e) => {
					const f = e.features?.[0];
					if (!f) return;
					setHover(f.id as number);
					label = (f.properties?.label as string) || null;
					m.getCanvas().style.cursor = f.properties?.ref ? "pointer" : "";
				});
				m.on("mouseleave", "places", () => {
					setHover(null);
					label = null;
					m.getCanvas().style.cursor = "";
				});
				m.on("click", "places", (e) => {
					const f = e.features?.[0];
					const ref = f?.properties?.ref as string | undefined;
					if (ref) windowShellStore.openRouteBeside(ref, (f?.properties?.label as string) || "Place");
					else label = (f?.properties?.label as string) || null;
				});
			});
		}

		void build();
		const onTheme = () => void build();
		window.addEventListener("themechange", onTheme);
		return () => {
			cancelled = true;
			window.removeEventListener("themechange", onTheme);
			resizer?.disconnect();
			map?.remove();
		};
	});
</script>

<div class="readout" aria-live="polite">
	{label ?? `${rows.length} ${rows.length === 1 ? "point" : "points"}`}
</div>
<div class="map" bind:this={container} role="img" aria-label="Map of {rows.length} points"></div>

<style>
	.readout {
		min-height: 1.25rem;
		margin-bottom: 0.375rem;
		font-family: var(--font-sans);
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}
	.map {
		width: 100%;
		height: 260px;
		border-radius: 12px;
		overflow: hidden;
		background: var(--color-surface);
	}
</style>
