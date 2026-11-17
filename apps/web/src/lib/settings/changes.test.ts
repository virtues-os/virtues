// Wording for Changed lately: each kind of value reads as a short line, and
// days read as a person says them.
import { describe, expect, it } from 'vitest';
import type { SettingsChange } from '$lib/api/client';
import { changeRowId, describeChange, whenLabel } from './changes';

function change(setting_id: string, new_value: unknown, extra: Partial<SettingsChange> = {}): SettingsChange {
	return {
		id: 'setting:7',
		kind: 'setting',
		setting_id,
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

	it('reads a theme id as a name', () => {
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

describe('changeRowId', () => {
	it('reads the row number of a setting change, and nothing for a pairing', () => {
		expect(changeRowId(change('theme', 'oxford'))).toBe(7);
		expect(changeRowId(change('', null, { kind: 'paired', id: 'paired:3' }))).toBeNull();
	});
});
