/**
 * Citation system exports
 * Provides inline source citations like NotebookLM/Perplexity
 */

// Types
export type { Citation, CitationContext, DisplayInfo, SourceType } from '$lib/types/Citation';

// Mapping utilities
export {
	ONTOLOGY_DISPLAY,
	TOOL_DISPLAY,
	DEFAULT_DISPLAY,
	getDisplayInfo,
	inferSourceType,
	extractOntologyFromQuery
} from './mapping';

// Builder utilities
export {
	buildCitationContext,
	buildCitationContextFromParts,
	extractToolCallParts,
	isToolCallPart,
	hasCitations,
	getCitationByMarker,
	type ToolCallPart
} from './builder';
