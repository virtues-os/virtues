/**
 * Version History for Yjs Documents
 *
 * Stores full document state for reliable version restoration.
 * Uses Y.encodeStateAsUpdate() which is self-contained and works with GC enabled.
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
	created_by: 'user' | 'ai' | 'auto';
	description?: string;
}

/**
 * Save the current document state as a version
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

		const response = await fetch(`/api/pages/${pageId}/versions`, {
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
 * Creates a fresh Y.Doc, applies the stored state, then copies
 * the text content into the live document.
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
