/**
 * What a chat answer cites: the records, queries and web pages its turn's
 * tools returned.
 */

export type { CitationContext, QueryResult, RecordSource, WebSource } from '$lib/types/Citation';

export {
	buildCitationContext,
	buildCitationContextFromParts,
	isToolCallPart,
	queryResult,
	webKey,
	type ToolCallPart
} from './builder';

export { citingSentences, findToolCall, type FoundCall, type Piece, type StoredTurn } from './query';
