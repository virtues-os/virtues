/**
 * Wording for Changed lately on the Settings page: what happened to a
 * setting, in a few words, and when, as a day. The setting's own title comes
 * from the list (`titleOf`); this is the line beside it.
 */
import type { SettingsChange } from '$lib/api/client';
import { getThemeDisplayName, isValidTheme } from '$lib/utils/theme';

/** Values longer than this read as "Changed" rather than being quoted. */
const QUOTE_MAX = 40;

function capitalize(text: string): string {
	return text.charAt(0).toUpperCase() + text.slice(1);
}

/** The capability each tool belongs to, as Settings names it (`api/capabilities.rs`). */
function capabilityOf(tool: string): string | null {
	if (tool === 'web_search') return 'Search the web';
	if (tool === 'browse' || tool.startsWith('browser_')) return 'Use your browser';
	if (tool === 'applet') return 'Make and change sites';
	if (tool === 'create_page' || tool === 'edit_page') return 'Write in your pages';
	return null;
}

/** "Search the web turned off": the capabilities whose tools changed, from the stored switch map. */
function describeCapabilities(before: unknown, after: unknown): string {
	const was = (before ?? {}) as Record<string, unknown>;
	const now = (after ?? {}) as Record<string, unknown>;
	const lines = new Map<string, boolean>();
	for (const [tool, on] of Object.entries(now)) {
		const name = capabilityOf(tool);
		if (name && typeof on === 'boolean' && (was[tool] ?? true) !== on) lines.set(name, on);
	}
	if (lines.size === 0) return 'Changed';
	return [...lines].map(([name, on]) => `${name} turned ${on ? 'on' : 'off'}`).join(', ');
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
			return typeof value === 'string' ? `Changed to ${isValidTheme(value) ? getThemeDisplayName(value) : capitalize(value.replace(/-/g, ' '))}` : 'Changed';
		case 'birthday':
			if (typeof value === 'string' && /^\d{4}-\d{2}-\d{2}/.test(value)) {
				const day = new Date(`${value.slice(0, 10)}T00:00:00Z`);
				return `Changed to ${day.toLocaleDateString(undefined, { month: 'long', day: 'numeric', year: 'numeric', timeZone: 'UTC' })}`;
			}
			return 'Changed';
		case 'style':
		case 'subjects':
			return 'Changed';
		case 'keep-recordings':
			return typeof value === 'number' ? `Changed to ${value === 365 ? '1 year' : `${value} days`}` : 'Changed to forever';
		case 'units':
			return value === 'metric' ? 'Changed to metric units' : 'Changed to US units';
		case 'capabilities':
			return describeCapabilities(change.old_value, value);
	}

	const words = VALUE_WORDS[change.setting_id ?? '']?.[String(value)];
	if (words) return `Changed to ${words}`;
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

/** Words for the stored values of the settings the agent can draft
 *  (`api/settings_draft.rs`). */
const VALUE_WORDS: Record<string, Record<string, string>> = {
	theme: { oxford: 'Light', asgard: 'Dark', system: 'Match system' },
	glass: { none: 'None', low: 'Low', medium: 'Medium', high: 'High' },
	'text-size': { s: 'Small', m: 'Medium', l: 'Large' },
	'reading-font': { sans: 'Sans serif', serif: 'Serif', mono: 'Monospace' },
	units: { us: 'US units', metric: 'Metric units' },
};

/** A stored value as a person reads it: dollars for limits, words for a
 *  choice, On or Off for a switch, "Not set" for nothing, the text otherwise. */
export function displayValue(settingId: string, value: unknown): string {
	if (settingId === 'keep-recordings') {
		if (value === null || value === undefined) return 'Forever';
		return value === 365 ? '1 year' : `${value} days`;
	}
	if (value === null || value === undefined || value === '') return 'Not set';
	if (typeof value === 'boolean') return value ? 'On' : 'Off';
	if ((settingId === 'limit-run' || settingId === 'limit-day') && typeof value === 'number') {
		return `$${(value / 1_000_000).toFixed(2)}`;
	}
	const words = VALUE_WORDS[settingId]?.[String(value)];
	if (words) return words;
	if (settingId === 'accent' && typeof value === 'string') return capitalize(value);
	return String(value);
}

