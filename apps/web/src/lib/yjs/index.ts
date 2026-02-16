/**
 * Yjs Integration Module
 *
 * Provides real-time collaborative editing support via Yjs.
 */

export {
	createYjsDocument,
	type YjsDocument
} from './document';

export { saveVersion, listVersions, restoreVersion, type PageVersion } from './versions';
