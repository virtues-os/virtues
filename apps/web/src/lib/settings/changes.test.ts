// Wording for Changed lately: each kind of value reads as a short line, and
// days read as a person says them.
import { describe, expect, it } from 'vitest';
import type { SettingsChange } from '$lib/api/client';
import { changeRowId, describeChange, displayValue, mergeChanges, textDiff, whenLabel } from './changes';

function change(setting_id: string, new_value: unknown, extra: Partial<SettingsChange> = {}): SettingsChange {
	return {
		id: 'setting:7',
		kind: 'setting',
		setting_id,
		field: null,
		old_value: null,
		new_value,
		label: null,
		actor: 'owner',
		occurred_at: '2026-03-10T09:00:00Z',
		can_undo: true,
		...extra,
	};
}

describe('describeChange', () => {
	it('says a switch turned on or off', () => {
		expect(describeChange(change('auto-top-up', false))).toBe('Turned off');
		expect(describeChange(change('reach', true))).toBe('Turned on');
	});

	it('writes spending limits as dollars', () => {
		expect(describeChange(change('limit-day', 5_000_000))).toBe('Changed to $5.00');
	});

	it('names the update channel in words', () => {
		expect(describeChange(change('update-channel', 'prerelease'))).toBe('Changed to early updates');
		expect(describeChange(change('update-channel', 'stable'))).toBe('Changed to released updates');
	});

	it('names a theme choice in words', () => {
		expect(describeChange(change('theme', 'oxford'))).toBe('Changed to Light');
		expect(describeChange(change('theme', 'system'))).toBe('Changed to Match system');
		// A name recorded before the themes were cut still reads as a name.
		expect(describeChange(change('theme', 'baker-street'))).toBe('Changed to Baker street');
	});

	it('does not quote long text or model ids', () => {
		expect(describeChange(change('style', 'Short answers unless I ask for more.'))).toBe('Changed');
		expect(describeChange(change('models', 'provider/some-model'))).toBe('Changed');
		expect(describeChange(change('models', null))).toBe('Back to the recommended model');
	});

	it('quotes a short value and says when one was cleared', () => {
		expect(describeChange(change('preferred-name', 'Nick'))).toBe('Changed to Nick');
		expect(describeChange(change('employer', null))).toBe('Cleared');
	});

	it('names an appearance choice in words', () => {
		expect(describeChange(change('reading-font', 'sans'))).toBe('Changed to Sans serif');
		expect(describeChange(change('glass', 'none'))).toBe('Changed to None');
	});

	it('names the units', () => {
		expect(describeChange(change('units', 'metric'))).toBe('Changed to metric units');
		expect(describeChange(change('units', 'us'))).toBe('Changed to US units');
	});

	it('names the capability that was switched', () => {
		const off = change('capabilities', { web_search: false, create_page: true }, { old_value: { web_search: true, create_page: true } });
		expect(describeChange(off)).toBe('Search the web turned off');
		const pages = change('capabilities', { create_page: true, edit_page: true }, { old_value: { create_page: false, edit_page: false } });
		expect(describeChange(pages)).toBe('Write in your pages turned on');
	});

	it('says a device was paired', () => {
		expect(describeChange(change('', null, { kind: 'paired', id: 'paired:3', label: 'Kitchen speaker' }))).toBe('Paired');
	});
});

describe('whenLabel', () => {
	const now = new Date(2026, 2, 12, 15, 0); // a Thursday afternoon, local time

	it('says Today and Yesterday', () => {
		expect(whenLabel(new Date(2026, 2, 12, 8, 0).toISOString(), now)).toBe('Today');
		expect(whenLabel(new Date(2026, 2, 11, 23, 0).toISOString(), now)).toBe('Yesterday');
	});

	it('names the weekday within the week, then the date', () => {
		const monday = new Date(2026, 2, 9, 10, 0);
		expect(whenLabel(monday.toISOString(), now)).toBe(monday.toLocaleDateString(undefined, { weekday: 'long' }));
		const earlier = new Date(2026, 1, 20, 10, 0);
		expect(whenLabel(earlier.toISOString(), now)).toBe(earlier.toLocaleDateString(undefined, { month: 'long', day: 'numeric' }));
	});

	it('adds the year for an earlier year', () => {
		const lastYear = new Date(2025, 11, 30, 10, 0);
		expect(whenLabel(lastYear.toISOString(), now)).toContain('2025');
	});
});

