/**
 * Opening one setting from somewhere else: a search result, Edit first on a
 * drafted change, a row in Changed lately. This goes to the page the setting
 * lives on; a section page then scrolls to the row and lights it once
 * (`takePendingSetting`, read when the page mounts).
 */
import { windowShellStore } from '$lib/stores/window-shell.svelte';
import { SETTINGS, findSetting, sectionAt, titleOf } from './index';

// The row to light, and when it was asked for: a page opened in a tab that
// was already there does not mount again, so an old request must not light a
// row the next time some section page mounts.
let pending: { id: string; at: number } | null = null;
const PENDING_MS = 3000;

/** The tab label for a Settings page: "Settings · Billing". */
export function settingsLabel(route: string, assistantName: string): string {
	// A query (the model list's `?role=`) picks within a page; the page is the path.
	route = route.split('?')[0];
	const section = sectionAt(route);
	if (section) return `Settings · ${titleOf(section.title, assistantName)}`;
	const page = SETTINGS.find((s) => s.kind === 'page' && s.route === route);
	return page ? `Settings · ${titleOf(page.title, assistantName)}` : 'Settings';
}

/** A section page finds the row on arrival. A page of its own holds no rows to find. */
function litOnArrival(route: string): boolean {
	return !!sectionAt(route);
}

/** Go to a Settings page, by route. */
export function openSettingsPage(route: string, assistantName: string): void {
	windowShellStore.navigate(route, { label: settingsLabel(route, assistantName) });
}

/** Go to the setting's page, and light its row there when it is a row on a section page. */
export function openSetting(id: string, assistantName: string): void {
	const setting = findSetting(id);
	if (!setting) return;
	pending = litOnArrival(setting.route) ? { id, at: Date.now() } : null;
	openSettingsPage(setting.route, assistantName);
}

/** The same, in a tab of its own (from ⌘K, which opens over whatever you are in). */
export function openSettingInTab(id: string, assistantName: string): void {
	const setting = findSetting(id);
	if (!setting) return;
	pending = litOnArrival(setting.route) ? { id, at: Date.now() } : null;
	void windowShellStore.openTabFromRoute(setting.route, { label: settingsLabel(setting.route, assistantName), focusExisting: true });
}

/** The row a section page should light on arrival, once. */
export function takePendingSetting(): string | null {
	const asked = pending;
	pending = null;
	return asked && Date.now() - asked.at < PENDING_MS ? asked.id : null;
}
