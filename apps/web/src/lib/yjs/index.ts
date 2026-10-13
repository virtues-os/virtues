/**
 * Yjs Integration Module
 *
 * Provides real-time collaborative editing support via Yjs.
 */

export {
	createYjsDocument,
	createTreeDocument,
	type YjsDocument,
	type TreeDocument,
} from './document';

export {
	saveVersion,
	listVersions,
	restoreVersion,
	cutServerVersion,
	restorePageVersion,
	type PageVersion,
} from './versions';
