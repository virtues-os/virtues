/**
 * What a link in a chat answer renders as (`linkTarget`), the decision
 * `Markdown.svelte`'s link snippet branches on. A query ref must win over
 * the entity routes, or `/chat/` claims it as a link to a chat and the rows
 * behind a figure never open.
 */

import { describe, expect, it } from 'vitest';
import { linkTarget } from './refRoutes';

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
