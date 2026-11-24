import { describe, expect, it } from 'vitest';
import type { BookmarkApi } from './api';
import { crumbOf, dayLabel, footerOf, groupBySave, groupOf, intoColumns, openLabel, shapeOf } from './shape';

function bookmark(over: Partial<BookmarkApi>): BookmarkApi {
	return {
		id: 'b1',
		url: 'https://example.com/a',
		title: null,
		description: null,
		note: null,
		source_platform: 'chrome',
		bookmark_type: 'bookmark',
		author: null,
		tags: null,
		picture_url: null,
		door: 'browser',
		details: {},
		timestamp: '2026-11-20T12:00:00Z',
		saved_at: '2026-11-20T12:00:00Z',
		medium: null,
		...over,
	};
}

// Tuesday, so this week began yesterday.
const NOW = new Date(2026, 10, 24, 12, 0, 0);

describe('shapeOf', () => {
	it('never makes a domain the headline of a post', () => {
		const s = shapeOf(
			bookmark({
				url: 'https://x.com/nick/status/1',
				source_platform: 'x',
				door: 'x',
				author: 'nick',
				title: 'Nick (@nick) on X',
				description: 'The dishwasher door is the hard part.',
			})
		);
		expect(s.kind).toBe('post');
		expect(s.headline).toBe('The dishwasher door is the hard part.');
		expect(s.handle).toBe('@nick');
		expect(s.pictureFirst).toBe(false);
	});

	it('puts an Instagram picture before its caption', () => {
		const s = shapeOf(
			bookmark({ source_platform: 'instagram', door: 'instagram', description: 'Lentil stew', picture_url: '/api/drive/files/f1/download' })
		);
		expect(s.kind).toBe('post');
		expect(s.pictureFirst).toBe(true);
		expect(s.picture).toContain('/api/drive/files/f1/download');
	});

	it('leads with a title, under its site', () => {
		const s = shapeOf(bookmark({ title: 'Why cheap robot arms wobble', description: 'Backlash.' }));
		expect(s).toMatchObject({ kind: 'page', headline: 'Why cheap robot arms wobble', site: 'example.com', summary: 'Backlash.' });
	});

	it('shows a picture alone when there is nothing else', () => {
		const s = shapeOf(bookmark({ url: '/drive/file_x', door: 'iphone', picture_url: '/api/drive/files/file_x/download' }));
		expect(s.kind).toBe('picture');
		expect(s.headline).toBeNull();
	});

	it('falls back to the address, and names the local network', () => {
		const s = shapeOf(bookmark({ url: 'http://192.168.1.1/' }));
		expect(s).toMatchObject({ kind: 'address', headline: '192.168.1.1', site: 'Local network' });
	});
});

describe('details', () => {
	it('shows where short links lead and drops the picture link', () => {
		const s = shapeOf(
			bookmark({
				source_platform: 'x',
				door: 'x',
				author: 'nick',
				description: 'read this https://t.co/aaa https://t.co/pic',
				details: {
					links: [{ short: 'https://t.co/aaa', expanded: 'https://example.com/essay', display: 'example.com/essay' }],
					media_short_url: 'https://t.co/pic',
					link_preview: { url: 'https://example.com/essay', title: 'An essay' },
				},
			})
		);
		expect(s.headline).toBe('read this example.com/essay');
		expect(s.preview).toMatchObject({ site: 'example.com', title: 'An essay', picture: null });
	});

	it('reads an older post author from the kept page title', () => {
		const s = shapeOf(
			bookmark({ source_platform: 'x', door: 'x', author: 'davidokafor', title: 'David Okafor (@davidokafor) on X', description: 'hi' })
		);
		expect(s.authorName).toBe('David Okafor');
	});

	it('names a page by its site, with only the facts it offered', () => {
		const s = shapeOf(
			bookmark({
				title: 'Hand-cutting dovetails',
				picture_url: '/api/drive/files/f/download',
				details: { site_name: 'Northwood Shop', duration_secs: 1122, reading_minutes: 9 },
			})
		);
		expect(s.site).toBe('Northwood Shop');
		expect(s.duration).toBe('18:42');
		expect(s.facts).toEqual(['9 min read']);
		expect(openLabel(bookmark({ details: { site_name: 'Northwood Shop' } }))).toBe('Open in Northwood Shop');
	});
});

describe('labels', () => {
	it('says how it arrived and when', () => {
		expect(footerOf(bookmark({ source_platform: 'chrome', saved_at: '2026-11-21T12:00:00Z' }), NOW)).toBe('Chrome · Nov 21');
		expect(footerOf(bookmark({ door: 'hand', source_platform: 'web', saved_at: new Date(2026, 10, 24, 9).toISOString() }), NOW)).toBe(
			'Saved by hand · Today'
		);
		expect(dayLabel('2019-03-01T12:00:00Z', NOW)).toBe('Mar 1, 2019');
	});

	it('names the crumb by the headline', () => {
		expect(crumbOf(bookmark({ source_platform: 'x', door: 'x', author: 'nick', description: 'hi' }))).toBe('Post by @nick');
		expect(
			crumbOf(bookmark({ source_platform: 'x', door: 'x', author: 'nick', description: 'hi', details: { author_name: 'Nick' } }))
		).toBe('Post by Nick');
		expect(crumbOf(bookmark({ url: '/drive/file_x', bookmark_type: 'screenshot', picture_url: '/p' }))).toBe('Saved screenshot');
		expect(crumbOf(bookmark({ url: 'http://192.168.1.1' }))).toBe('192.168.1.1');
	});

	it('names where the open button goes', () => {
		expect(openLabel(bookmark({ source_platform: 'instagram', door: 'instagram' }))).toBe('Open in Instagram');
		expect(openLabel(bookmark({ url: 'http://localhost:5173/x' }))).toBe('Open localhost:5173');
		expect(openLabel(bookmark({ url: '/drive/file_x' }))).toBeNull();
		expect(openLabel(bookmark({ url: 'javascript:alert(1)' })), 'a bookmarklet is never opened').toBeNull();
	});
});

describe('groups', () => {
	it('starts weeks on Monday', () => {
		expect(groupOf(new Date(2026, 10, 23, 8).toISOString(), NOW)).toBe('This week');
		expect(groupOf(new Date(2026, 10, 22, 20).toISOString(), NOW)).toBe('Last week');
		expect(groupOf(new Date(2026, 10, 16, 8).toISOString(), NOW)).toBe('Last week');
		expect(groupOf(new Date(2026, 10, 15, 8).toISOString(), NOW)).toBe('Earlier in November');
		expect(groupOf(new Date(2026, 8, 2).toISOString(), NOW)).toBe('September');
		expect(groupOf(new Date(2024, 8, 2).toISOString(), NOW)).toBe('2024');
	});

	it('keeps consecutive saves together', () => {
		const items = [
			{ saved_at: new Date(2026, 10, 24).toISOString() },
			{ saved_at: new Date(2026, 10, 23).toISOString() },
			{ saved_at: new Date(2026, 10, 18).toISOString() },
		];
		expect(groupBySave(items, NOW).map((g) => [g.label, g.items.length])).toEqual([
			['This week', 2],
			['Last week', 1],
		]);
	});
});

describe('intoColumns', () => {
	it('places each card in the shortest column, keeping order across a row', () => {
		const cols = intoColumns([300, 100, 200, 50, 50], 3, (h) => h);
		expect(cols).toEqual([[300], [100, 50, 50], [200]]);
	});
});
