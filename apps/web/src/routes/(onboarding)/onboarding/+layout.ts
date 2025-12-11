import type { LayoutLoad } from './$types';

export const load: LayoutLoad = async ({ fetch }) => {
	try {
		const [profileRes, assistantRes, placesRes] = await Promise.all([
			fetch('/api/profile'),
			fetch('/api/assistant-profile'),
			fetch('/api/entities/places')
		]);

		const places = placesRes.ok ? await placesRes.json() : [];
		const profile = profileRes.ok ? await profileRes.json() : null;

		// Transform places to the format expected by the places page
		// The API returns: { id, canonical_name, latitude, longitude, metadata: { formatted_address, ... } }
		// The page expects: { label, address, latitude, longitude, google_place_id }
		const locations = places.map((p: { id: string; canonical_name: string; latitude: number; longitude: number; metadata?: { formatted_address?: string; google_place_id?: string } }) => ({
			id: p.id,
			label: p.canonical_name,
			formatted_address: p.metadata?.formatted_address || '',
			latitude: p.latitude,
			longitude: p.longitude,
			google_place_id: p.metadata?.google_place_id
		}));

		return {
			profile,
			assistantProfile: assistantRes.ok ? await assistantRes.json() : null,
			locations,
			homePlaceId: profile?.home_place_id || null
		};
	} catch {
		return { profile: null, assistantProfile: null, locations: [], homePlaceId: null };
	}
};
