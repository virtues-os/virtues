/**
 * Citation ids from tool calls (`buildCitationContext`). Each web_search
 * result is its own citation, so each needs its own id within the call:
 * ids that collide in `byToolCallId` make every result open the last one.
 */

import { describe, expect, it } from 'vitest';
import { buildCitationContext, citedRoutes, type ToolCallPart } from './builder';

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

/**
 * The citation card reads a cited record's passage from the turn's own tool
 * output, and steps through the answer's citations in the order it links them.
 */
describe('records by route', () => {
	const search = part('semantic_search', 'call_search', {
		results: [
			{
				ontology: 'app_page',
				record_id: 'page_one',
				rank: 1,
				title: 'Practice notes',
				preview: 'The bow still feels like a wild animal.',
				timestamp: '2024-04-12T20:00:00Z',
				ref: '/record/app_page/page_one',
			},
			{
				ontology: 'content_document',
				record_id: 'doc_one',
				rank: 2,
				title: 'Scales',
				preview: 'Page three.',
				ref: '/drive/file_one?page=3',
			},
		],
		count: 2,
	});
	const query = part('sql_query', 'call_rows', {
		operation: 'query',
		row_count: 1,
		columns: ['id', 'title'],
		rows: [{ id: 'evt_one', title: 'Winter recital', ref: '/record/calendar_event/evt_one' }],
		ref: '/chat/chat_one/tool/call_rows',
	});

	it('keeps each record a tool returned, with the passage it matched', () => {
		const ctx = buildCitationContext([search, query]);
		const page = ctx.byRoute.get('/record/app_page/page_one');
		expect(page).toMatchObject({ ontology: 'app_page', recordId: 'page_one', rank: 1, toolCallId: 'call_search' });
		expect(page?.passage).toBe('The bow still feels like a wild animal.');
		expect(ctx.byRoute.get('/record/calendar_event/evt_one')?.title).toBe('Winter recital');
	});

	it('leaves out refs that are not records', () => {
		const ctx = buildCitationContext([search]);
		expect([...ctx.byRoute.keys()]).toEqual(['/record/app_page/page_one']);
	});

	it('orders citations by their first link in the answer, once each', () => {
		const ctx = buildCitationContext([search, query]);
		const answer =
			'In April you wrote that [the bow](/record/app_page/page_one) was hard. ' +
			'You practiced [61 evenings](/chat/chat_one/tool/call_rows), ' +
			'played the [recital](/record/calendar_event/evt_one), and [again](/record/app_page/page_one). ' +
			'See [your day](/day/2024-12-07) and [a record no tool returned](/record/app_page/page_two).';
		expect(citedRoutes(answer, ctx)).toEqual([
			'/record/app_page/page_one',
			'/chat/chat_one/tool/call_rows',
			'/record/calendar_event/evt_one',
		]);
	});
});
