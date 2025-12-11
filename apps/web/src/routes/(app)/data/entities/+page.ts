import type { PageLoad } from './$types';

interface PlaceFromApi {
	id: string;
	canonical_name: string;
	category: string | null;
	latitude: number | null;
	longitude: number | null;
	metadata: {
		formatted_address?: string;
		google_place_id?: string;
	} | null;
}

export const load: PageLoad = async ({ fetch }) => {
	const [placesRes, profileRes] = await Promise.all([
		fetch('/api/entities/places'),
		fetch('/api/profile')
	]);

	const places: PlaceFromApi[] = placesRes.ok ? await placesRes.json() : [];
	const profile = profileRes.ok ? await profileRes.json() : null;

	// Transform to frontend format
	const locations = places.map((p) => ({
		id: p.id,
		label: p.canonical_name,
		formatted_address: p.metadata?.formatted_address || '',
		latitude: p.latitude || 0,
		longitude: p.longitude || 0,
		google_place_id: p.metadata?.google_place_id
	}));

	return {
		locations,
		homePlaceId: profile?.home_place_id || null
	};
};
