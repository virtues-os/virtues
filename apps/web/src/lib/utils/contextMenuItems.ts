/**
 * Context Menu Item Helpers
 *
 * Provides reusable menu item generators for common actions across the app.
 * Components can compose these with their own specific items.
 */

import type { ContextMenuItem } from '$lib/stores/contextMenu.svelte';
import { spaceStore } from '$lib/stores/space.svelte';
import { toast } from 'svelte-sonner';

/**
 * Get "Add to Space" menu items
 * Works for any URL - the sidebar is URL-based per DND_UX_SPEC.md
 * Shows all user spaces (excludes current space and system space)
 * @param url - The URL of the item (e.g., '/page/page_xyz', '/chat/chat_abc')
 */
export function getMoveToWorkspaceMenuItems(url: string): ContextMenuItem[] {
	// Get all user spaces (exclude current and system space)
	const currentSpaceId = spaceStore.activeSpaceId;
	const userSpaces = spaceStore.spaces.filter(
		(ws) => ws.id !== currentSpaceId && !ws.is_system
	);

	if (userSpaces.length === 0) {
		return [];
	}

	return [
		{
			id: 'add-to-space',
			label: 'Add to Space',
			icon: 'ri:add-line',
			dividerBefore: true,
			submenu: userSpaces.map((ws) => ({
				id: `space-${ws.id}`,
				label: ws.name,
				icon: ws.icon || 'ri:folder-line',
				action: async () => {
					try {
						await spaceStore.addSpaceItem(url, ws.id);
						toast(`Added to ${ws.name}`);
					} catch (e) {
						console.error('[contextMenuItems] Failed to add to space:', e);
						toast.error('Failed to add to space');
					}
				}
			}))
		}
	];
}

/**
 * Get all space-related menu items (Add to Space only - use drag-drop for folders)
 * Used by tab context menus
 * @param url - The URL of the item (e.g., '/page/page_xyz')
 */
export function getWorkspaceMenuItems(url: string): ContextMenuItem[] {
	// "Add to Folder" removed - use drag-drop instead for clearer UX
	return [
		...getMoveToWorkspaceMenuItems(url)
	];
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
