/**
 * Context Menu Item Helpers
 *
 * Provides reusable menu item generators for common actions across the app.
 * Components can compose these with their own specific items.
 */

import type { ContextMenuItem } from '$lib/stores/contextMenu.svelte';
import { projectStore } from '$lib/stores/project.svelte';
import { pinMenuItem } from '$lib/pins/pinAction';
import { promptText } from '$lib/stores/dialog.svelte';
import { toast } from 'svelte-sonner';

/** A project's own url, in either spelling — you can't put a project in a project. */
export function isProjectUrl(url: string): boolean {
	return /^\/(?:project|notebook)\//.test(url);
}

/**
 * Get "Add to project" menu items — a submenu of all projects plus a "New project…"
 * action that creates one and adds this URL to it immediately.
 *
 * Organization moved from Things (folders) to notebooks (now projects); the menu
 * binds the item as a project member. Empty when the url is itself a project:
 * the server rejects that with 400, so the menu shouldn't offer it.
 *
 * @param url - The URL of the item (e.g., '/page/page_xyz', 'https://...')
 * @param _name - Reserved for a future display label (membership is URL-native).
 */
export function getAddToProjectMenuItems(
	url: string,
	_name?: string | null,
): ContextMenuItem[] {
	if (isProjectUrl(url)) return [];
	const projects = projectStore.projects;

	const submenu: ContextMenuItem[] = projects.map((s) => ({
		id: `project-${s.id}`,
		label: s.name,
		icon: s.icon || 'ri:folder-open-line',
		action: async () => {
			try {
				await projectStore.addItem(s.id, url);
				toast(`Added to ${s.name}`);
			} catch (e) {
				console.error('[contextMenuItems] Failed to add to project:', e);
				toast.error('Failed to add to project');
			}
		},
	}));

	submenu.push({
		id: 'new-project-with-item',
		label: projects.length > 0 ? 'New project…' : 'Create first project…',
		icon: 'ri:add-line',
		dividerBefore: projects.length > 0,
		action: async () => {
			// promptText, not window.prompt() — the latter is a no-op in the
			// Tauri/WKWebView shell, so this menu item did nothing there.
			const projectName = await promptText({
				title: 'New project',
				placeholder: 'Name your project',
				confirmLabel: 'Create',
			});
			if (!projectName) return;
			try {
				const project = await projectStore.create(projectName);
				await projectStore.addItem(project.id, url);
				toast(`Created "${project.name}" and added item`);
			} catch (e) {
				console.error('[contextMenuItems] Failed to create project:', e);
				toast.error('Failed to create project');
			}
		},
	});

	return [
		{
			id: 'add-to-project',
			label: 'Add to project',
			icon: 'ri:folder-add-line',
			dividerBefore: true,
			submenu,
		},
	];
}

/**
 * Get organization-related menu items (Add to project).
 * Used by tab/sidebar/page context menus.
 */
export function getProjectMenuItems(
	url: string,
	name?: string | null,
): ContextMenuItem[] {
	return getAddToProjectMenuItems(url, name);
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
