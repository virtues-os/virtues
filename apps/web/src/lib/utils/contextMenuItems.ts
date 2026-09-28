/**
 * Context Menu Item Helpers
 *
 * Provides reusable menu item generators for common actions across the app.
 * Components can compose these with their own specific items.
 */

import type { ContextMenuItem } from '$lib/stores/contextMenu.svelte';
import { projectMemberUrl, projectMenuItems } from '$lib/utils/projectActions';
import { pinMenuItem } from '$lib/pins/pinAction';

/**
 * "Add to project" for a thing with a url. Empty for anything that cannot be
 * filed: a list route, a project (you can't put a project in a project), a
 * new chat's bare `/`. The menu itself is `projectMenuItems`, shared with the
 * surfaces that file unsent chats.
 */
export function getProjectMenuItems(url: string, _name?: string | null): ContextMenuItem[] {
	const member = projectMemberUrl(url);
	return projectMenuItems(member ? { url: member } : null);
}

/**
 * The two things you can do with anything that has a url: file it, or keep it.
 *
 * "Add to project" is retrieval scope; "Pin" is navigation. They are
 * different verbs on the same object and they travel together, so every
 * surface that lists routable things can offer both with one call instead of
 * assembling the pair by hand — which is how the tab bar ended up with the
 * project submenu and no pin for months.
 */
export function getKeepMenuItems(target: {
	url: string;
	label?: string | null;
	icon?: string | null;
}): ContextMenuItem[] {
	return [
		...getProjectMenuItems(target.url, target.label),
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
