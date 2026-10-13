/**
 * A page's history.
 *
 * - A block page's versions are kept by the server: `cutServerVersion` asks it
 *   to keep the page as it stands, and `restorePageVersion` asks it to put one
 *   back. The browser sends no document either way, so nothing it holds can
 *   enter the history unchecked.
 * - A markdown page's versions are cut and restored here, from the Y.Text this
 *   browser holds (`saveVersion`, `restoreVersion`).
 */

import * as Y from 'yjs';
import type { YjsDocument } from './document';
import {
	request,
	listPageVersions,
	getPageVersion,
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

// TODO(2026-10-07): migrate markdown versions to the server; slice 7 deletes
// the client path (`saveVersion`, `restoreVersion`).

/**
 * Save a markdown page's current state as a version, from this browser's copy.
 *
 * Uses encodeStateAsUpdate() which captures the complete document state.
 * Unlike snapshots, this is self-contained and doesn't require gc: false.
 */
export async function saveVersion(
	ydoc: Y.Doc,
	pageId: string,
	description?: string,
	createdBy: 'user' | 'ai' | 'auto' = 'user',
	options?: { keepalive?: boolean }
): Promise<PageVersion | null> {
	try {
		// Capture complete document state (self-contained, works with GC)
		const fullState = Y.encodeStateAsUpdate(ydoc);

		// Get content preview from Y.Text
		const ytext = ydoc.getText('content');
		const contentPreview = ytext.toString().slice(0, 500);

		// Encode as base64 for JSON transport
		const stateBase64 = btoa(String.fromCharCode(...fullState));

		// Uses the underlying typed `request` (not the createPageVersion wrapper)
		// so we can carry `keepalive` for the on-unload save path.
		return await request<PageVersion>(`/pages/${encodeURIComponent(pageId)}/versions`, {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({
				snapshot: stateBase64,
				content_preview: contentPreview,
				description,
				created_by: createdBy
			}),
			keepalive: options?.keepalive
		});
	} catch (err) {
		console.error('Failed to save page version:', err);
		return null;
	}
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

/**
 * Restore a markdown page to a specific version
 *
 * Creates a fresh Y.Doc, applies the stored state, then copies
 * the text content into the live document.
 */
export async function restoreVersion(
	yjsDoc: YjsDocument,
	versionId: string
): Promise<boolean> {
	try {
		const data = await getPageVersion(versionId);
		if (!data.snapshot) {
			throw new Error('Version has no snapshot data');
		}

		// Decode stored state
		const stateData = Uint8Array.from(atob(data.snapshot), (c) => c.charCodeAt(0));

		// Create fresh doc and apply stored state
		const freshDoc = new Y.Doc();
		Y.applyUpdate(freshDoc, stateData);

		const restoredContent = freshDoc.getText('content').toString();

		// Replace content in the live document.
		// Must be two separate operations (not wrapped in transact) because
		// y-codemirror.next converts a single delete+insert delta into two
		// overlapping CM changes at position 0, which CM rejects.
		const { ytext } = yjsDoc;

		if (ytext.length > 0) {
			ytext.delete(0, ytext.length);
		}
		ytext.insert(0, restoredContent);

		// Cleanup
		freshDoc.destroy();

		return true;
	} catch (err) {
		console.error('Failed to restore version:', err);
		return false;
	}
}
