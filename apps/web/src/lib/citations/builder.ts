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

	// Push a citation, assigning it the current running index.
	const pushCitation = (c: Omit<Citation, 'id'>) => {
		const citation = { ...c, id: String(index) } as Citation;
		citations.push(citation);
		byId.set(citation.id, citation);
		byToolCallId.set(citation.tool_call_id, citation);
		index++;
	};

	// Build a citation for a single tool result (used for direct tool calls and for the
	// sources surfaced by Deep Research subagents).
	const citationForSource = (
		toolName: string,
		input: unknown,
		output: unknown,
		toolCallId: string
	) => {
		// web_search expands into one citation per result.
		if (toolName === 'web_search' && output) {
			const results = (output as Record<string, unknown>).results as
				| Array<{ position: number; title: string; url: string; summary?: string; text?: string }>
				| undefined;
			if (results && results.length > 0) {
				for (const result of results) {
					pushCitation({
						tool_call_id: `${toolCallId}-${result.position}`,
						tool_name: 'web_search',
						source_type: 'web_search',
						icon: 'ri:global-line',
						label: result.title?.slice(0, 40) || 'Web Result',
						color: 'text-blue-500',
						preview: result.summary || result.text?.slice(0, 100) || result.title || 'Web search result',
						data: result,
						args: input as Record<string, unknown> | undefined,
						url: result.url,
						title: result.title,
						timestamp: new Date().toISOString()
					});
				}
				return;
			}
		}

		const display = getDisplayInfo(toolName, input as Record<string, unknown> | undefined);
		pushCitation({
			tool_call_id: toolCallId,
			tool_name: toolName,
			source_type: inferSourceType(toolName),
			icon: display.icon,
			label: display.label,
			color: display.color,
			preview: buildPreview(toolName, output),
			data: output,
			args: input as Record<string, unknown> | undefined,
			timestamp: new Date().toISOString()
		});
	};

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

			// Deep Research: surface each subagent's underlying sources as citations, so a
			// worker's sql_query / web_search becomes a clickable reference in the report.
			if (toolName === 'dispatch_subagents' && part.output) {
				const missions = (part.output as Record<string, unknown>).missions as
					| Array<{ sources?: Array<{ tool_name: string; args?: unknown; data?: unknown }> }>
					| undefined;
				if (missions) {
					missions.forEach((mission, mi) => {
						(mission.sources ?? []).forEach((src, si) => {
							citationForSource(
								src.tool_name,
								src.args,
								src.data,
								`${part.toolCallId}-m${mi}-s${si}`
							);
						});
					});
				}
				continue;
			}

			// Direct tool call. web_search expands into one citation per result;
			// every other tool becomes a single citation. Both go through the shared
			// citationForSource helper (see the Deep Research path above).
			citationForSource(toolName, part.input, part.output, part.toolCallId);
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
 * Get citation by marker number (e.g., "1", "2", "3")
 */
export function getCitationByMarker(
	context: CitationContext | undefined,
	marker: string
): Citation | undefined {
	if (!context) return undefined;
	return context.byId.get(marker);
}
