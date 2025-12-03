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
	buildCitationsFromGrounding,
	mergeCitationContexts,
	extractToolCallParts,
	isToolCallPart,
	hasCitations,
	getCitationByMarker,
	type ToolCallPart,
	type GoogleGroundingMetadata,
	type GoogleGroundingChunk,
	type GoogleGroundingSupport
} from './builder';
