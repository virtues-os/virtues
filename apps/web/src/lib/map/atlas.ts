/**
 * The basemap for a Leaflet map.
 *
 * There is none yet, on purpose. The box is getting its own map files
 * (Protomaps extracts it downloads and serves itself; see
 * agents/plan/offline-maps-plan.md), and until they land no map in the app
 * fetches tiles from anyone. The maps still draw their own background, tracks,
 * pins and tooltips.
 *
 * CARTO went key-only on 2026-09-23. The OpenFreeMap proxy that briefly
 * replaced it told OpenFreeMap which tiles each person looked at, so it was
 * removed rather than shipped. Its MapLibre-in-Leaflet wiring, including the
 * `?worker&url` import the bundler needs for MapLibre's worker, is in commit
 * 2d49cf6b for the Protomaps layer to reuse.
 */
import type * as Leaflet from "leaflet";

export type AtlasStyle = "light" | "dark";

/**
 * The basemap layer for `style`, or null when the box has none. Callers add
 * it themselves, so a map torn down while this was loading is not resurrected.
 */
export async function atlasLayer(_style: AtlasStyle): Promise<Leaflet.Layer | null> {
	return null;
}
