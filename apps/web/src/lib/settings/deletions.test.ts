// Deletions in words. Every deletion, count and date here is invented.
import { describe, expect, it } from 'vitest';
import type { Deletion } from '$lib/api/deletions';
import { alsoRemoved, deletionLead, deletionLede, deletionLine, deletionTitle, span, stillKept } from './deletions';

const NOW = new Date('2026-11-19T12:00:00').getTime();
const at = (local: string) => new Date(local).toISOString();

const deletion = (over: Partial<Deletion> = {}): Deletion => ({
	id: 'del_a',
	chat_id: 'chat_a',
	summary: 'your visits to the Riverside gym',
	status: 'done',
	targets: [],
	preview_error: null,
	receipt: { records: 41, earliest: at('2026-01-06T09:00:00'), latest: at('2026-10-24T18:00:00') },
	error: null,
	created_at: at('2026-10-28T09:00:00'),
	approved_at: at('2026-10-28T09:01:00'),
	finished_at: at('2026-10-28T09:02:00'),
	...over,
});

describe('deletionTitle', () => {
	it('capitalizes the summary the assistant wrote', () => {
		expect(deletionTitle(deletion())).toBe('Your visits to the Riverside gym');
	});
});

describe('deletionLine', () => {
	it('says when, where and how many', () => {
		expect(deletionLine(deletion(), NOW)).toBe('October 28, from a chat. 41 records.');
		expect(deletionLine(deletion({ chat_id: null, receipt: { records: 1 } }), NOW)).toBe('October 28. 1 record.');
	});

	it('says when you let the records back', () => {
		const lifted = deletion({ receipt: { records: 41, tombstones_lifted: 41, lifted_at: at('2026-10-30T10:00:00') } });
		expect(deletionLine(lifted, NOW)).toBe('October 28, from a chat. 41 records. On October 30 you let them be collected again.');
		expect(deletionLine(deletion({ receipt: { records: 41, tombstones_lifted: 41 } }), NOW)).toMatch(/You let them be collected again\.$/);
	});

	it('says what stopped one, and that one is running', () => {
		expect(deletionLine(deletion({ status: 'failed', error: 'Your server restarted before this finished.' }), NOW)).toBe(
			'October 28, from a chat. Stopped: Your server restarted before this finished.',
		);
		expect(deletionLine(deletion({ status: 'running' }), NOW)).toBe('October 28, from a chat. Deleting now.');
	});

	it('gives the year for a deletion from another year', () => {
		const old = deletion({ approved_at: at('2025-10-03T10:00:00') });
		expect(deletionLine(old, NOW)).toMatch(/^October 3, 2025, from a chat\./);
	});

	it('keeps the day it was allowed, however late it was settled', () => {
		const cut = deletion({ status: 'failed', error: 'Your server restarted before this finished.', finished_at: at('2026-11-18T10:00:00') });
		expect(deletionLine(cut, NOW)).toMatch(/^October 28, from a chat\./);
	});

	it('says when one stopped and was set aside', () => {
		expect(deletionLine(deletion({ status: 'cancelled' }), NOW)).toBe('October 28, from a chat. Stopped, and you set it aside.');
	});
});

describe('deletionLede', () => {
	it('says Deleted once it finished, and Started before', () => {
		expect(deletionLede(deletion())).toBe('Deleted October 28, 2026, after you asked in a chat.');
		expect(deletionLede(deletion({ status: 'failed', chat_id: null }))).toBe('Started October 28, 2026.');
	});
});

describe('deletionLead', () => {
	it('counts the records and gives their dates', () => {
		expect(deletionLead(deletion())).toBe('Your server deleted 41 records, from January 6 to October 24, 2026.');
	});

	it('says when nothing matched', () => {
		expect(deletionLead(deletion({ receipt: { records: 0 } }))).toBe('Nothing matched by the time it ran, so nothing was removed.');
	});
});

describe('span', () => {
	it('names one day, a span in a year, or a span across years', () => {
		expect(span(at('2025-03-02T09:00:00'), at('2025-03-02T18:00:00'))).toBe('March 2, 2025');
		expect(span(at('2025-03-02T09:00:00'), at('2025-03-30T18:00:00'))).toBe('March 2 to March 30, 2025');
		expect(span(at('2024-12-30T09:00:00'), at('2025-01-02T18:00:00'))).toBe('December 30, 2024 to January 2, 2025');
		expect(span(null, null)).toBe('');
	});
});

describe('what a receipt says', () => {
	it('lists what else went, and where copies stay', () => {
		expect(alsoRemoved({ search_entries: 214, links: 3, files: 1, raw_records: 214 })).toBe(
			'214 search entries, 3 links to people and places, 1 file and 214 raw copies',
		);
		expect(stillKept({ kept: ['backups', 'pages_and_chats'] })).toBe(
			'in backups made before this and in pages or chats that already quoted them',
		);
	});
});
