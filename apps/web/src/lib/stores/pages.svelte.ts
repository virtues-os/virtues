/**
 * Pages Store
 *
 * Domain-specific store for page management. Provides:
 * - Page CRUD operations with automatic cache invalidation
 * - Pinned/recent page tracking (localStorage-backed)
 * - Sidebar items computation (pinned first, then recent)
 * - Optimistic updates for responsive UI
 *
 * Coordinates with spaceStore for space-related operations.
 */

import { createPage, updatePage, deletePage, listPages, type PageSummary, type Page } from '$lib/api/client';
import { spaceStore, type EntityMetadata } from './space.svelte';

const PINNED_STORAGE_KEY = 'virtues-pinned-pages';
const RECENT_STORAGE_KEY = 'virtues-recent-pages';
const MAX_RECENT = 20;
const MAX_SIDEBAR_ITEMS = 8;

class PagesStore {
	loading = $state(false);

	// Pages list state (for PagesView)
	pages = $state<PageSummary[]>([]);
	pagesLoading = $state(false);
	pagesError = $state<string | null>(null);

	// Pinned/Recent tracking (persisted to localStorage)
	pinnedPageIds = $state<Set<string>>(new Set());
	recentPageIds = $state<string[]>([]);

	constructor() {
		this.loadPinnedRecent();
	}

	private loadPinnedRecent(): void {
		if (typeof window === 'undefined') return;

		try {
			const pinnedRaw = localStorage.getItem(PINNED_STORAGE_KEY);
			if (pinnedRaw) {
				this.pinnedPageIds = new Set(JSON.parse(pinnedRaw));
			}

			const recentRaw = localStorage.getItem(RECENT_STORAGE_KEY);
			if (recentRaw) {
				this.recentPageIds = JSON.parse(recentRaw);
			}
		} catch (e) {
			console.error('[PagesStore] Failed to load pinned/recent from localStorage:', e);
		}
	}

	private persistPinnedRecent(): void {
		if (typeof window === 'undefined') return;

		try {
			localStorage.setItem(PINNED_STORAGE_KEY, JSON.stringify([...this.pinnedPageIds]));
			localStorage.setItem(RECENT_STORAGE_KEY, JSON.stringify(this.recentPageIds));
		} catch (e) {
			console.error('[PagesStore] Failed to persist pinned/recent to localStorage:', e);
		}
	}

	/**
	 * Toggle pin status for a page
	 */
	togglePin(pageId: string): void {
		const newSet = new Set(this.pinnedPageIds);
		if (newSet.has(pageId)) {
			newSet.delete(pageId);
		} else {
			newSet.add(pageId);
		}
		this.pinnedPageIds = newSet;
		this.persistPinnedRecent();
	}

	/**
	 * Check if a page is pinned
	 */
	isPinned(pageId: string): boolean {
		return this.pinnedPageIds.has(pageId);
	}

	/**
	 * Mark a page as recently accessed
	 */
	markAsRecent(pageId: string): void {
		// Remove if exists, add to front
		const filtered = this.recentPageIds.filter(id => id !== pageId);
		this.recentPageIds = [pageId, ...filtered].slice(0, MAX_RECENT);
		this.persistPinnedRecent();
	}

	/**
	 * Get sidebar items: pinned first, then recent to fill remaining slots
	 */
	getSidebarItems(maxItems: number = MAX_SIDEBAR_ITEMS): PageSummary[] {
		// Get pinned pages (that still exist in pages list)
		const pinnedPages = this.pages.filter(p => this.pinnedPageIds.has(p.id));

		// Get recent pages (excluding pinned, that still exist)
		const recentPages = this.recentPageIds
			.filter(id => !this.pinnedPageIds.has(id))
			.map(id => this.pages.find(p => p.id === id))
			.filter((p): p is PageSummary => p !== undefined);

		// Combine: pinned first, then recent to fill remaining slots
		const remaining = Math.max(0, maxItems - pinnedPages.length);
		return [...pinnedPages, ...recentPages.slice(0, remaining)];
	}

	/**
	 * Get the views from the workspace store for the current workspace
	 */
	get views() {
		return spaceStore.spaceViews;
	}

	async init() {
		// Delegate to workspace store
		await spaceStore.init();
	}

	async refresh() {
		await spaceStore.refreshViews();
	}

