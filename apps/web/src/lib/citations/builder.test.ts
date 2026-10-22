/**
 * Citation ids from tool calls (`buildCitationContext`). Each web_search
 * result is its own citation, so each needs its own id within the call:
 * ids that collide in `byToolCallId` make every result open the last one.
 */

import { describe, expect, it } from 'vitest';
import { buildCitationContext, type ToolCallPart } from './builder';

function part(toolName: string, toolCallId: string, output: unknown): ToolCallPart {
	return { type: `tool-${toolName}`, toolCallId, toolName, input: {}, state: 'output-available', output };
}

describe('buildCitationContext', () => {
	it('gives each web_search result a distinct id, as the box sends them', () => {
		const ctx = buildCitationContext([
			part('web_search', 'call_web', {
				query: 'tide tables',
				results: [
					{ title: 'Tides one', url: 'https://example.com/one' },
					{ title: 'Tides two', url: 'https://example.com/two' },
				],
			}),
		]);
		expect(ctx.citations.map((c) => c.tool_call_id)).toEqual(['call_web-1', 'call_web-2']);
		expect(ctx.byToolCallId.get('call_web-1')?.url).toBe('https://example.com/one');
		expect(ctx.byToolCallId.get('call_web-2')?.url).toBe('https://example.com/two');
	});

	it('indexes a query by its own call id, the id a query ref names', () => {
		const ctx = buildCitationContext([
			part('sql_query', 'call_sql', { columns: ['n'], rows: [{ n: 3 }], row_count: 1 }),
		]);
		expect(ctx.byToolCallId.get('call_sql')?.tool_name).toBe('sql_query');
	});
});