/** The rest of a change's sentence after the setting's name: "changed to
 *  US units, with Ari". */
export function changeSentence(change: SettingsChange, assistantName: string): string {
	const what = describeChange(change);
	const by = change.actor === 'assistant' ? `, with ${assistantName}` : '';
	return `${what.charAt(0).toLowerCase()}${what.slice(1)}${by}`;
}

/** When a change happened, after its sentence: "Today 2:14 PM", "Monday
 *  9:15 AM", "November 12, 9:15 AM". */
export function whenAt(iso: string, now: Date = new Date()): string {
	const day = whenLabel(iso, now);
	const time = new Date(iso).toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
	return /\d/.test(day) ? `${day}, ${time}` : `${day} ${time}`;
}

/** What a change row writes: the setting and field, plus the switches it
 *  names for a field that holds several. Matches the server's undo key. */
function changeKey(change: SettingsChange): string {
	const value = change.new_value;
	const switches = value && typeof value === 'object' ? Object.keys(value).sort().join(',') : '';
	return `${change.setting_id}|${change.field}|${switches}`;
}

/**
 * Changed lately with a run of changes to one setting on one day shown as
 * the one change they add up to: the oldest's value before, the newest's
 * after, on the newest's row (its id, time and Undo). A run that ends where
 * it started (Light, Dark, Light) changed nothing and is left out.
 * `changes` is newest first.
 */
export function mergeChanges(changes: SettingsChange[]): SettingsChange[] {
	const dayOf = (iso: string) => {
		const d = new Date(iso);
		return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
	};
	const merged: SettingsChange[] = [];
	const runs = new Map<string, { day: number; index: number; count: number }>();
	for (const change of changes) {
		if (change.kind !== 'setting') {
			merged.push(change);
			continue;
		}
		const key = changeKey(change);
		const run = runs.get(key);
		if (run && run.day === dayOf(change.occurred_at)) {
			merged[run.index] = { ...merged[run.index], old_value: change.old_value };
			run.count += 1;
			continue;
		}
		runs.set(key, { day: dayOf(change.occurred_at), index: merged.length, count: 1 });
		merged.push(change);
	}
	const same = (a: unknown, b: unknown) => JSON.stringify(a ?? null) === JSON.stringify(b ?? null);
	const unchanged = new Set(
		[...runs.values()].filter((r) => r.count > 1 && same(merged[r.index].old_value, merged[r.index].new_value)).map((r) => r.index),
	);
	return merged.filter((_, i) => !unchanged.has(i));
}

/** One piece of a drafted text against the one it replaces. */
export interface TextPart {
	text: string;
	kind: 'kept' | 'added' | 'removed';
}

/**
 * A drafted text against the old one, sentence by sentence, in reading order:
 * the sentences kept, added and taken out, so the card can strike what goes
 * and highlight what comes. `null` when the new value isn't text.
 */
export function textDiff(oldValue: unknown, newValue: unknown): TextPart[] | null {
	if (typeof newValue !== 'string') return null;
	const split = (text: string) => text.trim().split(/(?<=[.!?])\s+/).filter(Boolean);
	const was = typeof oldValue === 'string' ? split(oldValue) : [];
	const now = split(newValue);
	// The longest run of sentences the two share, then a walk through both in order.
	const shared = Array.from({ length: was.length + 1 }, () => new Array<number>(now.length + 1).fill(0));
	for (let i = was.length - 1; i >= 0; i--) {
		for (let j = now.length - 1; j >= 0; j--) {
			shared[i][j] = was[i] === now[j] ? shared[i + 1][j + 1] + 1 : Math.max(shared[i + 1][j], shared[i][j + 1]);
		}
	}
	const parts: TextPart[] = [];
	let i = 0;
	let j = 0;
	while (i < was.length || j < now.length) {
		if (i < was.length && j < now.length && was[i] === now[j]) {
			parts.push({ text: now[j], kind: 'kept' });
			i += 1;
			j += 1;
		} else if (i < was.length && (j >= now.length || shared[i + 1][j] >= shared[i][j + 1])) {
			parts.push({ text: was[i], kind: 'removed' });
			i += 1;
		} else {
			parts.push({ text: now[j], kind: 'added' });
			j += 1;
		}
	}
	return parts;
}
