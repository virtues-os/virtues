import type { PageLoad } from './$types';
import type { StreamObject } from '$lib/api/client';

export const load: PageLoad = async ({ fetch }) => {
	try {
		const response = await fetch('/api/storage/objects?limit=10');

		if (!response.ok) {
			console.error('Failed to fetch storage objects:', response.statusText);
			return { objects: [], error: 'Failed to load storage objects' };
		}

		const objects: StreamObject[] = await response.json();
		return { objects };
	} catch (error) {
		console.error('Failed to load storage objects:', error);
		return { objects: [], error: 'Failed to load storage objects' };
	}
};
