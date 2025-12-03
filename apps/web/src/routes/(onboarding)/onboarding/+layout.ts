import type { LayoutLoad } from './$types';

export const load: LayoutLoad = async ({ fetch }) => {
	try {
		const [profileRes, assistantRes, locationsRes] = await Promise.all([
			fetch('/api/profile'),
			fetch('/api/assistant-profile'),
			fetch('/api/profile/locations')
		]);

		const locationsData = locationsRes.ok ? await locationsRes.json() : null;

		return {
			profile: profileRes.ok ? await profileRes.json() : null,
			assistantProfile: assistantRes.ok ? await assistantRes.json() : null,
			locations: locationsData?.locations || []
		};
	} catch {
		return { profile: null, assistantProfile: null, locations: [] };
	}
};
