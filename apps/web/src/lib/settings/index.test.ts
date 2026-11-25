// The list of settings: every entry is well formed, belongs to a section the
// page draws, reads correctly on its own, and is found by the words people
// type for it.
import { describe, expect, it } from 'vitest';
import { LANDING, SECTIONS, SETTINGS, deviceLanding, findSetting, matchSettings, sectionAt, settingsIn, titleOf } from './index';

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

	it("lives on its section's page, or opens a page of its own under Settings", () => {
		for (const s of SETTINGS) expect(s.route, s.id).toMatch(/^\/virtues(\/[a-z]|$)/);
		for (const s of SETTINGS.filter((s) => s.kind === 'page')) expect(sectionAt(s.route), s.id).toBeUndefined();
		for (const s of SETTINGS.filter((s) => s.kind === 'control')) {
			expect(s.route, s.id).toBe(SECTIONS.find((section) => section.id === s.section)?.route);
		}
	});

	it('links every section page from the landing page', () => {
		const linked = new Set(LANDING.flatMap((g) => g.links.map((l) => l.route)));
		for (const section of SECTIONS) if (section.route) expect(linked.has(section.route), section.id).toBe(true);
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

	it('gives a name ending in s a bare apostrophe', () => {
		expect(titleOf("{name}'s name", 'Virtues')).toBe("Virtues' name");
		expect(titleOf("{name}'s name", 'Ari')).toBe("Ari's name");
	});
});

describe('matchSettings', () => {
	const ids = (q: string, name?: string) => matchSettings(q, name).map((s) => s.id);

	it('returns every setting, in page order, for an empty query', () => {
		expect(ids('  ')).toEqual(SETTINGS.filter((s) => !s.platform).map((s) => s.id));
	});

	it("lists a device's own settings only on that platform", () => {
		expect(ids('pause')).not.toContain('collecting');
		expect(matchSettings('pause', null, 'mac').map((s) => s.id)).toContain('collecting');
		expect(matchSettings('location', null, 'mac').map((s) => s.id)).not.toContain('what-it-sends');
		expect(matchSettings('location', null, 'ios').map((s) => s.id)).toContain('what-it-sends');
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
		expect(ids('ari calls', 'Ari')).toEqual(['preferred-name']);
		expect(ids('what ari', 'Ari')).toEqual(expect.arrayContaining(['preferred-name']));
	});

	it('ranks a title that starts with the query above a looser match', () => {
		expect(ids('theme')[0]).toBe('theme');
	});

	it('finds nothing for a query that matches nothing', () => {
		expect(ids('zzqx')).toEqual([]);
	});
});

describe('the landing page', () => {
	it("links Server's status page as Status", () => {
		expect(LANDING.find((g) => g.title === 'Server')?.links.find((l) => l.label === 'Status')?.route).toBe('/virtues/system');
	});

	it("links This device's parts to settings that exist on its page", () => {
		for (const here of ['mac-app', 'ios-app', 'desktop-app', 'browser'] as const) {
			const group = deviceLanding(here);
			for (const link of group.links) {
				expect(link.route, `${here} ${link.label}`).toBe('/virtues/devices/this');
				expect(findSetting(link.setting ?? '')?.route, `${here} ${link.label}`).toBe('/virtues/devices/this');
			}
		}
	});
});

describe('findSetting', () => {
	it('looks a setting up by id', () => {
		expect(findSetting('start-over')?.destructive).toBe(true);
		expect(findSetting('nope')).toBeUndefined();
	});
});
