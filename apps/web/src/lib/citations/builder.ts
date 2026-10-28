/**
 * What a chat answer can cite, from its turn's tool calls: the records,
 * queries and web pages they returned (`CitationContext`).
 */

import type { CitationContext, QueryResult, RecordSource, WebSource } from '$lib/types/Citation';
import { parseRecordRef, parseToolRef } from '$lib/utils/refRoutes';

/**
 * Tool call part from AI SDK UIMessage
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
 * The result of a call that read rows, or null when its output has none.
 * Both tools that read rows (`sql_query`, `show`) take their SQL as `sql`.
 */
export function queryResult(output: unknown, input?: unknown): QueryResult | null {
	if (!output || typeof output !== 'object') return null;
	const o = output as Record<string, unknown>;
	if (!Array.isArray(o.rows)) return null;
	const rows = o.rows.filter((r): r is Record<string, unknown> => !!r && typeof r === 'object');
	const named = Array.isArray(o.columns) && o.columns.every((c) => typeof c === 'string');
	const columns = (named ? (o.columns as string[]) : Object.keys(rows[0] ?? {})).filter((c) => c !== 'ref');

	// sql_query drops rows from the end of a result too large to carry, and a
	// saved result carries a preview; each says how many rows there were.
	// `show` says only that it cut its rows.
	const truncated = o.truncated as { returned?: unknown } | undefined;
	const saved = o.saved as { rows?: unknown } | undefined;
	const returned =
		typeof truncated?.returned === 'number' ? truncated.returned
		: typeof saved?.rows === 'number' ? saved.rows
		: rows.length;
	const count = Math.max(returned, typeof o.row_count === 'number' ? o.row_count : 0, rows.length);

	const sql = (input as { sql?: unknown } | undefined)?.sql;
	return { columns, rows, count, more: count > rows.length || o.more === true, sql: typeof sql === 'string' ? sql : undefined };
}

/**
 * A web address as the answer and the search may each have written it: the
 * host without `www.`, no fragment, no trailing slash.
 */
export function webKey(url: string): string {
	try {
		const u = new URL(url);
		return `${u.host.replace(/^www\./, '').toLowerCase()}${u.pathname.replace(/\/+$/, '')}${u.search}`;
	} catch {
		return url;
	}
}

/** Build the citation context from an array of tool call parts. */
export function buildCitationContext(toolCallParts: ToolCallPart[]): CitationContext {
	const queries = new Map<string, QueryResult>();
	const byRoute = new Map<string, RecordSource>();
	const byUrl = new Map<string, WebSource>();

	// Every record a tool returned with a `/record/` ref: semantic_search's
	// results, and the rows of a single-table sql_query. The first sighting
	// wins, so a record two searches found keeps the passage of the first.
	// The same goes for a page two web searches found.
	const index = (toolName: string, input: unknown, output: unknown, toolCallId: string) => {
		if (!output || typeof output !== 'object') return;
		const body = output as Record<string, unknown>;

		const query = queryResult(output, input);
		if (query) {
			// A query is cited by its own ref, which names the call that ran it.
			// A Deep Research worker's call is known only by that ref.
			const callId = (typeof body.ref === 'string' ? parseToolRef(body.ref)?.callId : undefined) ?? toolCallId;
			if (!queries.has(callId)) queries.set(callId, query);
		}

		if (toolName === 'web_search' && Array.isArray(body.results)) {
			for (const r of body.results as Record<string, unknown>[]) {
				if (typeof r?.url !== 'string') continue;
				const key = webKey(r.url);
				if (byUrl.has(key)) continue;
				byUrl.set(key, {
					url: r.url,
					title: typeof r.title === 'string' ? r.title : r.url,
					text: typeof r.text === 'string' ? r.text : undefined,
					published: typeof r.published_date === 'string' ? r.published_date : undefined,
					toolCallId
				});
			}
			return;
		}

		const lists = [body.results, body.rows].filter(Array.isArray) as unknown[][];
		for (const list of lists) {
			for (const item of list) {
				if (!item || typeof item !== 'object') continue;
				const r = item as Record<string, unknown>;
				const route = typeof r.ref === 'string' ? r.ref : '';
				const record = route.startsWith('/record/') ? parseRecordRef(route) : null;
				if (!record || byRoute.has(route)) continue;
				byRoute.set(route, {
					route,
					ontology: record.ontology,
					recordId: record.id,
					title: typeof r.title === 'string' ? r.title : undefined,
					passage: typeof r.preview === 'string' ? r.preview : undefined,
					timestamp: typeof r.timestamp === 'string' ? r.timestamp : undefined,
					rank: typeof r.rank === 'number' ? r.rank : undefined,
					toolCallId
				});
			}
		}
	};

	for (const part of toolCallParts) {
		if (part.state !== 'output-available' || !part.toolCallId) continue;
		// Derived from the type when the part has no toolName (while streaming).
		const toolName = getToolName(part);
		if (!toolName) continue;

		// Deep Research: what each worker's calls returned is citable in the
		// report, as a direct call's is in an answer.
		if (toolName === 'dispatch_subagents') {
			const missions = (part.output as { missions?: { sources?: { tool_name: string; args?: unknown; data?: unknown }[] }[] })?.missions ?? [];
			missions.forEach((mission, mi) => {
				(mission.sources ?? []).forEach((src, si) => index(src.tool_name, src.args, src.data, `${part.toolCallId}-m${mi}-s${si}`));
			});
			continue;
		}

		index(toolName, part.input, part.output, part.toolCallId);
	}

	return { queries, byRoute, byUrl };
}

/** Build the citation context from a full message's parts array. */
export function buildCitationContextFromParts(parts: unknown[]): CitationContext {
	return buildCitationContext(parts.filter(isToolCallPart));
}
