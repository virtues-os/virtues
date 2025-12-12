/**
 * Citation context builder
 * Transforms tool call parts into a citation context for rendering
 */

import type { Citation, CitationContext } from '$lib/types/Citation';
import { getDisplayInfo, inferSourceType } from './mapping';

/**
 * Tool call part from AI SDK UIMessage
 * This matches the structure used in +page.svelte
 */
export interface ToolCallPart {
	type: string; // "tool-{toolName}"
	toolCallId: string;
	toolName: string;
	input: Record<string, unknown>;
	state: 'pending' | 'output-available' | 'output-error';
	output?: unknown;
	errorText?: string;
}

/**
 * Check if a message part is a tool call part
 */
export function isToolCallPart(part: unknown): part is ToolCallPart {
	if (!part || typeof part !== 'object') return false;
	const p = part as Record<string, unknown>;
	return typeof p.type === 'string' && p.type.startsWith('tool-');
}

/**
 * Extract tool name from a tool call part
 * During streaming, AI SDK provides type: 'tool-{toolName}' but may not include toolName property
 * This function derives toolName from type when not explicitly provided
 */
export function getToolName(part: ToolCallPart): string {
	// If toolName is explicitly provided, use it
	if (part.toolName) return part.toolName;
	// Otherwise derive from type (format: 'tool-{toolName}')
	if (part.type?.startsWith('tool-')) {
		return part.type.slice(5); // Remove 'tool-' prefix
	}
	return '';
}

/**
 * Extract tool call parts from message parts array
 */
export function extractToolCallParts(parts: unknown[]): ToolCallPart[] {
	return parts.filter(isToolCallPart);
}

/**
 * Build a preview string from tool output
 */
function buildPreview(toolName: string, output: unknown): string {
	if (!output || typeof output !== 'object') {
		return 'Data retrieved';
	}

	const result = output as Record<string, unknown>;

	// Handle error case
	if (result.error) {
		return `Error: ${String(result.error).slice(0, 50)}`;
	}

	// Handle different tool output formats
	switch (toolName) {
		case 'web_search': {
			const results = result.results as unknown[] | undefined;
			const query = result.query as string | undefined;
			if (results) {
				return query
					? `${results.length} results for "${query.slice(0, 30)}${query.length > 30 ? '...' : ''}"`
					: `${results.length} results`;
			}
			break;
		}

		case 'virtues_query_ontology': {
			const rows = result.rows as unknown[] | undefined;
			const rowCount = (result.row_count as number) ?? rows?.length ?? 0;
			if (rowCount > 0) {
				return `${rowCount} record${rowCount !== 1 ? 's' : ''}`;
			}
			return 'No data found';
		}

		case 'virtues_query_narratives': {
			const narratives = result.narratives as unknown[] | undefined;
			const count = (result.narrative_count as number) ?? narratives?.length ?? 0;
			if (count > 0) {
				return `${count} narrative${count !== 1 ? 's' : ''}`;
			}
			return 'No narratives found';
		}

		case 'virtues_query_axiology': {
			// Try to summarize what was found
			const keys = Object.keys(result).filter((k) => !['success', 'error'].includes(k));
			if (keys.length > 0) {
				const items: string[] = [];
				for (const key of keys.slice(0, 3)) {
					const val = result[key];
					if (Array.isArray(val) && val.length > 0) {
						items.push(`${val.length} ${key}`);
					}
				}
				if (items.length > 0) {
					return items.join(', ');
				}
			}
			return 'Values retrieved';
		}

		case 'virtues_semantic_search': {
			const searchResults = result.results as unknown[] | undefined;
			if (searchResults) {
				return `${searchResults.length} matches`;
			}
			break;
		}

		case 'query_location_map': {
			const data = result.data as unknown[] | undefined;
			if (data) {
				return `${data.length} location${data.length !== 1 ? 's' : ''}`;
			}
			return 'Location data';
		}

		default: {
			// Generic handling for unknown tools
			if (result.rows && Array.isArray(result.rows)) {
				return `${(result.rows as unknown[]).length} records`;
			}
			if (result.results && Array.isArray(result.results)) {
				return `${(result.results as unknown[]).length} results`;
			}
		}
	}

	return 'Data retrieved';
}

/**
 * Extract URL from web search result (for citation linking)
 */
function extractUrl(part: ToolCallPart, toolName: string): string | undefined {
	if (toolName !== 'web_search' || !part.output) return undefined;

	const result = part.output as Record<string, unknown>;
	const results = result.results as Array<{ url?: string }> | undefined;

	// Return first result URL if available
	return results?.[0]?.url;
}

/**
 * Extract title from web search result
 */
function extractTitle(part: ToolCallPart, toolName: string): string | undefined {
	if (toolName !== 'web_search' || !part.output) return undefined;

	const result = part.output as Record<string, unknown>;
	const results = result.results as Array<{ title?: string }> | undefined;

	return results?.[0]?.title;
}

/**
 * Build citation context from an array of tool call parts
 *
 * @param toolCallParts - Array of tool call parts from message.parts
 * @returns CitationContext with citations and lookup maps
 */
