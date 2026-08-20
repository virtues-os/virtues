/**
 * Context Menu Item Helpers
 *
 * Provides reusable menu item generators for common actions across the app.
 * Components can compose these with their own specific items.
 */

import type { ContextMenuItem } from '$lib/stores/contextMenu.svelte';
import { notebookStore } from '$lib/stores/notebook.svelte';
import { pinMenuItem } from '$lib/pins/pinAction';
import { promptText } from '$lib/stores/dialog.svelte';
import { toast } from 'svelte-sonner';

/**
 * Get "Add to Notebook" menu items — a submenu of all notebooks plus a "New Notebook…"
 * action that creates one and adds this URL to it immediately.
 *
 * Organization moved from Things (folders) to Notebooks; the menu now binds the
 * item as a Notebook member.
 *
 * @param url - The URL of the item (e.g., '/page/page_xyz', 'https://...')
 * @param _name - Reserved for a future display label (membership is URL-native).
 */
export function getAddToNotebookMenuItems(
	url: string,
	_name?: string | null,
): ContextMenuItem[] {
	const notebooks = notebookStore.notebooks;

	const submenu: ContextMenuItem[] = notebooks.map((s) => ({
		id: `notebook-${s.id}`,
		label: s.name,
		icon: s.icon || 'ri:folder-open-line',
		action: async () => {
			try {
				await notebookStore.addItem(s.id, url);
				toast(`Added to ${s.name}`);
			} catch (e) {
				console.error('[contextMenuItems] Failed to add to notebook:', e);
				toast.error('Failed to add to notebook');
			}
		},
	}));

	submenu.push({
		id: 'new-notebook-with-item',
		label: notebooks.length > 0 ? 'New Notebook…' : 'Create First Notebook…',
		icon: 'ri:add-line',
		dividerBefore: notebooks.length > 0,
		action: async () => {
			// promptText, not window.prompt() — the latter is a no-op in the
			// Tauri/WKWebView shell, so this menu item did nothing there.
			const notebookName = await promptText({
				title: 'New notebook',
				placeholder: 'Name your notebook',
				confirmLabel: 'Create',
			});
			if (!notebookName) return;
			try {
				const notebook = await notebookStore.create(notebookName);
				await notebookStore.addItem(notebook.id, url);
				toast(`Created "${notebook.name}" and added item`);
			} catch (e) {
				console.error('[contextMenuItems] Failed to create notebook:', e);
				toast.error('Failed to create notebook');
			}
		},
	});

	return [
		{
			id: 'add-to-notebook',
			label: 'Add to Notebook',
			icon: 'ri:folder-add-line',
			dividerBefore: true,
			submenu,
		},
	];
}

/**
 * Get organization-related menu items (Add to Notebook).
 * Used by tab/sidebar/page context menus.
 */
export function getNotebookMenuItems(
	url: string,
	name?: string | null,
): ContextMenuItem[] {
	return getAddToNotebookMenuItems(url, name);
}

/**
 * The two things you can do with anything that has a url: file it, or keep it.
 *
 * "Add to notebook" is retrieval scope; "Add to desk" is navigation. They are
 * different verbs on the same object and they travel together, so every
 * surface that lists routable things can offer both with one call instead of
 * assembling the pair by hand — which is how the tab bar ended up with the
 * notebook submenu and no pin for months.
 */
export function getKeepMenuItems(target: {
	url: string;
	label?: string | null;
	icon?: string | null;
}): ContextMenuItem[] {
	return [
		...getNotebookMenuItems(target.url, target.label),
		pinMenuItem(target),
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
