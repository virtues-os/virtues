/**
 * What the Settings page's search field is showing, shared with every row
 * on the page through context. The page owns the query; a row asks
 * `shows(id)` and hides itself when the answer is no, so a row drawn inside
 * a section component (YouSettings, BillingSettings) filters the same way as
 * one drawn by the page.
 */
import { getContext, setContext } from 'svelte';

export interface SettingsFilter {
	/** Whether the row for this setting id shows for the current query. */
	shows(id: string): boolean;
}

const KEY = Symbol('settings-filter');

/** Everything shows: a row outside the Settings page has no filter. */
const SHOW_ALL: SettingsFilter = { shows: () => true };

export function provideSettingsFilter(filter: SettingsFilter): void {
	setContext(KEY, filter);
}

export function settingsFilter(): SettingsFilter {
	return getContext<SettingsFilter | undefined>(KEY) ?? SHOW_ALL;
}
