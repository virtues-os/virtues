/**
 * Version History for Yjs Documents
 *
 * Provides snapshot management for page version history.
 * Uses Yjs snapshots to capture document state at specific points.
 */

import * as Y from 'yjs';
import type { YjsDocument } from './document';

/**
 * Page version metadata
 */
export interface PageVersion {
	id: string;
	page_id: string;
	version_number: number;
	content_preview: string;
	created_at: string;
	created_by: 'user' | 'ai';
	description?: string;
}

/**
 * Save a version snapshot of the current document state
 *
 * @param ydoc - The Yjs document
 * @param pageId - The page ID
 * @param description - Optional description of this version
 * @param createdBy - Who created this version ('user' or 'ai')
 */
export async function saveVersion(
	ydoc: Y.Doc,
	pageId: string,
	description?: string,
	createdBy: 'user' | 'ai' = 'user'
): Promise<PageVersion | null> {
	try {
		// Create a Yjs snapshot
		const snapshot = Y.snapshot(ydoc);
		const snapshotData = Y.encodeSnapshot(snapshot);

		// Get content preview
		const text = ydoc.getText('content');
		const contentPreview = text.toString().slice(0, 500);

		// Encode snapshot as base64 for JSON transport
		const snapshotBase64 = btoa(String.fromCharCode(...snapshotData));

		const response = await fetch(`/api/pages/${pageId}/versions`, {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({
				snapshot: snapshotBase64,
				content_preview: contentPreview,
				description,
				created_by: createdBy
			})
		});

		if (!response.ok) {
			throw new Error(`Failed to save version: ${response.status}`);
		}

		return await response.json();
	} catch (err) {
		console.error('Failed to save page version:', err);
		return null;
	}
}

/**
 * List versions for a page
 *
 * @param pageId - The page ID
 * @param limit - Maximum number of versions to return
 */
export async function listVersions(pageId: string, limit = 20): Promise<PageVersion[]> {
	try {
		const response = await fetch(`/api/pages/${pageId}/versions?limit=${limit}`);
		if (!response.ok) {
			throw new Error(`Failed to list versions: ${response.status}`);
		}
		const data = await response.json();
		return data.versions || [];
	} catch (err) {
		console.error('Failed to list page versions:', err);
		return [];
	}
}

/**
 * Restore a document to a specific version
 *
 * Note: This creates a new transaction with the restored content,
 * it doesn't revert the Yjs history.
 *
 * @param yjsDoc - The Yjs document wrapper
 * @param versionId - The version ID to restore
 */
export async function restoreVersion(
	yjsDoc: YjsDocument,
	versionId: string
): Promise<boolean> {
	try {
		const response = await fetch(`/api/pages/versions/${versionId}`);
		if (!response.ok) {
			throw new Error(`Failed to fetch version: ${response.status}`);
		}

		const data = await response.json();
		if (!data.snapshot) {
			throw new Error('Version has no snapshot data');
		}

		// Decode base64 snapshot
		const snapshotData = Uint8Array.from(atob(data.snapshot), (c) => c.charCodeAt(0));
		const snapshot = Y.decodeSnapshot(snapshotData);

		// Create a document from the snapshot
		const restoredDoc = Y.createDocFromSnapshot(yjsDoc.ydoc, snapshot);
		const restoredText = restoredDoc.getText('content');
		const restoredContent = restoredText.toString();

		// Replace current content with restored content
		const { ytext, ydoc } = yjsDoc;
		ydoc.transact(() => {
			ytext.delete(0, ytext.length);
			ytext.insert(0, restoredContent);
		}, 'user');

		return true;
	} catch (err) {
		console.error('Failed to restore version:', err);
		return false;
	}
}
