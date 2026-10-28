/**
 * What a link in a chat answer renders as (`linkTarget`), the decision
 * `Markdown.svelte`'s link snippet branches on. A query ref must win over
 * the entity routes, or `/chat/` claims it as a link to a chat and the rows
 * behind a figure never open.
 */

import { describe, expect, it } from 'vitest';
import { isRecordRoute, linkTarget, parseRecordRef, parseToolRef } from './refRoutes';

describe('parseToolRef', () => {
	it('reads a query ref as its chat and call', () => {
		expect(parseToolRef('/chat/chat_abc/tool/call_123')).toEqual({ chatId: 'chat_abc', callId: 'call_123' });
	});

	it('reads a chat, and anything past the call, as no query', () => {
		expect(parseToolRef('/chat/chat_abc')).toBeNull();
		expect(parseToolRef('/chat/chat_abc/tool/call_1/extra')).toBeNull();
		expect(parseToolRef('https://example.com/chat/x/tool/y')).toBeNull();
	});
});

describe('parseRecordRef', () => {
	it('reads a record route and a day article citation alike', () => {
		expect(parseRecordRef('/record/calendar_event/evt_1')).toEqual({ ontology: 'calendar_event', id: 'evt_1' });
		expect(parseRecordRef('data_communication_message:msg_9')).toEqual({ ontology: 'data_communication_message', id: 'msg_9' });
		expect(parseRecordRef('app_pages:pg_2')).toEqual({ ontology: 'app_pages', id: 'pg_2' });
	});

	it('reads nothing else as a record', () => {
		expect(parseRecordRef('/person/david-okafor')).toBeNull();
		expect(parseRecordRef('/chat/c1/tool/call_1')).toBeNull();
		expect(parseRecordRef('mailto:nick@example.com')).toBeNull();
		expect(parseRecordRef('https://example.com/record/a/b')).toBeNull();
	});

	it('takes only routes as links to records', () => {
		expect(isRecordRoute('/record/financial_transaction/txn_4')).toBe(true);
		expect(isRecordRoute('data_communication_message:msg_9')).toBe(false);
		expect(isRecordRoute(undefined)).toBe(false);
	});
});

describe('linkTarget', () => {
	it('reads a query ref as the tool call it names', () => {
		expect(linkTarget('/chat/chat_abc/tool/call_123')).toEqual({ kind: 'query', callId: 'call_123' });
		expect(linkTarget('/chat/x/tool/y')).toEqual({ kind: 'query', callId: 'y' });
	});

	it('reads a chat that is not a query ref as an entity', () => {
		expect(linkTarget('/chat/chat_abc')).toEqual({ kind: 'entity' });
		expect(linkTarget('/chat/chat_abc/tool/')).toEqual({ kind: 'entity' });
		expect(linkTarget('/chat/chat_abc/tool/call_1/extra')).toEqual({ kind: 'entity' });
	});

	it('reads record and other entity routes as entities', () => {
		expect(linkTarget('/record/health_sleep/rec_1')).toEqual({ kind: 'entity' });
		expect(linkTarget('/person/david-okafor')).toEqual({ kind: 'entity' });
		expect(linkTarget('/notebook/nb_1')).toEqual({ kind: 'entity' });
	});

	it('reads the web as external', () => {
		expect(linkTarget('https://example.com/chat/x/tool/y')).toEqual({ kind: 'external' });
		expect(linkTarget('http://example.com')).toEqual({ kind: 'external' });
	});

	it('leaves anything else, and nothing, to the plain branches', () => {
		expect(linkTarget('/settings')).toEqual({ kind: 'other' });
		expect(linkTarget('mailto:nick@example.com')).toEqual({ kind: 'other' });
		expect(linkTarget('')).toEqual({ kind: 'none' });
		expect(linkTarget(undefined)).toEqual({ kind: 'none' });
	});
});
