/**
 * The record card, as a chat answer opens it: a click on a link to a record
 * opens one card for the answer, ← and → step through the records in the
 * order the answer links them and stop at the ends, Escape closes it, and a
 * query citation opens the same card with its rows. On a day page the card
 * takes the citation's own kicker and marks the words the sentence shares
 * with the record. Every record here is invented.
 */

import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';
import type { OntologyRecord } from '$lib/api/client';

const ROWS: Record<string, OntologyRecord> = {
	'calendar_event/evt_one': {
		ontology: 'calendar_event',
		record_id: 'evt_one',
		display_name: 'Calendar Events',
		table_name: 'data_calendar_event',
		timestamp_column: 'started_at',
		row: { id: 'evt_one', title: 'Winter recital', started_at: '2031-12-06T19:00:00Z', location_name: 'Lakeport Hall' },
	},
	'financial_transaction/txn_one': {
		ontology: 'financial_transaction',
		record_id: 'txn_one',
		display_name: 'Transactions',
		table_name: 'data_financial_transaction',
		timestamp_column: 'date',
		row: { id: 'txn_one', amount: 4250, currency: 'USD', merchant_name: 'Lakeport Strings', date: '2031-12-01T12:00:00Z' },
	},
	'data_communication_message/msg_one': {
		ontology: 'communication_message',
		record_id: 'msg_one',
		display_name: 'Messages',
		table_name: 'data_communication_message',
		timestamp_column: 'timestamp',
		row: { id: 'msg_one', body: 'Bring the rosin, the hall is cold.', from_name: 'Nick', timestamp: '2031-12-06T17:40:00Z' },
	},
};

vi.mock('$lib/api/client', async (original) => ({
	...(await original<typeof import('$lib/api/client')>()),
	getRecord: vi.fn(async (ontology: string, id: string) => {
		const r = ROWS[`${ontology}/${id}`];
		if (!r) throw new Error('not found');
		return r;
	}),
}));

const { default: Markdown } = await import('./Markdown.svelte');
const { default: RecordCard } = await import('./RecordCard.svelte');
const { buildCitationContext } = await import('$lib/citations/builder');

let open: ReturnType<typeof mount> | null = null;

afterEach(() => {
	if (open) unmount(open);
	open = null;
	document.body.replaceChildren();
});

async function settle() {
	for (let i = 0; i < 4; i++) {
		await tick();
		await new Promise((r) => setTimeout(r, 0));
	}
	flushSync();
}

const card = () => document.body.querySelector<HTMLElement>('.record-card');
const press = (key: string) => (document.activeElement ?? document.body).dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true }));

const answer =
	'You played the [winter recital](/record/calendar_event/evt_one) after buying ' +
	'[new strings](/record/financial_transaction/txn_one), and you practiced on ' +
	'[61 evenings](/chat/chat_one/tool/call_rows). The [recital](/record/calendar_event/evt_one) ran long.';

const citations = buildCitationContext([
	{
		type: 'tool-semantic_search',
		toolCallId: 'call_search',
		toolName: 'semantic_search',
		input: {},
		state: 'output-available',
		output: {
			results: [{ ontology: 'calendar_event', record_id: 'evt_one', rank: 1, title: 'Winter recital', ref: '/record/calendar_event/evt_one' }],
		},
	},
	{
		type: 'tool-sql_query',
		toolCallId: 'call_rows',
		toolName: 'sql_query',
		input: {},
		state: 'output-available',
		output: { operation: 'query', row_count: 61, columns: ['day', 'minutes'], rows: [{ day: '2031-11-02', minutes: 40 }], ref: '/chat/chat_one/tool/call_rows' },
	},
]);

async function renderAnswer() {
	open = mount(Markdown, { target: document.body, props: { content: answer, citations } });
	await settle();
	return [...document.body.querySelectorAll<HTMLElement>('[data-cite-route]')];
}

