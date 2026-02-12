/**
 * Yjs Integration Module
 *
 * Provides real-time collaborative editing support via Yjs.
 */

export {
	createYjsDocument,
	applyMarkup,
	type YjsDocument,
	type MarkupInstruction
} from './document';

export { saveVersion, listVersions, restoreVersion, type PageVersion } from './versions';
