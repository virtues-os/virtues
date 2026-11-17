// The list of settings: every entry is well formed, belongs to a section the
// page draws, reads correctly on its own, and is found by the words people
// type for it.
import { describe, expect, it } from 'vitest';
import { SECTIONS, SETTINGS, findSetting, matchSettings, matchingIds, settingsIn, titleOf } from './index';

describe('the list of settings', () => {
	it('gives every setting a unique id', () => {
		const ids = SETTINGS.map((s) => s.id);
		expect(new Set(ids).size).toBe(ids.length);
	});

	it('puts every setting in a section, and every section has settings', () => {
		const sections = new Set(SECTIONS.map((s) => s.id));
		for (const s of SETTINGS) expect(sections.has(s.section), s.id).toBe(true);
		for (const section of SECTIONS) expect(settingsIn(section.id).length, section.id).toBeGreaterThan(0);
	});

	it('keeps each section together, in the order the page draws them', () => {
		const order = SECTIONS.map((s) => s.id);
		const seen = SETTINGS.map((s) => order.indexOf(s.section));
		expect(seen).toEqual([...seen].sort((a, b) => a - b));
	});

	it('writes titles in sentence case without em dashes', () => {
		for (const s of SETTINGS) {
			const title = titleOf(s.title, 'Ari');
			expect(title, s.id).not.toMatch(/—/);
			expect(title.charAt(0), s.id).toBe(title.charAt(0).toUpperCase());
		}
	});

	it('lives on the Settings page, or opens a page of its own under Settings', () => {
		for (const s of SETTINGS) expect(s.route, s.id).toMatch(/^\/virtues(\/[a-z]|$)/);
		for (const s of SETTINGS.filter((s) => s.kind === 'page')) expect(s.route, s.id).not.toBe('/virtues');
		for (const s of SETTINGS.filter((s) => s.kind === 'control')) expect(s.route, s.id).toBe('/virtues');
	});

	it('marks only actions as impossible to undo', () => {
		for (const s of SETTINGS.filter((s) => s.destructive)) expect(s.kind, s.id).toBe('action');
	});
});

describe('titleOf', () => {
	it("fills in the assistant's name", () => {
		expect(titleOf('What {name} remembers', 'Ari')).toBe('What Ari remembers');
	});

	it('says "your assistant" when no name was chosen', () => {
		expect(titleOf('What {name} remembers', '')).toBe('What your assistant remembers');
		expect(titleOf('What {name} remembers', null)).toBe('What your assistant remembers');
	});

	it('capitalises the name when a title starts with it', () => {
		expect(titleOf("{name}'s name", null)).toBe("Your assistant's name");
		expect(titleOf('{name}', 'ari')).toBe('Ari');
	});
});

describe('matchSettings', () => {
	const ids = (q: string, name?: string) => matchSettings(q, name).map((s) => s.id);

	it('returns every setting, in page order, for an empty query', () => {
		expect(ids('  ')).toEqual(SETTINGS.map((s) => s.id));
	});

	it('finds a setting by a word people use for it, not only its title', () => {
		expect(ids('dark')).toContain('theme');
		expect(ids('timezone')).toContain('home');
		expect(ids('shorter')).toContain('style');
	});

	it('finds both spending limits for "spend", and the cost page', () => {
		const found = ids('spend');
		expect(found).toEqual(expect.arrayContaining(['limit-day', 'limit-run', 'ai-cost']));
	});

	it("matches on the assistant's name as written in titles", () => {
		expect(ids('ari remem', 'Ari')).toEqual(['memory']);
		expect(ids('what ari', 'Ari')).toEqual(expect.arrayContaining(['preferred-name', 'memory']));
	});

	it('ranks a title that starts with the query above a looser match', () => {
		expect(ids('theme')[0]).toBe('theme');
	});

	it('finds nothing for a query that matches nothing', () => {
		expect(ids('zzqx')).toEqual([]);
	});
});

describe('findSetting', () => {
	it('looks a setting up by id', () => {
		expect(findSetting('start-over')?.destructive).toBe(true);
		expect(findSetting('nope')).toBeUndefined();
	});
});

describe('matchingIds', () => {
	it('is null for an empty field, so everything shows', () => {
		expect(matchingIds('')).toBeNull();
		expect(matchingIds('   ')).toBeNull();
	});

	it('is the set of matching ids otherwise, empty when nothing matches', () => {
		expect(matchingIds('dark')?.has('theme')).toBe(true);
		expect(matchingIds('zzqx')?.size).toBe(0);
	});
});
