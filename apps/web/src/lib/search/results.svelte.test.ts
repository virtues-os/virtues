// Settings in search: the #settings scope lists them, and the page each lives
// on is named the way Settings names it.
import { describe, expect, it } from 'vitest';
import { SETTINGS, findSetting, settingPlace } from '$lib/settings';
import { parseQuery, rowNoun, searchGroups, type BookmarkHit } from './results';

const scoped = (query: string) => searchGroups({ query, here: null, commands: [], content: [], assistantName: 'Iris' });

describe('#settings', () => {
	it('is a scope, singular or plural', () => {
		expect(parseQuery('#settings spend').scope).toBe('setting');
		expect(parseQuery('#setting').scope).toBe('setting');
		expect(parseQuery('#settings spend').text).toBe('spend');
	});

	it('lists every setting when nothing follows it', () => {
		const [group] = scoped('#settings');
		expect(group.label).toBe('Settings');
		expect(group.rows.length).toBe(SETTINGS.length);
		expect(group.rows.every((r) => r.kind === 'setting')).toBe(true);
	});

	it('finds settings by the words people use, best first', () => {
		const [group] = scoped('#settings spend');
		const ids = group.rows.map((r) => (r.kind === 'setting' ? r.setting.id : ''));
		expect(ids).toContain('limit-day');
		expect(ids).toContain('limit-run');
		expect(ids).not.toContain('theme');
	});

	it('calls each row a Setting', () => {
		const [group] = scoped('#settings dark');
		expect(rowNoun(group.rows[0])).toBe('Setting');
	});
});

describe('settingPlace', () => {
	it("names the page, with the assistant's name where Settings uses it", () => {
		expect(settingPlace(findSetting('limit-day')!, 'Iris')).toBe('Models and spending');
		expect(settingPlace(findSetting('style')!, 'Iris')).toBe('How Iris talks to you');
		expect(settingPlace(findSetting('occupation')!, null)).toBe('Profile');
		expect(settingPlace(findSetting('search-runs-on')!, 'Iris')).toBe('Your server');
		expect(settingPlace(findSetting('byo-ai')!, 'Iris')).toBe('Models and spending');
		expect(settingPlace(findSetting('ai-cost')!, 'Iris')).toBe('Models and spending');
		expect(settingPlace(findSetting('start-over')!, 'Iris')).toBe('Devices');
	});
});

describe('bookmarks', () => {
	const bookmark: BookmarkHit = {
		id: 'bm_1',
		url: 'https://www.example.com/sourdough',
		title: 'Sourdough starter guide',
		tags: ['baking', 'bread'],
		description: null,
		note: null,
		source_platform: 'safari',
		bookmark_type: 'bookmark',
		author: null,
		picture_url: null,
		door: 'browser',
		details: {},
		timestamp: '2026-11-20T12:00:00Z',
		saved_at: '2026-11-20T12:00:00Z',
		medium: null,
		line: { title: 'Sourdough starter guide', meta: 'example.com' },
		crumb: 'Sourdough starter guide',
		summary: null,
	};
	const groups = (query: string) =>
		searchGroups({ query, here: null, commands: [], content: [], bookmarks: [bookmark], assistantName: 'Iris' });

	it('is a scope, by #bookmarks or #tags', () => {
		expect(parseQuery('#bookmarks bread').scope).toBe('bookmark');
		expect(parseQuery('#tags baking').scope).toBe('bookmark');
	});

	it('lists what the box matched under Bookmarks, as a Bookmark', () => {
		const group = groups('baking').find((g) => g.label === 'Bookmarks');
		expect(group?.rows).toEqual([{ kind: 'bookmark', bookmark }]);
		expect(rowNoun(group!.rows[0])).toBe('Bookmark');
		expect(groups('#tags baking')).toEqual([{ label: 'Bookmarks', rows: [{ kind: 'bookmark', bookmark }] }]);
	});

	it('does not list a bookmark again for its text', () => {
		const hit = { ontology: 'content_bookmark', record_id: 'bm_1', title: 'Sourdough starter guide', preview: 'feed it daily' };
		const all = searchGroups({ query: 'sourdough', here: null, commands: [], content: [hit as never], bookmarks: [bookmark], assistantName: 'Iris' });
		expect(all.some((g) => g.label === 'In your records')).toBe(false);
	});
});
