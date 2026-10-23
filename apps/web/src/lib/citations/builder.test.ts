/**
 * What a chat answer can cite, from its turn's tool calls
 * (`buildCitationContext`): queries by the call id their ref names, records
 * by route, web pages by address. Every value here is invented.
 */

import { describe, expect, it } from 'vitest';
import { buildCitationContext, queryResult, webKey, type ToolCallPart } from './builder';

function part(toolName: string, toolCallId: string, output: unknown, input: Record<string, unknown> = {}): ToolCallPart {
	return { type: `tool-${toolName}`, toolCallId, toolName, input, state: 'output-available', output };
}

describe('queries', () => {
	it('indexes a query by its own call id, the id a query ref names, with its SQL', () => {
		const ctx = buildCitationContext([
			part('sql_query', 'call_sql', { columns: ['n'], rows: [{ n: 3 }], row_count: 1, ref: '/chat/c1/tool/call_sql' }, { operation: 'query', sql: 'SELECT count(*) AS n FROM app_pages' }),
		]);
		expect(ctx.queries.get('call_sql')).toEqual({ columns: ['n'], rows: [{ n: 3 }], count: 1, more: false, sql: 'SELECT count(*) AS n FROM app_pages' });
	});

	it('counts the rows a result too large to carry returned, past the ones it kept', () => {
		const q = queryResult({ columns: ['body'], rows: [{ body: 'a' }, { body: 'b' }], row_count: 2, truncated: { returned: 40, shown: 2, why: '…' } });
		expect(q).toMatchObject({ count: 40, more: true });
	});

	it('says a show figure cut its rows, without a count', () => {
		const q = queryResult({ kind: 'table', columns: ['day'], rows: [{ day: '2031-11-02' }], row_count: 1, more: true });
		expect(q).toMatchObject({ count: 1, more: true });
	});

	it('leaves a row\'s ref out of the columns', () => {
		const q = queryResult({ rows: [{ id: 'evt_one', title: 'Winter recital', ref: '/record/calendar_event/evt_one' }] });
		expect(q?.columns).toEqual(['id', 'title']);
	});

	it('indexes a Deep Research worker\'s query by the call its ref names', () => {
		const ctx = buildCitationContext([
			part('dispatch_subagents', 'call_dr', {
				missions: [{ sources: [{ tool_name: 'sql_query', args: { sql: 'SELECT 1' }, data: { columns: ['x'], rows: [{ x: 1 }], ref: '/chat/c1/tool/call_worker' } }] }],
			}),
		]);
		expect(ctx.queries.get('call_worker')?.sql).toBe('SELECT 1');
	});
});

describe('web pages', () => {
	const search = part('web_search', 'call_web', {
		query: 'tide tables',
		results: [
			{ title: 'Tides one', url: 'https://www.example.com/tides/', text: 'High tide at 6:12 in the morning.', published_date: '2031-11-02' },
			{ title: 'Tides two', url: 'https://example.org/two#today' },
		],
	});

	it('keeps each page by its address, however the answer writes it', () => {
		const ctx = buildCitationContext([search]);
		expect(ctx.byUrl.get(webKey('https://example.com/tides'))).toMatchObject({ title: 'Tides one', text: 'High tide at 6:12 in the morning.', published: '2031-11-02' });
		expect(ctx.byUrl.get(webKey('https://example.org/two'))?.url).toBe('https://example.org/two#today');
	});

	it('is not a record or a query', () => {
		const ctx = buildCitationContext([search]);
		expect(ctx.byRoute.size).toBe(0);
		expect(ctx.queries.size).toBe(0);
	});
});

/**
 * The record card reads a cited record's passage from the turn's own tool
 * output.
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
});
