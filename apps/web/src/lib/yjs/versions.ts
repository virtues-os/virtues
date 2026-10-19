/**
 * A page's history, kept by the server: `cutServerVersion` asks it to keep
 * the page as it stands, and `restorePageVersion` asks it to put one back.
 * The browser sends no document either way, so nothing it holds can enter
 * the history unchecked.
 */

import {
	request,
	listPageVersions,
	restorePageVersionRequest,
	type PageFormat,
	type RestoredVersion,
} from '$lib/api/client';

/**
 * Page version metadata
 */
export interface PageVersion {
	id: string;
	page_id: string;
	version_number: number;
	content_preview: string;
	created_at: string;
	created_by: 'user' | 'ai' | 'auto';
	description?: string;
	/** What the version holds; absent from a server older than block pages. */
	format?: PageFormat;
}

/**
 * Ask the server to keep the page as it stands now, either format. It reads
 * its own copy of the page, which every editor's edits reach first.
 */
export async function cutServerVersion(
	pageId: string,
	description?: string,
	createdBy: 'user' | 'ai' | 'auto' = 'user',
	options?: { keepalive?: boolean },
): Promise<PageVersion | null> {
	try {
		// The typed `request`, so the on-unload save can carry `keepalive`.
		return await request<PageVersion>(`/pages/${encodeURIComponent(pageId)}/versions`, {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ description, created_by: createdBy }),
			keepalive: options?.keepalive,
		});
	} catch (err) {
		console.error('Failed to save page version:', err);
		return null;
	}
}

/**
 * Put a block page back as `versionId` left it. Throws with the server's
 * reason when it refuses.
 */
export function restorePageVersion(pageId: string, versionId: string): Promise<RestoredVersion> {
	return restorePageVersionRequest(pageId, versionId);
}

/**
 * List versions for a page
 */
export async function listVersions(pageId: string, limit = 20): Promise<PageVersion[]> {
	try {
		const data = await listPageVersions<{ versions?: PageVersion[] }>(pageId, limit);
		return data.versions || [];
	} catch (err) {
		console.error('Failed to list page versions:', err);
		return [];
	}
}
