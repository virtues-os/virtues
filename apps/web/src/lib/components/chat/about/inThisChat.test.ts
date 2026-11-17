/**
 * "In this chat" read from a transcript. Every value here is invented.
 */

import { describe, expect, it } from 'vitest';
import { inThisChat } from './inThisChat';

const text = (t: string) => ({ type: 'text', text: t });
const tool = (name: string, toolCallId: string, input: Record<string, unknown>, output: unknown) => ({
	type: `tool-${name}`,
	toolCallId,
	toolName: name,
	input,
	state: 'output-available',
	output,
});

describe('inThisChat', () => {
	it('lists people and places the replies name, once each, in order of first appearance', () => {
		const panel = inThisChat([
			{ id: 'a1', role: 'assistant', parts: [text('You met [Nick](/person/nick) at [Harbor Books](/place/harbor-books).')] },
			{ id: 'a2', role: 'assistant', parts: [text('[Nick](/person/nick) and [David Okafor](/person/david-okafor) were both there.')] },
		]);
		expect(panel.mentioned.map((r) => r.label)).toEqual(['Nick', 'Harbor Books', 'David Okafor']);
		expect(panel.mentioned[0].messageIds).toEqual(['a1', 'a2']);
	});

	it('splits what a reply cites from what its tools only read', () => {
		const panel = inThisChat([
			{
				id: 'a1',
				role: 'assistant',
				parts: [
					tool('semantic_search', 'call_1', { query: 'recital' }, {
						results: [
							{ ref: '/record/calendar_event/evt_one', title: 'Winter recital' },
							{ ref: '/record/calendar_event/evt_two', title: 'Choir rehearsal' },
						],
					}),
					text('The [recital](/record/calendar_event/evt_one) ran late.'),
				],
			},
		]);
		expect(panel.sources.map((r) => r.label)).toEqual(['Winter recital']);
		expect(panel.alsoRead.map((r) => r.label)).toEqual(['Choir rehearsal']);
	});

	it('lists a page once, by its title, whether a reply cites it as a record or a tool read it', () => {
		const panel = inThisChat([
			{
				id: 'a1',
				role: 'assistant',
				parts: [
					tool('get_page_content', 'call_p', { page_id: 'page_garden' }, { page_id: 'page_garden', title: 'Garden notes' }),
					tool('get_page_content', 'call_q', { page_id: 'page_shed' }, { page_id: 'page_shed', title: 'Shed plans' }),
					text('The beds go in first ([page](/record/app_page/page_garden)).'),
				],
			},
		]);
		expect(panel.sources.map((r) => [r.label, r.route])).toEqual([['Garden notes', '/page/page_garden']]);
		expect(panel.alsoRead.map((r) => r.label)).toEqual(['Shed plans']);
	});

	it('names every web search and browse as leaving the box', () => {
		const panel = inThisChat([
			{
				id: 'a1',
				role: 'assistant',
				parts: [
					tool('web_search', 'call_w', { query: 'tide tables for the weekend' }, { results: [{ url: 'https://example.com/tides', title: 'Tides' }] }),
					tool('browse', 'call_b', { task: 'Find the ferry schedule' }, {}),
					text('High tide is at noon ([Tides](https://example.com/tides/)).'),
				],
			},
		]);
		expect(panel.leftTheBox.map((r) => r.detail)).toEqual(['Web search', 'Browsed the web']);
		expect(panel.sources.map((r) => r.label)).toEqual(['Tides']);
		expect(panel.alsoRead).toEqual([]);
	});

	it('names the model that wrote each answer once, with its provider', () => {
		const panel = inThisChat([
			{ id: 'a1', role: 'assistant', model: 'acme/writer-2', provider: 'acme', parts: [text('One.')] },
			{ id: 'a2', role: 'assistant', model: 'acme/writer-2', provider: 'acme', parts: [text('Two.')] },
		]);
		expect(panel.writers.map((r) => [r.label, r.detail, r.messageIds])).toEqual([['acme/writer-2', 'acme', ['a1', 'a2']]]);
	});

	it('leaves the box empty when no tool reached outside', () => {
		const panel = inThisChat([{ id: 'a1', role: 'assistant', parts: [text('Nothing to look up.')] }]);
		expect(panel.leftTheBox).toEqual([]);
	});

	it('lists your attachments and @-mentions as brought in', () => {
		const panel = inThisChat([
			{
				id: 'u1',
				role: 'user',
				parts: [
					{ type: 'file', filename: 'floor-plan.pdf', mediaType: 'application/pdf', url: '/api/assets/asset_one' },
					text('Compare this with [@Garden notes](/page/page_garden).'),
				],
			},
		]);
		expect(panel.broughtIn.map((r) => [r.label, r.detail])).toEqual([
			['floor-plan.pdf', 'Attached'],
			['Garden notes', 'You mentioned'],
		]);
	});

	it('lists something you brought in once, even after a tool reads it', () => {
		const panel = inThisChat([
			{ id: 'u1', role: 'user', parts: [text('What is in [@Kitchen plans](/project/proj_kitchen)?')] },
			{
				id: 'a1',
				role: 'assistant',
				parts: [
					tool('get_project_item', 'call_k', { item_url: '/project/proj_kitchen' }, { type: 'project', name: 'Kitchen plans' }),
					tool('get_project_item', 'call_c', { item_url: '/chat/chat_tiles' }, { type: 'chat', title: 'Tile samples' }),
				],
			},
		]);
		expect(panel.broughtIn.map((r) => r.label)).toEqual(['Kitchen plans']);
		expect(panel.alsoRead.map((r) => [r.label, r.detail])).toEqual([['Tile samples', 'Chat']]);
	});

	it('lists a page made and then edited in the same chat once, as new', () => {
		const panel = inThisChat([
			{
				id: 'a1',
				role: 'assistant',
				parts: [
					tool('create_page', 'c1', {}, { page_id: 'page_trip', title: 'Trip plan' }),
					tool('edit_page', 'c2', {}, { applied: true, page_title: 'Trip plan', edit: { page_id: 'page_trip' } }),
					tool('show', 'c3', { kind: 'map' }, { kind: 'map', title: 'Where you walked', rows: [] }),
					tool('create_task', 'c4', {}, { task: { id: 'task_one', title: 'Book the ferry' } }),
				],
			},
		]);
		expect(panel.made.map((r) => [r.label, r.detail])).toEqual([
			['Trip plan', 'New page'],
			['Where you walked', 'Map'],
			['Book the ferry', 'Task added'],
		]);
	});
});