export function buildCitationContext(toolCallParts: ToolCallPart[]): CitationContext {
	const citations: Citation[] = [];
	const byId = new Map<string, Citation>();
	const byToolCallId = new Map<string, Citation>();

	let index = 1;

	for (const part of toolCallParts) {
		try {
			// Only include completed tool calls
			if (part.state !== 'output-available') continue;

			// Derive toolName from type if not explicitly provided (happens during streaming)
			const toolName = getToolName(part);

			// Validate required fields
			if (!toolName || !part.toolCallId) {
				continue;
			}

			// Special handling for web_search: expand results into individual citations
			// This allows [1], [2], [3] markers to map to individual search results
			if (toolName === 'web_search' && part.output) {
				const output = part.output as Record<string, unknown>;
				const results = output.results as Array<{
					position: number;
					title: string;
					url: string;
					summary?: string;
					text?: string;
				}> | undefined;

				if (results && results.length > 0) {
					for (const result of results) {
						const citation: Citation = {
							id: String(index),
							tool_call_id: `${part.toolCallId}-${result.position}`,
							tool_name: toolName,
							source_type: 'web_search',
							icon: 'ri:global-line',
							label: result.title?.slice(0, 40) || 'Web Result',
							color: 'text-blue-500',
							preview: result.summary || result.text?.slice(0, 100) || result.title || 'Web search result',
							data: result,
							args: part.input,
							url: result.url,
							title: result.title,
							timestamp: new Date().toISOString()
						};

						citations.push(citation);
						byId.set(citation.id, citation);
						byToolCallId.set(citation.tool_call_id, citation);
						index++;
					}
					continue; // Skip the default single-citation handling
				}
			}

			const display = getDisplayInfo(toolName, part.input);
			const preview = buildPreview(toolName, part.output);
			const sourceType = inferSourceType(toolName);

			const citation: Citation = {
				id: String(index),
				tool_call_id: part.toolCallId,
				tool_name: toolName,
				source_type: sourceType,
				icon: display.icon,
				label: display.label,
				color: display.color,
				preview,
				data: part.output,
				args: part.input,
				url: extractUrl(part, toolName),
				title: extractTitle(part, toolName),
				timestamp: new Date().toISOString()
			};

			citations.push(citation);
			byId.set(citation.id, citation);
			byToolCallId.set(part.toolCallId, citation);

			index++;
		} catch (error) {
			// Log error but continue processing other parts
			console.error('[buildCitationContext] Error processing tool part:', error, part);
		}
	}

	return {
		citations,
		byId,
		byToolCallId
	};
}

/**
 * Build citation context from a full message's parts array
 * Convenience function that extracts tool calls and builds context
 *
 * @param parts - Full parts array from a UIMessage
 * @returns CitationContext
 */
export function buildCitationContextFromParts(parts: unknown[]): CitationContext {
	const toolCallParts = extractToolCallParts(parts);
	return buildCitationContext(toolCallParts);
}

/**
 * Check if a citation context has any citations
 */
export function hasCitations(context: CitationContext | undefined): boolean {
	return !!context && context.citations.length > 0;
}

/**
 * Google Grounding Metadata types
 * These come from providerMetadata.google.groundingMetadata
 */
export interface GoogleGroundingChunk {
	web?: {
		uri: string;
		title: string;
	};
}

export interface GoogleGroundingSupport {
	segment: {
		startIndex: number;
		endIndex: number;
	};
	groundingChunkIndices: number[];
	confidenceScores: number[];
}

export interface GoogleGroundingMetadata {
	groundingChunks?: GoogleGroundingChunk[];
	groundingSupports?: GoogleGroundingSupport[];
	webSearchQueries?: string[];
}

/**
 * Build citations from Google grounding metadata
 * This handles the different format from Google's native search grounding
 *
 * @param groundingMetadata - Grounding metadata from providerMetadata.google.groundingMetadata
 * @returns CitationContext with citations from grounding chunks
 */
export function buildCitationsFromGrounding(groundingMetadata: GoogleGroundingMetadata): CitationContext {
	const citations: Citation[] = [];
	const byId = new Map<string, Citation>();
	const byToolCallId = new Map<string, Citation>();

	const chunks = groundingMetadata.groundingChunks || [];

	for (let i = 0; i < chunks.length; i++) {
		const chunk = chunks[i];
		if (!chunk.web) continue;

		const citation: Citation = {
			id: String(i + 1),
			tool_call_id: `grounding-${i}`,
			tool_name: 'googleSearch',
			source_type: 'web_search',
			icon: 'ri:google-line',
			label: 'Web Search',
			color: 'text-blue-500',
			preview: chunk.web.title || 'Web result',
			url: chunk.web.uri,
			title: chunk.web.title,
			timestamp: new Date().toISOString()
		};

		citations.push(citation);
		byId.set(citation.id, citation);
		byToolCallId.set(citation.tool_call_id, citation);
	}

	return {
		citations,
		byId,
		byToolCallId
	};
}

/**
 * Merge tool call citations with grounding citations
 * Used when both MCP tools and Google grounding are used in the same response
 *
 * @param toolContext - Citations from tool calls
 * @param groundingContext - Citations from Google grounding
 * @returns Merged CitationContext
 */
export function mergeCitationContexts(
	toolContext: CitationContext,
	groundingContext: CitationContext
): CitationContext {
	const citations: Citation[] = [...toolContext.citations];
	const byId = new Map(toolContext.byId);
	const byToolCallId = new Map(toolContext.byToolCallId);

	// Renumber grounding citations to continue from tool citations
	const startIndex = toolContext.citations.length;
	for (let i = 0; i < groundingContext.citations.length; i++) {
		const citation = {
			...groundingContext.citations[i],
			id: String(startIndex + i + 1)
		};
		citations.push(citation);
		byId.set(citation.id, citation);
		byToolCallId.set(citation.tool_call_id, citation);
	}

	return {
		citations,
		byId,
		byToolCallId
	};
}

/**
 * Get citation by marker number (e.g., "1", "2", "3")
 */
export function getCitationByMarker(
	context: CitationContext | undefined,
	marker: string
): Citation | undefined {
	if (!context) return undefined;
	return context.byId.get(marker);
}
