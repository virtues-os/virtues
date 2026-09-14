/**
 * Muted places — the phone's copy of the box's `wiki_places` rows with
 * `is_audio_muted`.
 *
 * The row on the box is the authority: the desktop flips it from a place's
 * wiki page, the phone flips it from the device screen, and either way the
 * phone's native audio plugin only ever holds a CACHE of the muted subset so
 * its gate can run with no network. This module is the copy step: read the
 * rows, hand them to the plugin when they differ from what it holds.
 */
import { invoke } from "@tauri-apps/api/core";
import { updatePlace } from "$lib/wiki/api";

/** What the native plugin caches (camelCase — see the plugin's models.rs). */
export interface MutedPlace {
	id: string;
	name: string;
	lat: number;
	lon: number;
	radiusM: number;
}

/** A row of `GET /api/entities/places` (entities.rs `Place`). */
interface PlaceRow {
	id: string;
	name: string;
	latitude: number | null;
	longitude: number | null;
	radius_m: number | null;
	is_audio_muted: boolean;
}

/** The box's muted places, in the plugin's shape; null when the box could not be reached. */
export async function fetchMutedPlaces(fetchFn: typeof fetch = fetch): Promise<MutedPlace[] | null> {
	try {
		const res = await fetchFn("/api/entities/places");
		if (!res.ok) return null;
		const rows = (await res.json()) as PlaceRow[];
		return rows
			.filter((p) => p.is_audio_muted && p.latitude != null && p.longitude != null)
			.map((p) => ({
				id: p.id,
				name: p.name,
				lat: p.latitude as number,
				lon: p.longitude as number,
				radiusM: p.radius_m ?? 100,
			}));
	} catch {
		return null;
	}
}

function samePlaces(a: MutedPlace[], b: MutedPlace[]): boolean {
	if (a.length !== b.length) return false;
	const key = (p: MutedPlace) => `${p.id}|${p.name}|${p.lat}|${p.lon}|${p.radiusM}`;
	const bs = new Set(b.map(key));
	return a.every((p) => bs.has(key(p)));
}

/**
 * Copy the box's muted places into the native plugin if they differ from
 * what it reports holding. Resolves the plugin's fresh status when it pushed,
 * null when nothing changed or the box was unreachable (the cache stands).
 */
export async function syncMutedPlaces<T>(current: MutedPlace[] | undefined): Promise<T | null> {
	const fresh = await fetchMutedPlaces();
	if (!fresh) return null;
	if (current && samePlaces(current, fresh)) return null;
	try {
		return await invoke<T>("plugin:audio|set_places", { places: fresh });
	} catch {
		// An older native build without the command: nothing to copy into.
		return null;
	}
}

/** Flip one place's flag on the box. Name it too when `name` is given (the unnamed-cluster door). */
export async function setPlaceMuted(id: string, muted: boolean, name?: string): Promise<boolean> {
	const data: { is_audio_muted: boolean; name?: string } = { is_audio_muted: muted };
	if (name) data.name = name;
	return (await updatePlace(id, data)) != null;
}

/** "Mute here": a new place at the given fix, muted from birth. */
export async function createMutedPlace(
	label: string,
	latitude: number,
	longitude: number,
	formattedAddress = "",
	fetchFn: typeof fetch = fetch,
): Promise<boolean> {
	try {
		const res = await fetchFn("/api/entities/places", {
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({
				label,
				formatted_address: formattedAddress,
				latitude,
				longitude,
				is_audio_muted: true,
			}),
		});
		return res.ok;
	} catch {
		return false;
	}
}

/** A Google Places suggestion, proxied by the box (a place you have never been). */
export interface PlaceSuggestion {
	place_id: string;
	description: string;
	main_text: string;
	secondary_text: string;
}

export async function suggestPlaces(
	query: string,
	fetchFn: typeof fetch = fetch,
): Promise<PlaceSuggestion[]> {
	try {
		const res = await fetchFn(`/api/places/autocomplete?query=${encodeURIComponent(query)}`);
		if (!res.ok) return [];
		const body = (await res.json()) as { predictions?: PlaceSuggestion[] };
		return body.predictions ?? [];
	} catch {
		return [];
	}
}

export interface PlaceDetails {
	place_id: string;
	formatted_address: string;
	latitude: number;
	longitude: number;
}

export async function placeDetails(
	placeId: string,
	fetchFn: typeof fetch = fetch,
): Promise<PlaceDetails | null> {
	try {
		const res = await fetchFn(`/api/places/details?place_id=${encodeURIComponent(placeId)}`);
		if (!res.ok) return null;
		return (await res.json()) as PlaceDetails;
	} catch {
		return null;
	}
}