	/**
	 * Create a new page
	 */
	async createNewPage(title: string = 'Untitled', spaceId?: string): Promise<Page> {
		// Don't auto-add for system space
		const sId = spaceStore.isSystemSpace
			? undefined
			: (spaceId || spaceStore.activeSpaceId);
		const page = await createPage(title, '', sId);

		// Invalidate the Pages view cache so the sidebar refreshes
		spaceStore.invalidateViewCache('page');

		// Refresh space items if added to a space
		if (sId) {
			await spaceStore.loadSpaceItems(sId);
		}

		await this.refresh();
		return page;
	}

	/**
	 * Rename a page (deprecated - use savePage instead)
	 */
	async renamePage(pageId: string, newTitle: string): Promise<void> {
		await this.savePage(pageId, { title: newTitle });
	}

	/**
	 * Save page content and metadata. Handles all side effects:
	 * - API call
	 * - Optimistic local update (registry)
	 * - Cache invalidation (smart folders)
	 * - Space items reload (sidebar)
	 */
	async savePage(pageId: string, updates: {
		title?: string;
		content?: string;
		icon?: string | null;
		cover_url?: string | null;
		tags?: string | null;
	}): Promise<void> {
		await updatePage(pageId, updates);

		// Optimistic local update for title/icon
		if (updates.title || 'icon' in updates) {
			const metadataUpdates: Partial<EntityMetadata> = {};
			if (updates.title) metadataUpdates.name = updates.title;
			if ('icon' in updates) metadataUpdates.icon = updates.icon || 'ri:file-text-line';
			spaceStore.updateEntityMetadata(pageId, metadataUpdates);
		}

		// Sidebar refresh (only if visible fields changed)
		if (updates.title || 'icon' in updates) {
			spaceStore.invalidateViewCache('page');
			if (!spaceStore.isSystemSpace) {
				await spaceStore.loadSpaceItems();
			}
		}
	}

	/**
	 * Update a page locally (optimistic update for title/icon changes)
	 * Used for immediate UI feedback before debounced save
	 */
	updatePageLocally(pageId: string, updates: Partial<PageSummary & { icon?: string | null }>): void {
		const metadataUpdates: Partial<EntityMetadata> = {};
		if (updates.title) {
			metadataUpdates.name = updates.title;
		}
		if ('icon' in updates) {
			metadataUpdates.icon = updates.icon || 'ri:file-text-line';
		}
		if (Object.keys(metadataUpdates).length > 0) {
			spaceStore.updateEntityMetadata(pageId, metadataUpdates);
		}
	}

	/**
	 * Delete a page. Handles all side effects:
	 * - Close any open tabs for this page
	 * - API call
	 * - Local list update
	 * - Cache invalidation
	 * - Space items reload
	 * - IndexedDB cleanup (Yjs offline persistence)
	 */
	async removePage(pageId: string): Promise<void> {
		// Close any open tabs for this page
		spaceStore.closeTabsByRoute(`/page/${pageId}`);
		// API call
		await deletePage(pageId);
		// Update local list
		this.removePageFromList(pageId);
		// Cache invalidation
		spaceStore.invalidateViewCache('page');
		// Sidebar refresh
		if (!spaceStore.isSystemSpace) {
			await spaceStore.loadSpaceItems();
		}
		await this.refresh();

		// Clean up IndexedDB (Yjs offline persistence)
		// This prevents stale data if the page ID is ever reused
		if (typeof indexedDB !== 'undefined') {
			try {
				indexedDB.deleteDatabase(pageId);
			} catch (e) {
				console.warn('[PagesStore] Failed to delete IndexedDB for page:', pageId, e);
			}
		}
	}

	/**
	 * Load all pages (for PagesView flat list)
	 */
	async loadPages(): Promise<void> {
		this.pagesLoading = true;
		this.pagesError = null;
		try {
			const response = await listPages();
			this.pages = response.pages;
		} catch (e) {
			console.error('[PagesStore] Failed to load pages:', e);
			this.pagesError = e instanceof Error ? e.message : 'Failed to load pages';
			this.pages = [];
		} finally {
			this.pagesLoading = false;
		}
	}

	/**
	 * Add a page to the list (optimistic update)
	 */
	addPage(page: PageSummary): void {
		this.pages = [page, ...this.pages];
	}

	/**
	 * Remove a page from the list (optimistic update)
	 */
	removePageFromList(pageId: string): void {
		this.pages = this.pages.filter(p => p.id !== pageId);
	}
}

export const pagesStore = new PagesStore();
