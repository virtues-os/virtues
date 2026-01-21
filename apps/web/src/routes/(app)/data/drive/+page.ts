import type { PageLoad } from './$types';
import type { DriveFile, DriveUsage } from '$lib/api/client';

export const load: PageLoad = async ({ fetch, url }) => {
	const path = url.searchParams.get('path') || '';

	try {
		// Fetch files and usage in parallel
		const [filesResponse, usageResponse] = await Promise.all([
			fetch(`/api/drive/files${path ? `?path=${encodeURIComponent(path)}` : ''}`),
			fetch('/api/drive/usage')
		]);

		if (!filesResponse.ok) {
			const errorData = await filesResponse.json().catch(() => ({ error: 'Failed to load files' }));
			return {
				files: [] as DriveFile[],
				usage: null as DriveUsage | null,
				currentPath: path,
				error: errorData.error || 'Failed to load files'
			};
		}

		const files: DriveFile[] = await filesResponse.json();
		const usage: DriveUsage | null = usageResponse.ok ? await usageResponse.json() : null;

		return {
			files,
			usage,
			currentPath: path,
			error: null as string | null
		};
	} catch (err) {
		console.error('Failed to load drive data:', err);
		return {
			files: [] as DriveFile[],
			usage: null as DriveUsage | null,
			currentPath: path,
			error: 'Failed to load drive data'
		};
	}
};