describe('the record card in a chat answer', () => {
	it('draws every link to a record, and the query, as a citation', async () => {
		const links = await renderAnswer();
		expect(links.map((a) => a.dataset.citeRoute)).toEqual([
			'/record/calendar_event/evt_one',
			'/record/financial_transaction/txn_one',
			'/chat/chat_one/tool/call_rows',
			'/record/calendar_event/evt_one',
		]);
		expect(links.every((a) => a.classList.contains('ref-link--cite'))).toBe(true);
	});

	it('opens on a click, steps in the answer\'s order, stops at the end, and closes on Escape', async () => {
		const [recital, strings] = await renderAnswer();
		recital.click();
		await settle();
		expect(card()?.getAttribute('role')).toBe('dialog');
		expect(card()?.textContent).toContain('Winter recital');
		expect(card()?.textContent).toContain('Lakeport Hall');
		expect(card()?.textContent).toContain('1 of 3');
		expect(recital.classList.contains('is-open')).toBe(true);

		press('ArrowRight');
		await settle();
		expect(card()?.textContent).toContain('2 of 3');
		expect(card()?.textContent).toContain('$42');
		expect(card()?.textContent).toContain('Lakeport Strings');
		expect(strings.classList.contains('is-open')).toBe(true);
		expect(recital.classList.contains('is-open')).toBe(false);

		press('ArrowRight');
		await settle();
		expect(card()?.textContent).toContain('61 rows');
		expect(card()?.textContent).toContain('Show all rows');

		press('ArrowRight');
		await settle();
		expect(card()?.textContent).toContain('3 of 3');

		press('Escape');
		await settle();
		expect(card()).toBeNull();
	});

	it('closes when the same citation is clicked again', async () => {
		const [recital] = await renderAnswer();
		recital.click();
		await settle();
		expect(card()).not.toBeNull();
		recital.click();
		await settle();
		expect(card()).toBeNull();
	});
});

describe('a web page in a chat answer', () => {
	it('cites a page the turn\'s search returned, and its card shows the page as the search read it', async () => {
		const web = buildCitationContext([
			{
				type: 'tool-web_search',
				toolCallId: 'call_web',
				toolName: 'web_search',
				input: {},
				state: 'output-available',
				output: {
					results: [
						{ title: 'Lakeport Hall reopens', url: 'https://www.example.com/hall/', text: 'The hall reopens after a winter of repairs to its heating.', published_date: '2031-12-01' },
					],
				},
			},
		]);
		open = mount(Markdown, {
			target: document.body,
			props: { content: 'The hall reopened after [its repairs](https://example.com/hall). There is [another site](https://example.org/) too.', citations: web },
		});
		await settle();
		const links = [...document.body.querySelectorAll<HTMLElement>('[data-cite-route]')];
		expect(links.map((a) => a.dataset.citeRoute)).toEqual(['https://example.com/hall']);

		links[0].click();
		await settle();
		const kick = card()?.querySelector('.kick')?.textContent ?? '';
		expect(kick).toContain('Web');
		expect(kick).toContain('example.com');
		expect(card()?.textContent).toContain('Lakeport Hall reopens');
		expect([...(card()?.querySelectorAll('mark') ?? [])].map((m) => m.textContent).join(' ')).toContain('repairs');
		expect(card()?.textContent).toContain('Open site');
	});
});

describe('the record card on a day page', () => {
	it('keeps the citation\'s kicker and marks the words the sentence shares', async () => {
		const anchor = document.createElement('span');
		document.body.append(anchor);
		open = mount(RecordCard, {
			target: document.body,
			props: {
				anchor,
				sources: [{ ref: 'data_communication_message:msg_one', label: 'Message · 5:40 PM' }],
				sentence: 'Nick said the hall would be cold.',
				mode: 'popover',
				when: 'time',
				openLabel: 'Open in Data',
				onopen: () => {},
				onclose: () => {},
			},
		});
		await settle();
		expect(card()?.querySelector('.kick')?.textContent).toContain('Message');
		expect(card()?.querySelector('.kick')?.textContent).toContain('5:40 PM');
		expect(card()?.querySelector('.kick')?.textContent).toContain('Nick');
		expect([...(card()?.querySelectorAll('mark') ?? [])].map((m) => m.textContent)).toEqual(['hall is cold']);
		expect(card()?.textContent).toContain('Open in Data');
	});
});
