/**
 * Context Menu Item Helpers
 *
 * Provides reusable menu item generators for common actions across the app.
 * Components can compose these with their own specific items.
 */

import type { ContextMenuItem } from '$lib/stores/contextMenu.svelte';
import { thingsStore } from '$lib/stores/things.svelte';
import { toast } from 'svelte-sonner';

/**
 * Get "Add to Thing" menu items — a submenu of all things plus a
 * "New Thing…" action that creates one and pins this URL immediately.
 * @param url - The URL of the item (e.g., '/page/page_xyz', 'https://...')
 * @param name - Optional display name for the item
 */
export function getAddToThingMenuItems(
	url: string,
	name?: string | null,
): ContextMenuItem[] {
	const things = thingsStore.things;

	const submenu: ContextMenuItem[] = things.map((t) => ({
		id: `thing-${t.id}`,
		label: t.name,
		icon: t.icon || 'ri:folder-open-line',
		action: async () => {
			try {
				await thingsStore.addPin(t.id, url, { name });
				toast(`Added to ${t.name}`);
			} catch (e) {
				console.error('[contextMenuItems] Failed to add to thing:', e);
				toast.error('Failed to add to thing');
			}
		},
	}));

	submenu.push({
		id: 'new-thing-with-item',
		label: things.length > 0 ? 'New Thing…' : 'Create First Thing…',
		icon: 'ri:add-line',
		dividerBefore: things.length > 0,
		action: async () => {
			const thingName = prompt('Thing name:');
			if (!thingName || !thingName.trim()) return;
			try {
				const thing = await thingsStore.create(thingName.trim());
				await thingsStore.addPin(thing.id, url, { name });
				toast(`Created "${thing.name}" and added item`);
			} catch (e) {
				console.error('[contextMenuItems] Failed to create thing:', e);
				toast.error('Failed to create thing');
			}
		},
	});

	return [
		{
			id: 'add-to-thing',
			label: 'Add to Thing',
			icon: 'ri:folder-add-line',
			dividerBefore: true,
			submenu,
		},
	];
}

/**
 * Get organization-related menu items (Add to Thing).
 * Used by tab context menus. "Move to Workspace" removed — single workspace.
 */
export function getWorkspaceMenuItems(
	url: string,
	name?: string | null,
): ContextMenuItem[] {
	return getAddToThingMenuItems(url, name);
}

/**
 * Get tab management menu items
 */
export function getTabMenuItems(options: {
	onClose?: () => void;
	onCloseOthers?: () => void;
	onCloseToRight?: () => void;
	onPin?: () => void;
	onDuplicate?: () => void;
	onOpenInSplit?: () => void;
	isPinned?: boolean;
	canCloseOthers?: boolean;
	canCloseToRight?: boolean;
}): ContextMenuItem[] {
	const items: ContextMenuItem[] = [];

	if (options.onClose) {
		items.push({
			id: 'close-tab',
			label: 'Close',
			icon: 'ri:close-line',
			shortcut: '⌘W',
			action: options.onClose
		});
	}

	if (options.onCloseOthers && options.canCloseOthers) {
		items.push({
			id: 'close-others',
			label: 'Close Others',
			action: options.onCloseOthers
		});
	}

	if (options.onCloseToRight && options.canCloseToRight) {
		items.push({
			id: 'close-to-right',
			label: 'Close to Right',
			action: options.onCloseToRight
		});
	}

	if (options.onPin) {
		items.push({
			id: 'pin-tab',
			label: options.isPinned ? 'Unpin' : 'Pin',
			icon: options.isPinned ? 'ri:pushpin-fill' : 'ri:pushpin-line',
			dividerBefore: true,
			action: options.onPin
		});
	}

	if (options.onDuplicate) {
		items.push({
			id: 'duplicate-tab',
			label: 'Duplicate',
			icon: 'ri:file-copy-line',
			action: options.onDuplicate
		});
	}

	if (options.onOpenInSplit) {
		items.push({
			id: 'open-in-split',
			label: 'Open in Split Pane',
			icon: 'ri:layout-column-line',
			action: options.onOpenInSplit
		});
	}

	return items;
}

/**
 * Get link/navigation menu items
 */
export function getLinkMenuItems(options: {
	href: string;
	onOpenInNewTab?: () => void;
	onOpenInSplit?: () => void;
	onCopyLink?: () => void;
}): ContextMenuItem[] {
	const items: ContextMenuItem[] = [];

	if (options.onOpenInNewTab) {
		items.push({
			id: 'open-new-tab',
			label: 'Open in New Tab',
			icon: 'ri:external-link-line',
			action: options.onOpenInNewTab
		});
	}

	if (options.onOpenInSplit) {
		items.push({
			id: 'open-in-split',
			label: 'Open in Split Pane',
			icon: 'ri:layout-column-line',
			action: options.onOpenInSplit
		});
	}

	if (options.onCopyLink) {
		items.push({
			id: 'copy-link',
			label: 'Copy Link',
			icon: 'ri:link',
			dividerBefore: true,
			action: options.onCopyLink
		});
	}

	return items;
}

/**
 * Get destructive action menu items (delete, remove, etc.)
 */
export function getDestructiveMenuItems(options: {
	onDelete?: () => void;
	onRemove?: () => void;
	deleteLabel?: string;
	removeLabel?: string;
}): ContextMenuItem[] {
	const items: ContextMenuItem[] = [];

	if (options.onRemove) {
		items.push({
			id: 'remove',
			label: options.removeLabel || 'Remove',
			icon: 'ri:close-line',
			variant: 'destructive',
			dividerBefore: true,
			action: options.onRemove
		});
	}

	if (options.onDelete) {
		items.push({
			id: 'delete',
			label: options.deleteLabel || 'Delete',
			icon: 'ri:delete-bin-line',
			variant: 'destructive',
			dividerBefore: !options.onRemove,
			action: options.onDelete
		});
	}

	return items;
}
