/**
 * Context Menu Item Helpers
 *
 * Provides reusable menu item generators for common actions across the app.
 * Components can compose these with their own specific items.
 */

import type { ContextMenuItem } from '$lib/stores/contextMenu.svelte';
import { projectsStore } from '$lib/stores/projects.svelte';
import { toast } from 'svelte-sonner';

/**
 * Get "Add to Project" menu items. Shows all projects as a submenu; if no
 * projects exist yet, offers "New Project…" that creates one and adds the
 * URL to it immediately.
 * @param url - The URL of the item (e.g., '/page/page_xyz', 'https://...')
 * @param name - Optional display name for the item
 */
export function getAddToProjectMenuItems(
	url: string,
	name?: string | null,
): ContextMenuItem[] {
	const projects = projectsStore.projects;

	const submenu: ContextMenuItem[] = projects.map((p) => ({
		id: `project-${p.id}`,
		label: p.name,
		icon: p.icon || 'ri:folder-open-line',
		action: async () => {
			try {
				await projectsStore.addItem(p.id, url, { name });
				toast(`Added to ${p.name}`);
			} catch (e) {
				console.error('[contextMenuItems] Failed to add to project:', e);
				toast.error('Failed to add to project');
			}
		},
	}));

	submenu.push({
		id: 'new-project-with-item',
		label: projects.length > 0 ? 'New Project…' : 'Create First Project…',
		icon: 'ri:add-line',
		dividerBefore: projects.length > 0,
		action: async () => {
			const projectName = prompt('Project name:');
			if (!projectName || !projectName.trim()) return;
			try {
				const project = await projectsStore.create(projectName.trim());
				await projectsStore.addItem(project.id, url, { name });
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
			label: 'Add to Project',
			icon: 'ri:folder-add-line',
			dividerBefore: true,
			submenu,
		},
	];
}

/**
 * Get organization-related menu items (Add to Project).
 * Used by tab context menus. "Move to Workspace" removed — single workspace.
 * @param url - The URL of the item (e.g., '/page/page_xyz')
 * @param label - Optional cached label for the item
 * @param icon - Optional cached icon for the item
 */
export function getWorkspaceMenuItems(
	url: string,
	name?: string | null,
): ContextMenuItem[] {
	return getAddToProjectMenuItems(url, name);
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
