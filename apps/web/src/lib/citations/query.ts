/**
 * A query a chat answer cites: `/chat/<chat>/tool/<call>`, the rows a figure
 * was computed from. The record card reads its result from the turn in hand;
 * the query's own tab (QueryView) reads the result, the call that ran it and
 * the sentences that cite it from the stored chat.
 */

import { parseToolRef } from '$lib/utils/refRoutes';
import { markRanges } from '$lib/wiki/recordWords';
import { getToolName, isToolCallPart } from './builder';

/** A stored turn as the chat view holds it (`toUiMessage`). */
export interface StoredTurn {
	id: string;
	role: string;
	parts: unknown[];
	createdAt?: Date;
	startedAt?: Date;
}

export interface FoundCall {
	toolName: string;
	input: unknown;
	output: unknown;
	turn: StoredTurn;
}

/**
 * The call a query ref names, in a chat's stored turns. A Deep Research
 * worker's calls are not parts of the turn; its report's sources carry them,
 * each with the ref the box gave it.
 */
export function findToolCall(turns: StoredTurn[], route: string): FoundCall | null {
	const callId = parseToolRef(route)?.callId;
	if (!callId) return null;
	for (const turn of turns) {
		for (const part of turn.parts) {
			if (!isToolCallPart(part) || part.state !== 'output-available') continue;
			const toolName = getToolName(part);
			if (part.toolCallId === callId) return { toolName, input: part.input, output: part.output, turn };
			if (toolName !== 'dispatch_subagents') continue;
			const missions = (part.output as { missions?: { sources?: { tool_name: string; args?: unknown; data?: unknown }[] }[] })?.missions ?? [];
			for (const src of missions.flatMap((m) => m.sources ?? [])) {
				if ((src.data as { ref?: unknown } | undefined)?.ref === route) {
					return { toolName: src.tool_name, input: src.args, output: src.data, turn };
				}
			}
		}
	}
	return null;
}

export type Piece = { text: string; marked: boolean };

const LINK = /\[([^\]]*)\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g;
const LINE_MARK = /^\s*(?:[-*+]|\d+[.)]|#{1,6}|>)\s+/;
const plainText = (s: string) => s.replace(/\*+|`|__/g, '');

/**
 * Each sentence of a reply that links `route`, as its reader saw it: the
 * link's words marked, the markdown around them dropped.
 */
export function citingSentences(markdown: string, route: string): Piece[][] {
	const found: Piece[][] = [];
	for (const line of markdown.split(/\n+/)) {
		if (!line.includes(route)) continue;
		const body = line.replace(LINE_MARK, '');
		let text = '';
		let last = 0;
		const cited: [number, number][] = [];
		for (const m of body.matchAll(LINK)) {
			text += plainText(body.slice(last, m.index));
			const words = plainText(m[1]);
			if (m[2] === route) cited.push([text.length, text.length + words.length]);
			text += words;
			last = (m.index ?? 0) + m[0].length;
		}
		text += plainText(body.slice(last));
		if (!cited.length) continue;

		const sentences =
			typeof Intl.Segmenter === 'function'
				? [...new Intl.Segmenter(undefined, { granularity: 'sentence' }).segment(text)].map((s) => ({ at: s.index, text: s.segment }))
				: [{ at: 0, text }];
		for (const s of sentences) {
			const inside = cited
				.filter(([a]) => a >= s.at && a < s.at + s.text.length)
				.map(([a, b]): [number, number] => [a - s.at, Math.min(b, s.at + s.text.length) - s.at]);
			if (!inside.length) continue;
			const lead = s.text.length - s.text.trimStart().length;
			const trimmed = s.text.trim();
			found.push(markRanges(trimmed, inside.map(([a, b]): [number, number] => [Math.max(0, a - lead), Math.min(trimmed.length, b - lead)])));
		}
	}
	return found;
}
