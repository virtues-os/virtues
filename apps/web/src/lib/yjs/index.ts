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

export {
	setAIRanges,
	clearAIDecorations,
	aiDecorationField,
	aiDecorationTheme,
	aiDecorationExtension,
	highlightAIEdit,
	clearAIHighlights,
	type AIEditRange
} from './ai-decorations';

export {
	criticMarkupExtension,
	hasCriticMarkup,
	acceptAllChanges,
	rejectAllChanges,
	type CriticMarkupConfig
} from './critic-markup';

export { saveVersion, listVersions, restoreVersion, type PageVersion } from './versions';
