import type { PageServerLoad } from './$types';
import { env } from '$env/dynamic/private';
import type { StreamObject } from '$lib/api/client';

export const load: PageServerLoad = async ({ fetch }) => {
	try {
		const apiUrl = env.CORE_API_URL || 'http://localhost:8000';
		const response = await fetch(`${apiUrl}/api/storage/objects?limit=10`);

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
