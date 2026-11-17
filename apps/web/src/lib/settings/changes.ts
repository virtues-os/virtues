/**
 * Wording for Changed lately on the Settings page: what happened to a
 * setting, in a few words, and when, as a day. The setting's own title comes
 * from the list (`titleOf`); this is the line beside it.
 */
import type { SettingsChange } from '$lib/api/client';

/** Values longer than this read as "Changed" rather than being quoted. */
const QUOTE_MAX = 40;

function capitalize(text: string): string {
	return text.charAt(0).toUpperCase() + text.slice(1);
}

/** What happened, in a few words: "Turned off", "Changed to $5.00". */
export function describeChange(change: SettingsChange): string {
	if (change.kind === 'paired') return 'Paired';

	const value = change.new_value;
	if (typeof value === 'boolean') return value ? 'Turned on' : 'Turned off';

	switch (change.setting_id) {
		case 'update-channel':
			return value === 'prerelease' ? 'Changed to early updates' : 'Changed to released updates';
		case 'limit-run':
		case 'limit-day':
			return typeof value === 'number' ? `Changed to $${(value / 1_000_000).toFixed(2)}` : 'Changed';
		case 'models':
			return value === null ? 'Back to the recommended model' : 'Changed';
		case 'theme':
			return typeof value === 'string' ? `Changed to ${capitalize(value.replace(/-/g, ' '))}` : 'Changed';
		case 'birthday':
			if (typeof value === 'string' && /^\d{4}-\d{2}-\d{2}/.test(value)) {
				const day = new Date(`${value.slice(0, 10)}T00:00:00Z`);
				return `Changed to ${day.toLocaleDateString(undefined, { month: 'long', day: 'numeric', year: 'numeric', timeZone: 'UTC' })}`;
			}
			return 'Changed';
		case 'style':
		case 'subjects':
			return 'Changed';
	}

	if (value === null || value === '') return 'Cleared';
	if (typeof value === 'string' && value.length <= QUOTE_MAX) return `Changed to ${value}`;
	if (typeof value === 'number') return `Changed to ${value}`;
	return 'Changed';
}

/** The day a change happened: "Today", "Yesterday", a weekday within the
 *  week, then a date. */
export function whenLabel(iso: string, now: Date = new Date()): string {
	const then = new Date(iso);
	const startOf = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
	const days = Math.round((startOf(now) - startOf(then)) / 86_400_000);
	if (days <= 0) return 'Today';
	if (days === 1) return 'Yesterday';
	if (days < 7) return then.toLocaleDateString(undefined, { weekday: 'long' });
	return then.toLocaleDateString(undefined, {
		month: 'long',
		day: 'numeric',
		...(then.getFullYear() === now.getFullYear() ? {} : { year: 'numeric' }),
	});
}

/** The row number in a setting change's id (`setting:12` → 12). */
export function changeRowId(change: SettingsChange): number | null {
	const match = /^setting:(\d+)$/.exec(change.id);
	return match ? Number(match[1]) : null;
}

/** A stored value as a person reads it: dollars for limits, "Not set" for
 *  nothing, the text otherwise. */
export function displayValue(settingId: string, value: unknown): string {
	if (value === null || value === undefined || value === '') return 'Not set';
	if ((settingId === 'limit-run' || settingId === 'limit-day') && typeof value === 'number') {
		return `$${(value / 1_000_000).toFixed(2)}`;
	}
	return String(value);
}

/** When a drafted text keeps the old one and adds to it, the two parts, so
 *  the addition can be highlighted. `null` when it rewrites instead. */
export function addedText(oldValue: unknown, newValue: unknown): { kept: string; added: string } | null {
	if (typeof oldValue !== 'string' || typeof newValue !== 'string') return null;
	const kept = oldValue.trim();
	if (!kept || !newValue.startsWith(kept) || newValue.length === kept.length) return null;
	return { kept, added: newValue.slice(kept.length) };
}
