/**
 * The Atlas basemap for a Leaflet map.
 *
 * The box serves the whole map: a MapLibre style whose tiles, glyphs and
 * sprite all point back at `/api/map/*`, cached on the box after first fetch
 * (virtues-core/src/server/atlas.rs, agents/record/map-atlas-plan.md). The
 * browser never talks to a tile provider. The tiles are vector, so the
 * browser draws them: MapLibre GL, bound into Leaflet as one layer, so the
 * tracks, pins and tooltips the maps already draw keep working unchanged.
 *
 * Attribution rides on the style's source (the box sets it) and the binding
 * copies it into Leaflet's attribution control. Keep `attributionControl: true`
 * on any map that uses this.
 */
import type * as Leaflet from "leaflet";
import { backendUrl } from "$lib/config/backend";

export type AtlasStyle = "light" | "dark";

type Style = {
	sprite: string;
	glyphs: string;
	sources: Record<string, { tiles?: string[] }>;
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

const styles = new Map<AtlasStyle, Promise<Style>>();

function loadStyle(style: AtlasStyle): Promise<Style> {
	let pending = styles.get(style);
	if (!pending) {
		pending = fetch(`/api/map/style/${style}`)
			.then((r) => {
				if (!r.ok) throw new Error(`map style ${style}: ${r.status}`);
				return r.json() as Promise<Style>;
			})
			.then((s) => {
				s.sprite = absolute(s.sprite);
				s.glyphs = absolute(s.glyphs);
				for (const source of Object.values(s.sources)) {
					if (source.tiles) source.tiles = source.tiles.map(absolute);
				}
				return s;
			});
		// A failure (box offline on first view) must not stick for the session.
		pending.catch(() => styles.delete(style));
		styles.set(style, pending);
	}
	return pending;
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
 * Build the basemap layer for `style`. The caller adds it (and removes it),
 * so a map torn down while this was loading is not resurrected. Resolves null
 * when the box cannot produce the map — the container's own background shows
 * and every overlay still draws.
 */
export async function atlasLayer(style: AtlasStyle): Promise<Leaflet.Layer | null> {
	try {
		const [s, { maplibreGL }] = await Promise.all([loadStyle(style), loadEngine()]);
		return maplibreGL({
			// A fresh copy: MapLibre keeps and mutates the object it is given.
			style: structuredClone(s) as never,
			interactive: false,
		});
	} catch (e) {
		console.warn("atlas: basemap unavailable", e);
		return null;
	}
}
