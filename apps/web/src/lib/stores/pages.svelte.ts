/**
 * Pages Store (Compatibility Layer)
 * 
 * This store provides backwards compatibility for components that still reference pagesStore.
 * It delegates to the workspace store for tree management.
 * 
 * TODO: Migrate components to use workspaceStore directly, then delete this file.
 */

import { createPage, updatePage, deletePage, type PageSummary, type Page } from '$lib/api/client';
import { workspaceStore, type EntityMetadata } from './workspace.svelte';

class PagesStore {
	loading = $state(false);

	/**
	 * Get the tree from the workspace store
	 * Note: This returns ExplorerTreeNode[] now, not the old TreeNode[]
	 */
	get tree() {
		return workspaceStore.tree;
	}

	async init() {
		// Delegate to workspace store
		await workspaceStore.init();
	}

	async refresh() {
		await workspaceStore.refreshTree();
	}

	/**
	 * Create a new page
	 */
	async createNewPage(title: string = 'Untitled', workspaceId?: string): Promise<Page> {
		const wsId = workspaceId || workspaceStore.activeWorkspaceId;
		const page = await createPage(title, '', wsId);
		
		// Create a shortcut node in the current workspace
		if (!workspaceStore.isSystemWorkspace) {
			await workspaceStore.createShortcut(page.id);
		}
		
		await this.refresh();
		return page;
	}

	/**
	 * Rename a page
	 */
	async renamePage(pageId: string, newTitle: string): Promise<void> {
		await updatePage(pageId, { title: newTitle });
		// Update registry
		workspaceStore.updateEntityMetadata(pageId, { name: newTitle });
	}

	/**
	 * Update a page locally (optimistic update for title changes)
	 */
	updatePageLocally(pageId: string, updates: Partial<PageSummary>): void {
		if (updates.title) {
			workspaceStore.updateEntityMetadata(pageId, { name: updates.title });
		}
	}

	/**
	 * Delete a page
	 */
	async removePage(pageId: string): Promise<void> {
		await deletePage(pageId);
		await this.refresh();
	}
}

export const pagesStore = new PagesStore();
