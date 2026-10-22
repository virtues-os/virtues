/**
 * The two ways to open a thing somewhere other than the tab you are on, as
 * menu items: a new tab next to this one, or the pane beside. A plain click
 * opens it here and ⌘-click opens a new tab (window-shell `openInNewTab`);
 * every right-click menu on something openable offers both of these.
 */
import type { ContextMenuItem } from '$lib/stores/contextMenu.svelte';
import { windowShellStore } from '$lib/stores/window-shell.svelte';
import { shortcuts } from '$lib/shortcuts/registry.svelte';

export function openMenuItems(url: string, label: string): ContextMenuItem[] {
	return [
		{
			id: 'open-new-tab',
			label: 'Open in new tab',
			icon: 'ri:add-box-line',
			shortcut: `${shortcuts.format('mod')} click`,
			action: () => void windowShellStore.openInNewTab(url, label),
		},
		{
			id: 'open-beside',
			label: 'Open beside',
			icon: 'ri:layout-column-line',
			action: () => void windowShellStore.openRouteBeside(url, label),
		},
	];
}
