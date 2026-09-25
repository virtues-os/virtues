/**
 * The basemap for a Leaflet map, drawn from the box's own map files.
 *
 * The box holds Protomaps archives in three tiers (virtues-core/src/maps,
 * agents/plan/offline-maps-plan.md): a world overview, the regions you have
 * visited at street level, and home at full detail. Every tile, font and icon
 * comes from the box, so no tile provider ever learns which streets anyone
 * looks at, and the maps work offline.
 *
 * The tiles are vector, so the browser draws them: MapLibre GL, bound into
 * Leaflet as one layer, so the tracks, pins and tooltips the maps already draw
 * keep working unchanged. The three tiers are three MapLibre sources stacked
 * world, visited, home, so each keeps its own max zoom and MapLibre overzooms
 * it. The upper two drop their background layer: otherwise it paints over the
 * tier underneath everywhere outside their squares.
 *
 * The data credit rides on the world source and the binding copies it into
 * Leaflet's attribution control. Keep `attributionControl: true` on any map
 * that uses this.
 */
import type * as Leaflet from "leaflet";
import type { LayerSpecification, StyleSpecification } from "maplibre-gl";
import { backendUrl } from "$lib/config/backend";

export type AtlasStyle = "light" | "dark";

/** What `/api/map/sources` reports the box holds. */
type Sources = {
	world: boolean;
	visited: number[][];
	home: number[][];
	attribution: string;
};

/**
 * Absolute URL for a box path. MapLibre fetches from a worker, which resolves
 * nothing against the page and never sees the mobile fetch shim. Not
 * `new URL()`: it would percent-encode the `{z}`/`{fontstack}` placeholders.
 */
function absolute(path: string): string {
	const url = backendUrl(path);
	return url.startsWith("/") ? window.location.origin + url : url;
}

/** One box of bounds around every square of a tier. */
function union(squares: number[][]): [number, number, number, number] {
	return [
		Math.min(...squares.map((b) => b[0])),
		Math.min(...squares.map((b) => b[1])),
		Math.max(...squares.map((b) => b[2])),
		Math.max(...squares.map((b) => b[3])),
	];
}

let sources: Promise<Sources> | null = null;

function loadSources(): Promise<Sources> {
	sources ??= fetch("/api/map/sources").then((r) => {
		if (!r.ok) throw new Error(`map sources: ${r.status}`);
		return r.json() as Promise<Sources>;
	});
	// A failure (the box restarting) must not stick for the session.
	sources.catch(() => (sources = null));
	return sources;
}

async function buildStyle(style: AtlasStyle, have: Sources): Promise<StyleSpecification | null> {
	if (!have.world && !have.visited.length && !have.home.length) return null;
	const { layers, namedFlavor } = await import("@protomaps/basemaps");
	const flavor = namedFlavor(style);

	const tiers = [
		// World first: the bottom of the stack, and the only background.
		have.world && { id: "world", maxzoom: 7, bounds: undefined, keepBackground: true },
		have.visited.length && { id: "visited", maxzoom: 13, bounds: union(have.visited), keepBackground: false },
		have.home.length && { id: "home", maxzoom: 15, bounds: union(have.home), keepBackground: false },
	].filter(Boolean) as { id: string; maxzoom: number; bounds?: number[]; keepBackground: boolean }[];

	const shapes: LayerSpecification[] = [];
	const labels: LayerSpecification[] = [];
	for (const t of tiers) {
		for (const l of layers(t.id, flavor, { lang: "en" }) as LayerSpecification[]) {
			if (l.type === "background" && !t.keepBackground) continue;
			// Ids must be unique across the stacked copies of one layer set.
			const layer = { ...l, id: `${t.id}:${l.id}` } as LayerSpecification;
			// Every label above every shape, and within labels the most
			// detailed tier last, so it wins placement where tiers overlap.
			(l.type === "symbol" ? labels : shapes).push(layer);
		}
	}

	return {
		version: 8,
		glyphs: absolute("/api/map/fonts/{fontstack}/{range}.pbf"),
		sprite: absolute(`/api/map/sprite/${style}`),
		sources: Object.fromEntries(
			tiers.map((t, i) => [
				t.id,
				{
					type: "vector",
					tiles: [absolute(`/api/map/vt/${t.id}/{z}/{x}/{y}`)],
					minzoom: 0,
					maxzoom: t.maxzoom,
					...(t.bounds ? { bounds: t.bounds } : {}),
					// One credit for the whole map, not one per tier.
					...(i === 0 ? { attribution: have.attribution } : {}),
				},
			]),
		),
		layers: [...shapes, ...labels],
	} as StyleSpecification;
}

let engine: Promise<typeof import("@maplibre/maplibre-gl-leaflet")> | null = null;

function loadEngine() {
	engine ??= (async () => {
		const [maplibre, workerUrl] = await Promise.all([
			import("maplibre-gl"),
			// MapLibre finds its worker next to its own module by default, which
			// is not where the bundler puts either of them.
			import("maplibre-gl/dist/maplibre-gl-worker.mjs?worker&url").then((m) => m.default),
			import("maplibre-gl/dist/maplibre-gl.css"),
		]);
		maplibre.setWorkerUrl(workerUrl);
		return import("@maplibre/maplibre-gl-leaflet");
	})();
	engine.catch(() => (engine = null));
	return engine;
}

/**
 * The basemap layer for `style`, or null when the box holds no map files yet
 * (or cannot be reached): the container's own background shows and every
 * overlay still draws. The caller adds and removes the layer, so a map torn
 * down while this was loading is not resurrected.
 */
export async function atlasLayer(style: AtlasStyle): Promise<Leaflet.Layer | null> {
	try {
		const spec = await buildStyle(style, await loadSources());
		if (!spec) return null;
		const { maplibreGL } = await loadEngine();
		return maplibreGL({ style: spec, interactive: false });
	} catch (e) {
		console.warn("atlas: basemap unavailable", e);
		return null;
	}
}
