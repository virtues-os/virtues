// Settings in search: the #settings scope lists them, and the page each lives
// on is named the way Settings names it.
import { describe, expect, it } from 'vitest';
import { SETTINGS, findSetting, settingPlace } from '$lib/settings';
import { parseQuery, rowNoun, searchGroups } from './results';

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
		expect(settingPlace(findSetting('limit-day')!, 'Iris')).toBe('Billing');
		expect(settingPlace(findSetting('style')!, 'Iris')).toBe('Iris');
		expect(settingPlace(findSetting('occupation')!, null)).toBe('Profile');
		expect(settingPlace(findSetting('devices')!, 'Iris')).toBe('Your server');
		expect(settingPlace(findSetting('memory')!, 'Iris')).toBe('Your data');
		expect(settingPlace(findSetting('ai-cost')!, 'Iris')).toBe('Billing');
		expect(settingPlace(findSetting('start-over')!, 'Iris')).toBe('Settings');
	});
});
