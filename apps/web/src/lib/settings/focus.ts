/**
 * Opening one setting from somewhere else: a search result, Edit first on a
 * drafted change, a row in Changed lately. This goes to the page the setting
 * lives on; a section page then scrolls to the row and lights it once
 * (`takePendingSetting`, read when the page mounts).
 */
import { windowShellStore } from '$lib/stores/window-shell.svelte';
import { SETTINGS, findSetting, sectionAt, titleOf } from './index';

let pending: string | null = null;

/** The tab label for a Settings page: "Settings · Billing". */
export function settingsLabel(route: string, assistantName: string): string {
	const section = sectionAt(route);
	if (section) return `Settings · ${titleOf(section.title, assistantName)}`;
	const page = SETTINGS.find((s) => s.kind === 'page' && s.route === route);
	return page ? `Settings · ${titleOf(page.title, assistantName)}` : 'Settings';
}

/** Go to a Settings page, by route. */
export function openSettingsPage(route: string, assistantName: string): void {
	windowShellStore.navigate(route, { label: settingsLabel(route, assistantName) });
}

/** Go to the setting's page, and light its row there when it is a row on a section page. */
export function openSetting(id: string, assistantName: string): void {
	const setting = findSetting(id);
	if (!setting) return;
	pending = sectionAt(setting.route) ? id : null;
	openSettingsPage(setting.route, assistantName);
}

/** The row a section page should light on arrival, once. */
export function takePendingSetting(): string | null {
	const id = pending;
	pending = null;
	return id;
}
