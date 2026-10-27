/**
 * A query's own tab reads its call and the sentences that cite it from the
 * stored chat (`findToolCall`, `citingSentences`). Every value is invented.
 */

import { describe, expect, it } from 'vitest';
import { citingSentences, findToolCall, type StoredTurn } from './query';

const ROUTE = '/chat/chat_one/tool/call_rows';

const turns: StoredTurn[] = [
	{ id: 'm1', role: 'user', parts: [{ type: 'text', text: 'How often did I practice?' }] },
	{
		id: 'm2',
		role: 'assistant',
		parts: [
			{ type: 'tool-sql_query', toolCallId: 'call_rows', toolName: 'sql_query', input: { sql: 'SELECT day FROM practice' }, state: 'output-available', output: { rows: [{ day: '2031-11-02' }], ref: ROUTE } },
			{
				type: 'tool-dispatch_subagents',
				toolCallId: 'call_dr',
				toolName: 'dispatch_subagents',
				input: {},
				state: 'output-available',
				output: { missions: [{ sources: [{ tool_name: 'sql_query', args: { sql: 'SELECT 2' }, data: { rows: [{ x: 2 }], ref: '/chat/chat_one/tool/call_worker' } }] }] },
			},
			{ type: 'text', text: 'You practiced on [61 evenings](/chat/chat_one/tool/call_rows).' },
		],
	},
];

describe('findToolCall', () => {
	it('finds the call a query ref names', () => {
		const found = findToolCall(turns, ROUTE);
		expect(found?.toolName).toBe('sql_query');
		expect(found?.input).toEqual({ sql: 'SELECT day FROM practice' });
		expect(found?.turn.id).toBe('m2');
	});

	it('finds a Deep Research worker\'s call by the ref its source carries', () => {
		expect(findToolCall(turns, '/chat/chat_one/tool/call_worker')?.input).toEqual({ sql: 'SELECT 2' });
	});

	it('finds nothing for a call the chat does not hold', () => {
		expect(findToolCall(turns, '/chat/chat_one/tool/call_gone')).toBeNull();
		expect(findToolCall(turns, '/record/calendar_event/evt_one')).toBeNull();
	});
});

describe('citingSentences', () => {
	it('gives the sentence that links the query, its link marked and the markdown gone', () => {
		const text = 'You kept at it. **Most weeks** you practiced on [61 evenings](/chat/chat_one/tool/call_rows), and the [recital](/record/calendar_event/evt_one) ran long. Then you rested.';
		expect(citingSentences(text, ROUTE)).toEqual([
			[
				{ text: 'Most weeks you practiced on ', marked: false },
				{ text: '61 evenings', marked: true },
				{ text: ', and the recital ran long.', marked: false },
			],
		]);
	});

	it('reads a list item without its mark, and each line on its own', () => {
		const text = '- First, [the counts](/chat/chat_one/tool/call_rows).\n- Second, nothing cited.';
		expect(citingSentences(text, ROUTE)).toEqual([
			[
				{ text: 'First, ', marked: false },
				{ text: 'the counts', marked: true },
				{ text: '.', marked: false },
			],
		]);
	});

	it('gives nothing when the text links another query', () => {
		expect(citingSentences('See [the rows](/chat/chat_one/tool/call_other).', ROUTE)).toEqual([]);
	});
});