describe('displayValue', () => {
	it('shows limits as dollars and nothing as Not set', () => {
		expect(displayValue('limit-day', 3_000_000)).toBe('$3.00');
		expect(displayValue('employer', null)).toBe('Not set');
		expect(displayValue('preferred-name', 'Nick')).toBe('Nick');
	});

	it('names choices and switches in words', () => {
		expect(displayValue('theme', 'asgard')).toBe('Dark');
		expect(displayValue('text-size', 'l')).toBe('Large');
		expect(displayValue('reduce-motion', true)).toBe('On');
		expect(displayValue('keep-recordings', null)).toBe('Forever');
		expect(displayValue('keep-recordings', 30)).toBe('30 days');
		expect(displayValue('accent', 'teal')).toBe('Teal');
	});
});

describe('textDiff', () => {
	it('marks what a draft added to a note', () => {
		expect(textDiff('No lists.', 'No lists. Keep answers short.')).toEqual([
			{ text: 'No lists.', kind: 'kept' },
			{ text: 'Keep answers short.', kind: 'added' },
		]);
	});

	it('marks what a rewrite takes out, in reading order', () => {
		expect(textDiff('Use British spelling. Keep answers short.', 'Keep answers short. Write in paragraphs.')).toEqual([
			{ text: 'Use British spelling.', kind: 'removed' },
			{ text: 'Keep answers short.', kind: 'kept' },
			{ text: 'Write in paragraphs.', kind: 'added' },
		]);
	});

	it('reads a first note as all added, and is null for a value that is not text', () => {
		expect(textDiff(null, 'Short answers.')).toEqual([{ text: 'Short answers.', kind: 'added' }]);
		expect(textDiff('x', 5)).toBeNull();
	});
});

describe('mergeChanges', () => {
	const at = (hour: number) => new Date(2026, 2, 12, hour, 0).toISOString();

	it('shows a run of changes to one setting on one day as the change they add up to', () => {
		const merged = mergeChanges([
			change('style', 'C', { id: 'setting:3', field: 'style_notes', old_value: 'B', occurred_at: at(11) }),
			change('style', 'B', { id: 'setting:2', field: 'style_notes', old_value: 'A', occurred_at: at(10) }),
		]);
		expect(merged).toHaveLength(1);
		expect(merged[0]).toMatchObject({ id: 'setting:3', old_value: 'A', new_value: 'C' });
	});

	it('leaves out a run that ends where it started, and keeps other settings', () => {
		const merged = mergeChanges([
			change('theme', 'oxford', { id: 'setting:4', field: 'ui_preferences.theme', old_value: 'asgard', occurred_at: at(12) }),
			change('units', 'metric', { id: 'setting:9', field: 'units', old_value: 'us', occurred_at: at(11) }),
			change('theme', 'asgard', { id: 'setting:3', field: 'ui_preferences.theme', old_value: 'oxford', occurred_at: at(10) }),
		]);
		expect(merged.map((c) => c.id)).toEqual(['setting:9']);
	});

	it('keeps changes to different switches of one setting apart', () => {
		const merged = mergeChanges([
			change('capabilities', { web_search: false }, { id: 'setting:6', field: 'enabled_tools', occurred_at: at(11) }),
			change('capabilities', { applet: false }, { id: 'setting:5', field: 'enabled_tools', occurred_at: at(10) }),
		]);
		expect(merged).toHaveLength(2);
	});
});

describe('changeRowId', () => {
	it('reads the row number of a setting change, and nothing for a pairing', () => {
		expect(changeRowId(change('theme', 'oxford'))).toBe(7);
		expect(changeRowId(change('', null, { kind: 'paired', id: 'paired:3' }))).toBeNull();
	});
});
