/**
 * Space Store
 *
 * UNIFIED state management for the Knowledge OS. This is the SINGLE source of truth.
 * All components should import from here.
 *
 * Architecture: Always-Panes Model
 * - Every tab lives in a pane
 * - Single-pane mode = panes.length === 1
 * - This eliminates 30+ conditional checks for split mode
 *
 * Features:
 * - Spaces (swipeable contexts like Arc browser)
 * - Views (manual lists or smart queries)
 * - Tabs (with full split-screen support)
 * - Entity metadata registry (lazy-loaded cache)
 */

import {
	listSpaces,
	createSpace as apiCreateSpace,
	updateSpace as apiUpdateSpace,
	deleteSpace as apiDeleteSpace,
	listViews,
	createView as apiCreateView,
	deleteView as apiDeleteView,
	resolveView as apiResolveView,
	listSpaceItems as apiListSpaceItems,
	addSpaceItem as apiAddSpaceItem,
	removeSpaceItem as apiRemoveSpaceItem,
	reorderSpaceItems as apiReorderSpaceItems,
	type Space,
	type SpaceSummary,
	type View,
	type ViewSummary,
	type ViewEntity
} from '$lib/api/client';
import {
	type Tab,
	type TabType,
	type FallbackView,
	type PaneState,
	getTabDomain,
	entityIdToRoute,
	routeToEntityId
} from '$lib/tabs/types';
import { parseRoute } from '$lib/tabs/registry';
import { pushState, replaceState } from '$app/navigation';

// Re-export types for convenience
export type { Tab, TabType, FallbackView, PaneState };
export { parseRoute };

// ============================================================================
// Types
// ============================================================================

export interface EntityMetadata {
	id: string;
	name: string;
	type: string;
	icon: string;
	route: string;
}

// Split state for backwards compatibility
export interface SplitState {
	enabled: boolean;
	panes: [PaneState, PaneState] | null;
	activePaneId: 'left' | 'right';
}

export interface TabState {
	tabs: Tab[];
	activeTabId: string | null;
	split: SplitState;
}

// ============================================================================
// Entity Type Utilities
// ============================================================================

const ENTITY_TYPE_MAP: Record<string, { type: string; icon: string; routePrefix: string }> = {
	session: { type: 'chat', icon: 'ri:chat-1-line', routePrefix: '/chat' },
	page: { type: 'page', icon: 'ri:file-text-line', routePrefix: '/page' },
	person: { type: 'person', icon: 'ri:user-line', routePrefix: '/person' },
	place: { type: 'place', icon: 'ri:map-pin-line', routePrefix: '/place' },
	org: { type: 'org', icon: 'ri:building-line', routePrefix: '/org' },
	thing: { type: 'thing', icon: 'ri:box-3-line', routePrefix: '/thing' },
	day: { type: 'day', icon: 'ri:calendar-line', routePrefix: '/day' },
	year: { type: 'year', icon: 'ri:calendar-line', routePrefix: '/year' },
	source: { type: 'source', icon: 'ri:database-2-line', routePrefix: '/source' },
	file: { type: 'drive', icon: 'ri:file-line', routePrefix: '/drive' }
};

/**
 * Get entity type info from an entity ID prefix
 */
export function getEntityTypeFromId(entityId: string): { type: string; icon: string; routePrefix: string } {
	const prefix = entityId.split('_')[0];
	return ENTITY_TYPE_MAP[prefix] || { type: 'unknown', icon: 'ri:question-line', routePrefix: '' };
}

/**
 * Build a route from an entity ID using namespace-based URLs
 */
export function getRouteFromEntityId(entityId: string): string {
	return entityIdToRoute(entityId);
}

// ============================================================================
// Store Class
// ============================================================================

const TAB_STORAGE_KEY_PREFIX = 'virtues-window-tabs';
const TAB_STORAGE_VERSION = 7; // Increment for entityId removal (route is now source of truth)
const WORKSPACE_STORAGE_KEY = 'virtues-active-workspace';

// Initialize activeSpaceId synchronously from localStorage to prevent flicker
const initialSpaceId =
	typeof window !== 'undefined'
		? localStorage.getItem(WORKSPACE_STORAGE_KEY) ?? 'space_system'
		: 'space_system';

class SpaceStore {
	// ============================================================================
	// Space State
	// ============================================================================
	spaces = $state<SpaceSummary[]>([]);
	activeSpaceId = $state<string>(initialSpaceId);

	// Views for each workspace
	views = $state<Map<string, ViewSummary[]>>(new Map());
	expandedViewIds = $state<Set<string>>(new Set());

	get spaceViews(): ViewSummary[] {
		return this.views.get(this.activeSpaceId) || [];
	}

	// ============================================================================
	// Tab State - Always Panes Model
	// ============================================================================
	// Every tab lives in a pane. Single-pane mode = panes.length === 1.

	panes = $state<PaneState[]>([
		{ id: 'left', tabs: [], activeTabId: null, width: 100 }
	]);
	activePaneId = $state<string>('left');

	// Derived - computed, not stored
	get isSplit(): boolean {
		return this.panes.length > 1;
	}

	get activePane(): PaneState | undefined {
		return this.panes.find(p => p.id === this.activePaneId);
	}

	get activeTab(): Tab | undefined {
		const pane = this.activePane;
		return pane?.tabs.find(t => t.id === pane.activeTabId);
	}

	// Backwards compatibility getters (read-only, for components not yet migrated)
	get tabs(): Tab[] {
		return this.panes[0]?.tabs ?? [];
	}

	get activeTabId(): string | null {
		return this.panes[0]?.activeTabId ?? null;
	}

	get split(): SplitState {
		if (this.panes.length > 1) {
			return {
				enabled: true,
				panes: [this.panes[0], this.panes[1]] as [PaneState, PaneState],
				activePaneId: this.activePaneId as 'left' | 'right'
			};
		}
		return { enabled: false, panes: null, activePaneId: 'left' };
	}

	get leftPane(): PaneState | null {
		return this.panes[0] || null;
	}

	get rightPane(): PaneState | null {
		return this.panes[1] || null;
	}

	// ============================================================================
	// Other State
	// ============================================================================
	fallbackPreference = $state<FallbackView>('empty');
	swipeProgress = $state(0);

	private _history = $state<string[]>([]);
	private _historyIndex = $state<number>(-1);
	private _isNavigatingHistory = false;

	viewCache = $state<Map<string, ViewEntity[]>>(new Map());
	viewCacheVersion = $state<number>(0); // Incremented when cache is invalidated
	spaceItems = $state<Map<string, ViewEntity[]>>(new Map()); // Root-level items per workspace
	registry = $state<Map<string, EntityMetadata>>(new Map());

	loading = $state(false);
	viewsLoading = $state(false);

	private initialized = false;
	private urlSyncEnabled = false;
	private _skipUrlSync = false;

	// ============================================================================
	// Space Getters
	// ============================================================================
	get activeSpace(): SpaceSummary | undefined {
		return this.spaces.find((w) => w.id === this.activeSpaceId);
	}

	get isSystemSpace(): boolean {
		return this.activeSpaceId === 'space_system';
	}

	// ============================================================================
	// Initialization
	// ============================================================================

	async init(): Promise<void> {
		if (this.initialized) return;
		if (typeof window === 'undefined') return;

		this.initialized = true;

		try {
			await this.loadSpaces();

			// Validate saved workspace exists, fall back to space_system if not
			if (!this.spaces.find((w) => w.id === this.activeSpaceId)) {
				this.activeSpaceId = 'space_system';
				localStorage.setItem(WORKSPACE_STORAGE_KEY, 'space_system');
			}

			await this.loadAllViews();
			await this.loadAllSpaceItems(); // Load root items for all spaces (prevents CLS on switch)
			this.restoreTabState();
		} catch (e) {
			console.error('[SpaceStore] Failed to initialize:', e);
		}
	}

	// ============================================================================
	// URL Sync
	// ============================================================================

	initUrlSync(): void {
		if (typeof window === 'undefined') return;
		if (this.urlSyncEnabled) return;

		this.urlSyncEnabled = true;
		window.addEventListener('popstate', this.handlePopState);
		this.syncActiveToUrl(false);
	}

	destroyUrlSync(): void {
		if (typeof window === 'undefined') return;
		window.removeEventListener('popstate', this.handlePopState);
		this.urlSyncEnabled = false;
	}

	private buildUrlFromState(): string {
		const leftPane = this.panes[0];
		const rightPane = this.panes[1];

		const leftTab = leftPane?.tabs.find(t => t.id === leftPane.activeTabId);
		if (!leftTab?.route) return '/';

		if (rightPane) {
			const rightTab = rightPane.tabs.find(t => t.id === rightPane.activeTabId);
			if (rightTab?.route) {
				const url = new URL(leftTab.route, window.location.origin);
				url.searchParams.set('right', rightTab.route);
				return url.pathname + url.search;
			}
		}

		return leftTab.route;
	}

	syncActiveToUrl(usePush: boolean = false): void {
		if (typeof window === 'undefined') return;
		if (!this.urlSyncEnabled || this._skipUrlSync) return;

		const url = this.buildUrlFromState();
		const currentUrl = window.location.pathname + window.location.search;

		if (currentUrl === url) return;

		// Use SvelteKit's shallow routing to update URL without triggering navigation
		if (usePush) {
			pushState(url, {});
		} else {
			replaceState(url, {});
		}
	}

	handleDeepLink(path: string, rightRoute: string | null): void {
		this._skipUrlSync = true;

		try {
			if (path && path !== '/') {
				this.openTabFromRoute(path, { forceNew: false });
			}

			if (rightRoute) {
				if (!this.isSplit) {
					this.enableSplit();
				}
				this.openTabFromRoute(rightRoute, { paneId: 'right', forceNew: false });
			} else if (this.isSplit) {
				this.disableSplit();
			}
		} finally {
			this._skipUrlSync = false;
		}
	}

	private handlePopState = (): void => {
		if (typeof window === 'undefined') return;

		const path = window.location.pathname;
		const searchParams = new URLSearchParams(window.location.search);
		const rightRoute = searchParams.get('right');

		this.handleDeepLink(path, rightRoute);
	};

	// ============================================================================
	// Space Management
	// ============================================================================

	async loadSpaces(): Promise<void> {
		this.loading = true;
		try {
			const response = await listSpaces();
			this.spaces = response.spaces;

			if (!this.spaces.find((w) => w.id === this.activeSpaceId)) {
				this.activeSpaceId = 'space_system';
			}
		} catch (e) {
			console.error('[SpaceStore] Failed to load spaces:', e);
		} finally {
			this.loading = false;
		}
	}

	async createSpace(name: string, icon?: string, themeId?: string, accentColor?: string): Promise<Space | null> {
		try {
			const space = await apiCreateSpace(name, icon, themeId, accentColor);
			await this.loadSpaces();
			return space;
		} catch (e) {
			console.error('[SpaceStore] Failed to create workspace:', e);
			return null;
		}
	}

	async updateSpace(
		id: string,
		updates: { name?: string; icon?: string; accent_color?: string }
	): Promise<void> {
		try {
			await apiUpdateSpace(id, updates);
			await this.loadSpaces();
		} catch (e) {
			console.error('[SpaceStore] Failed to update workspace:', e);
		}
	}

	async deleteSpace(id: string): Promise<void> {
		const space = this.spaces.find((w) => w.id === id);
		if (space?.is_system) {
			console.warn('[SpaceStore] Cannot delete system workspace');
			return;
		}

		try {
			if (this.activeSpaceId === id) {
				await this.switchSpace('space_system', true);
			}

			await apiDeleteSpace(id);
			await this.loadSpaces();
		} catch (e) {
			console.error('[SpaceStore] Failed to delete workspace:', e);
		}
	}

	async switchSpace(spaceId: string, usePush: boolean = false): Promise<void> {
		if (spaceId === this.activeSpaceId) return;

		this.persistTabState();
		this.activeSpaceId = spaceId;

		// Persist active workspace to localStorage
		localStorage.setItem(WORKSPACE_STORAGE_KEY, spaceId);

		// Skip URL sync during tab restoration to prevent double-sync
		// (openDefaultTab -> openTab -> syncActiveToUrl would push when we want replace)
		this._skipUrlSync = true;
		try {
			this.restoreTabState();
		} finally {
			this._skipUrlSync = false;
		}

		this.viewCache = new Map();

		if (!this.views.has(spaceId)) {
			await this.loadViews(spaceId);
		}

		// Load workspace items if not cached
		if (!this.spaceItems.has(spaceId)) {
			await this.loadSpaceItems(spaceId);
		}

		// Sync URL to reflect new workspace's active tab
		// usePush=false (replaceState) for swipes to avoid history pollution
		// usePush=true (pushState) for explicit clicks to enable back navigation
		this.syncActiveToUrl(usePush);
	}

	navigateSpace(direction: 'prev' | 'next', usePush: boolean = false): void {
		const currentIndex = this.spaces.findIndex((w) => w.id === this.activeSpaceId);
		if (currentIndex === -1) return;

		let newIndex: number;
		if (direction === 'prev') {
			newIndex = currentIndex > 0 ? currentIndex - 1 : this.spaces.length - 1;
		} else {
			newIndex = currentIndex < this.spaces.length - 1 ? currentIndex + 1 : 0;
		}

		this.switchSpace(this.spaces[newIndex].id, usePush);
	}

	// ============================================================================
	// Views Management
	// ============================================================================

	async loadViews(spaceId: string): Promise<void> {
		this.viewsLoading = true;
		try {
			const response = await listViews(spaceId);
			const newViews = new Map(this.views);
			newViews.set(spaceId, response.views);
			this.views = newViews;
		} catch (e) {
			console.error('[SpaceStore] Failed to load views:', e);
			const newViews = new Map(this.views);
			newViews.set(spaceId, []);
			this.views = newViews;
		} finally {
			this.viewsLoading = false;
		}
	}

	async loadAllViews(): Promise<void> {
		await Promise.all(this.spaces.map(ws => this.loadViews(ws.id)));
	}

	async refreshViews(): Promise<void> {
		await this.loadViews(this.activeSpaceId);
	}

	getViewsForSpace(spaceId: string): ViewSummary[] {
		return this.views.get(spaceId) || [];
	}

	toggleViewExpanded(viewId: string): void {
		const newSet = new Set(this.expandedViewIds);
		if (newSet.has(viewId)) {
			newSet.delete(viewId);
		} else {
			newSet.add(viewId);
		}
		this.expandedViewIds = newSet;
	}

	isViewExpanded(viewId: string): boolean {
		return this.expandedViewIds.has(viewId);
	}

	// ============================================================================
	// View CRUD
	// ============================================================================

	async createManualView(name: string, icon?: string): Promise<View | null> {
		const space = this.activeSpace;
		if (space?.is_system) return null;

		try {
			const view = await apiCreateView(this.activeSpaceId, {
				name,
				icon,
				view_type: 'manual'
			});
			await this.refreshViews();
			return view;
		} catch (e) {
			console.error('[SpaceStore] Failed to create manual view:', e);
			return null;
		}
	}

	async createSmartView(name: string, queryConfig: object, icon?: string): Promise<View | null> {
		const space = this.activeSpace;
		if (space?.is_system) return null;

		try {
			const view = await apiCreateView(this.activeSpaceId, {
				name,
				icon,
				view_type: 'smart',
				query_config: queryConfig
			});
			await this.refreshViews();
			return view;
		} catch (e) {
			console.error('[SpaceStore] Failed to create smart view:', e);
			return null;
		}
	}

	async deleteView(viewId: string): Promise<void> {
		const space = this.activeSpace;
		if (space?.is_system) return;

		try {
			await apiDeleteView(viewId);
			await this.refreshViews();
		} catch (e) {
			console.error('[SpaceStore] Failed to delete view:', e);
		}
	}

	// ============================================================================
	// View Resolution
	// ============================================================================

	async resolveView(viewId: string, forceRefresh = false): Promise<ViewEntity[]> {
		if (!forceRefresh) {
			const cached = this.viewCache.get(viewId);
			if (cached) return cached;
		}

		try {
			const response = await apiResolveView(viewId);

			const newCache = new Map(this.viewCache);
			newCache.set(viewId, response.entities);
			this.viewCache = newCache;

			return response.entities;
		} catch (e) {
			console.error('[SpaceStore] Failed to resolve view:', e);
			return [];
		}
	}

	/**
	 * Invalidate view cache for specific views or by namespace.
	 * Use this when entities are created/updated/deleted to refresh smart views.
	 * @param namespace - Optional namespace to invalidate (e.g., 'chat', 'page')
	 *                    If not provided, clears entire cache.
	 */
	invalidateViewCache(namespace?: string): void {
		if (!namespace) {
			// Clear entire cache
			this.viewCache = new Map();
			this.viewCacheVersion++;
			return;
		}

		// Map namespace to known system view IDs
		const namespaceToViewId: Record<string, string> = {
			chat: 'view_sys_chats',
			page: 'view_sys_pages',
		};

		const viewId = namespaceToViewId[namespace];
		if (viewId) {
			const newCache = new Map(this.viewCache);
			newCache.delete(viewId);
			this.viewCache = newCache;
			this.viewCacheVersion++;
		}
	}

	// ============================================================================
	// Space Items (root-level items at workspace level, not in folders)
	// ============================================================================

	/**
	 * Get cached workspace items for current workspace
	 */
	getSpaceItems(spaceId?: string): ViewEntity[] {
		const wsId = spaceId ?? this.activeSpaceId;
		return this.spaceItems.get(wsId) || [];
	}

	/**
	 * Load workspace items from backend
	 */
	async loadSpaceItems(spaceId?: string): Promise<ViewEntity[]> {
		const wsId = spaceId ?? this.activeSpaceId;
		try {
			const items = await apiListSpaceItems(wsId);
			const newMap = new Map(this.spaceItems);
			newMap.set(wsId, items);
			this.spaceItems = newMap;
			return items;
		} catch (e) {
			console.error('[SpaceStore] Failed to load workspace items:', e);
			return [];
		}
	}

	/**
	 * Load workspace items for all spaces (parallel)
	 * Prevents CLS when switching spaces
	 */
	async loadAllSpaceItems(): Promise<void> {
		await Promise.all(this.spaces.map(ws => this.loadSpaceItems(ws.id)));
	}

	/**
	 * Add an item to workspace root level
	 */
	async addSpaceItem(url: string, spaceId?: string): Promise<void> {
		const wsId = spaceId ?? this.activeSpaceId;
		try {
			await apiAddSpaceItem(wsId, url);
			// Reload to get resolved entity
			await this.loadSpaceItems(wsId);
		} catch (e) {
			console.error('[SpaceStore] Failed to add workspace item:', e);
		}
	}

	/**
	 * Remove an item from workspace root level
	 */
	async removeSpaceItem(url: string, spaceId?: string): Promise<void> {
		const wsId = spaceId ?? this.activeSpaceId;
		try {
			await apiRemoveSpaceItem(wsId, url);
			// Reload to ensure reactive update (matches addSpaceItem pattern)
			await this.loadSpaceItems(wsId);
		} catch (e) {
			console.error('[SpaceStore] Failed to remove workspace item:', e);
		}
	}

	/**
	 * Reorder workspace root items
	 */
	async reorderSpaceItems(urlOrder: string[], spaceId?: string): Promise<void> {
		const wsId = spaceId ?? this.activeSpaceId;
		try {
			await apiReorderSpaceItems(wsId, urlOrder);
			// Reload to get updated order
			await this.loadSpaceItems(wsId);
		} catch (e) {
			console.error('[SpaceStore] Failed to reorder workspace items:', e);
		}
	}

	// ============================================================================
	// Entity Registry
	// ============================================================================

	async getEntityMetadata(entityId: string): Promise<EntityMetadata | null> {
		const cached = this.registry.get(entityId);
		if (cached) return cached;

		const { type, icon } = getEntityTypeFromId(entityId);
		const route = getRouteFromEntityId(entityId);

		const metadata: EntityMetadata = {
			id: entityId,
			name: entityId,
			type,
			icon,
			route
		};

		const newRegistry = new Map(this.registry);
		newRegistry.set(entityId, metadata);
		this.registry = newRegistry;

		return metadata;
	}

	updateEntityMetadata(entityId: string, updates: Partial<EntityMetadata>): void {
		const existing = this.registry.get(entityId);
		if (existing) {
			const newRegistry = new Map(this.registry);
			newRegistry.set(entityId, { ...existing, ...updates });
			this.registry = newRegistry;
		}
	}

	// ============================================================================
	// Tab Persistence
	// ============================================================================

	private getTabStorageKey(): string {
		return `${TAB_STORAGE_KEY_PREFIX}-${this.activeSpaceId}`;
	}

	private persistTabState(): void {
		if (typeof window === 'undefined') return;

		const data = {
			version: TAB_STORAGE_VERSION,
			panes: this.panes,
			activePaneId: this.activePaneId,
			expandedViewIds: [...this.expandedViewIds]
		};

		try {
			localStorage.setItem(this.getTabStorageKey(), JSON.stringify(data));
		} catch (e) {
			console.warn('[SpaceStore] Failed to persist tab state:', e);
		}
	}

	private restoreTabState(): void {
		if (typeof window === 'undefined') return;

		// Load fallback preference
		const fallbackStored = localStorage.getItem('virtues-fallback-preference');
		if (fallbackStored && ['empty', 'chat', 'conway', 'dog-jump', 'day-today'].includes(fallbackStored)) {
			this.fallbackPreference = fallbackStored as FallbackView;
		}

		const storageKey = this.getTabStorageKey();

		try {
			const stored = localStorage.getItem(storageKey);
			if (stored) {
				const data = JSON.parse(stored);

				// Version 6+: namespace-based format
				if (data.version >= TAB_STORAGE_VERSION && Array.isArray(data.panes)) {
					// Deduplicate tabs within each pane to prevent "each_key_duplicate" errors
					// This can happen if state gets corrupted somehow
					const deduplicatedPanes = data.panes.map((pane: PaneState) => {
						const seenIds = new Set<string>();
						const uniqueTabs = pane.tabs.filter((tab: Tab) => {
							if (seenIds.has(tab.id)) {
								console.warn(`[SpaceStore] Removing duplicate tab: ${tab.id}`);
								return false;
							}
							seenIds.add(tab.id);
							return true;
						});
						return { ...pane, tabs: uniqueTabs };
					});

					this.panes = deduplicatedPanes;
					this.activePaneId = data.activePaneId || 'left';
					if (data.expandedViewIds) {
						this.expandedViewIds = new Set(data.expandedViewIds);
					}
					return;
				}

				// Older versions - clear and start fresh (clean slate approach)
				localStorage.removeItem(storageKey);
			}
		} catch (e) {
			console.warn('[SpaceStore] Failed to restore tab state:', e);
		}

		// Default: single pane with no tabs
		this.panes = [{ id: 'left', tabs: [], activeTabId: null, width: 100 }];
		this.activePaneId = 'left';
		this.openDefaultTab();
	}

	private openDefaultTab(): void {
		// Dashboard is now the default - it shows when there are no tabs
		// No need to open a default tab anymore since Dashboard renders inline
		// But we can still respect legacy preferences for users who want a specific view
		const pref = this.fallbackPreference;
		if (pref === 'chat') {
			this.openTab({ type: 'chat', label: 'New Chat', route: '/chat', icon: 'ri:chat-1-line' });
		} else if (pref === 'conway') {
			this.openTabFromRoute('/life');
		} else if (pref === 'dog-jump') {
			this.openTabFromRoute('/jump');
		} else if (pref === 'day-today') {
			const today = new Date().toISOString().split('T')[0];
			this.openTabFromRoute(`/day/day_${today}`);
		}
		// 'empty' now means Dashboard - no tab needed, Dashboard renders inline
	}

	// ============================================================================
	// Pane Helpers (internal)
	// ============================================================================

	private updatePane(paneId: string, updater: (pane: PaneState) => PaneState): void {
		this.panes = this.panes.map(p => p.id === paneId ? updater(p) : p);
	}

	private findPaneForTab(tabId: string): PaneState | undefined {
		return this.panes.find(p => p.tabs.some(t => t.id === tabId));
	}

	// ============================================================================
	// Tab CRUD - Unified Implementation
	// ============================================================================

	openTab(input: Omit<Tab, 'id' | 'createdAt'>, paneId?: string): string {
		const id = crypto.randomUUID();
		const tab: Tab = { ...input, id, createdAt: Date.now() };
		const targetPaneId = paneId ?? this.activePaneId;

		this.pushToHistory(id);

		this.updatePane(targetPaneId, pane => ({
			...pane,
			tabs: [...pane.tabs, tab],
			activeTabId: id
		}));

		this.activePaneId = targetPaneId;
		this.persistTabState();
		this.syncActiveToUrl(true);
		return id;
	}

	openTabFromRoute(route: string, options?: {
		label?: string;
		forceNew?: boolean;
		preferEmptyPane?: boolean;
		paneId?: 'left' | 'right';
	}): string {
		const parsed = parseRoute(route);
		const targetDomain = getTabDomain(parsed.type);
		// Use normalized route if available (e.g., /day → /day/day_2026-01-25)
		const effectiveRoute = parsed.normalizedRoute || route;

		// Find existing tab if not forcing new
		if (!options?.forceNew) {
			let result: { tab: Tab; paneId: string } | undefined;

			if (parsed.entityId) {
				// Entity-based tabs: match by route (URL is the identity)
				result = this.findTab((t) => t.route === effectiveRoute);
			} else if (parsed.virtuesPage) {
				// System pages: match by virtuesPage
				result = this.findTab((t) => t.type === 'virtues' && t.virtuesPage === parsed.virtuesPage);
			} else if (parsed.storagePath) {
				// Storage pages: match by storagePath
				result = this.findTab((t) => t.type === 'drive' && t.storagePath === parsed.storagePath);
			} else {
				// List views: match by type only (no entity, no virtues page, no storage path)
				result = this.findTab((t) => t.type === parsed.type && !t.virtuesPage && !t.storagePath && !routeToEntityId(t.route));
			}

			if (result) {
				this.setActiveTab(result.tab.id);
				return result.tab.id;
			}

			// Hybrid navigation: same domain navigates in place
			const currentTab = this.activeTab;
			if (currentTab && !currentTab.pinned) {
				const currentDomain = getTabDomain(currentTab.type);
				if (currentDomain === targetDomain) {
					this.updateTab(currentTab.id, {
						type: parsed.type,
						label: options?.label || parsed.label,
						route: effectiveRoute,
						icon: parsed.icon,
						storagePath: parsed.storagePath,
						virtuesPage: parsed.virtuesPage
					});
					return currentTab.id;
				}
			}
		}

		// Create new tab
		const tabInput = {
			type: parsed.type,
			label: options?.label || parsed.label,
			route: effectiveRoute,
			icon: parsed.icon,
			storagePath: parsed.storagePath,
			virtuesPage: parsed.virtuesPage
		};

		// Determine target pane
		let targetPaneId = options?.paneId ?? this.activePaneId;

		if (options?.preferEmptyPane && this.isSplit) {
			if (this.panes[0].tabs.length === 0) targetPaneId = 'left';
			else if (this.panes[1]?.tabs.length === 0) targetPaneId = 'right';
		}

		return this.openTab(tabInput, targetPaneId);
	}

	openEntityTab(entityId: string, name?: string): string {
		const route = getRouteFromEntityId(entityId);
		return this.openTabFromRoute(route, { label: name || entityId });
	}

	openOrFocusChat(chatId: string, label?: string): void {
		const entityId = chatId.startsWith('chat_') ? chatId : `chat_${chatId}`;
		const route = `/chat/${entityId}`;
		const existing = this.findTab((t) => t.route === route);
		if (existing) {
			this.setActiveTab(existing.tab.id);
		} else {
			this.openTab({
				type: 'chat',
				label: label || 'Chat',
				route,
				icon: 'ri:chat-1-line'
			});
		}
	}

	closeTab(tabId: string): void {
		const pane = this.findPaneForTab(tabId);
		if (!pane) return;

		const tabIndex = pane.tabs.findIndex(t => t.id === tabId);
		const newTabs = pane.tabs.filter(t => t.id !== tabId);

		// If this was the last tab in a split pane, collapse split
		if (newTabs.length === 0 && this.isSplit) {
			const otherPane = this.panes.find(p => p.id !== pane.id);
			if (otherPane) {
				this.panes = [{
					id: 'left',
					tabs: otherPane.tabs,
					activeTabId: otherPane.activeTabId,
					width: 100
				}];
				this.activePaneId = 'left';
				this.persistTabState();
				this.syncActiveToUrl(false);
				return;
			}
		}

		// If closing last tab in single pane, open default
		if (newTabs.length === 0 && !this.isSplit) {
			this.panes = [{ id: 'left', tabs: [], activeTabId: null, width: 100 }];
			this.openDefaultTab();
			return;
		}

		// Update active tab if needed
		let newActiveId = pane.activeTabId;
		if (newActiveId === tabId) {
			if (tabIndex === newTabs.length) {
				newActiveId = newTabs[tabIndex - 1]?.id || null;
			} else {
				newActiveId = newTabs[tabIndex]?.id || null;
			}
		}

		this.updatePane(pane.id, () => ({
			...pane,
			tabs: newTabs,
			activeTabId: newActiveId
		}));

		this.persistTabState();
		this.syncActiveToUrl(false);
	}

	closeOtherTabs(tabId: string, paneId?: string): void {
		const targetPaneId = paneId ?? this.findPaneForTab(tabId)?.id;
		if (!targetPaneId) return;

		const pane = this.panes.find(p => p.id === targetPaneId);
		const tabToKeep = pane?.tabs.find(t => t.id === tabId);
		if (!tabToKeep) return;

		this.updatePane(targetPaneId, () => ({
			id: targetPaneId,
			tabs: [tabToKeep],
			activeTabId: tabId,
			width: pane?.width ?? 100
		}));

		this.persistTabState();
		this.syncActiveToUrl(false);
	}

	closeTabsToRight(tabId: string, paneId?: string): void {
		const targetPaneId = paneId ?? this.findPaneForTab(tabId)?.id;
		if (!targetPaneId) return;

		const pane = this.panes.find(p => p.id === targetPaneId);
		if (!pane) return;

		const index = pane.tabs.findIndex(t => t.id === tabId);
		if (index === -1) return;

		const newTabs = pane.tabs.slice(0, index + 1);
		const newActiveId = newTabs.some(t => t.id === pane.activeTabId)
			? pane.activeTabId
			: newTabs[newTabs.length - 1]?.id || null;

		this.updatePane(targetPaneId, () => ({
			...pane,
			tabs: newTabs,
			activeTabId: newActiveId
		}));

		this.persistTabState();
		this.syncActiveToUrl(false);
	}

	closeAllTabs(): void {
		this.panes = [{ id: 'left', tabs: [], activeTabId: null, width: 100 }];
		this.activePaneId = 'left';
		this._history = [];
		this._historyIndex = -1;
		localStorage.removeItem(this.getTabStorageKey());
	}

	/**
	 * Close all tabs that match a given route.
	 * Used when deleting an entity to close any open tabs for it.
	 */
	closeTabsByRoute(route: string): void {
		for (const pane of this.panes) {
			const tabsToClose = pane.tabs.filter(t => t.route === route);
			for (const tab of tabsToClose) {
				this.closeTab(tab.id);
			}
		}
	}

	setActiveTab(tabId: string): void {
		const pane = this.findPaneForTab(tabId);
		if (!pane) return;

		this.pushToHistory(tabId);

		this.updatePane(pane.id, p => ({
			...p,
			activeTabId: tabId
		}));

		this.activePaneId = pane.id;
		this.persistTabState();
		this.syncActiveToUrl(false);
	}

	// Backwards compatibility alias
	setActiveTabInPane(tabId: string, paneId: 'left' | 'right'): void {
		const pane = this.panes.find(p => p.id === paneId);
		if (!pane?.tabs.some(t => t.id === tabId)) return;

		this.pushToHistory(tabId);

		this.updatePane(paneId, p => ({ ...p, activeTabId: tabId }));
		this.activePaneId = paneId;
		this.persistTabState();
		this.syncActiveToUrl(false);
	}

	updateTab(tabId: string, updates: Partial<Omit<Tab, 'id' | 'createdAt'>>): void {
		const pane = this.findPaneForTab(tabId);
		if (!pane) return;

		// Check if route is changing (need to sync URL if so)
		const routeChanged = updates.route !== undefined;

		this.updatePane(pane.id, p => ({
			...p,
			tabs: p.tabs.map(t => t.id === tabId ? { ...t, ...updates } : t)
		}));

		this.persistTabState();

		// Sync URL if route changed (e.g., new chat "/" → "/chat/chat_xyz")
		if (routeChanged) {
			this.syncActiveToUrl(false);
		}
	}

	togglePin(tabId: string): void {
		const pane = this.findPaneForTab(tabId);
		if (!pane) return;

		const tab = pane.tabs.find(t => t.id === tabId);
		if (!tab) return;

		this.updatePane(pane.id, p => {
			const updatedTabs = p.tabs.map(t =>
				t.id === tabId ? { ...t, pinned: !t.pinned } : t
			);
			// Sort pinned tabs first
			const sortedTabs = [...updatedTabs].sort((a, b) => {
				if (a.pinned && !b.pinned) return -1;
				if (!a.pinned && b.pinned) return 1;
				return 0;
			});
			return { ...p, tabs: sortedTabs };
		});

		this.persistTabState();
	}

	reorderTabs(fromIndex: number, toIndex: number, paneId?: string): void {
		const targetPaneId = paneId ?? this.activePaneId;
		const pane = this.panes.find(p => p.id === targetPaneId);
		if (!pane) return;

		if (fromIndex === toIndex) return;
		if (fromIndex < 0 || fromIndex >= pane.tabs.length) return;
		if (toIndex < 0 || toIndex >= pane.tabs.length) return;

		this.updatePane(targetPaneId, p => {
			const newTabs = [...p.tabs];
			const [moved] = newTabs.splice(fromIndex, 1);
			newTabs.splice(toIndex, 0, moved);
			return { ...p, tabs: newTabs };
		});

		this.persistTabState();
	}

	// Backwards compatibility alias
	reorderTabsInPane(fromIndex: number, toIndex: number, paneId: 'left' | 'right'): void {
		this.reorderTabs(fromIndex, toIndex, paneId);
	}

	/**
	 * Set tab order directly from an array of tab IDs.
	 * Used by dndzone which provides the reordered array.
	 */
	setTabOrder(tabIds: string[], paneId?: string): void {
		const targetPaneId = paneId ?? this.activePaneId;
		const pane = this.panes.find(p => p.id === targetPaneId);
		if (!pane) return;

		this.updatePane(targetPaneId, p => {
			// Create a map for quick lookup
			const tabMap = new Map(p.tabs.map(t => [t.id, t]));
			// Reorder based on the provided IDs
			const reorderedTabs = tabIds
				.map(id => tabMap.get(id))
				.filter((t): t is Tab => t !== undefined);
			return { ...p, tabs: reorderedTabs };
		});

		this.persistTabState();
	}

	// ============================================================================
	// Tab Query Methods
	// ============================================================================

	findTab(predicate: (tab: Tab) => boolean): { tab: Tab; paneId: string } | undefined {
		for (const pane of this.panes) {
			const found = pane.tabs.find(predicate);
			if (found) return { tab: found, paneId: pane.id };
		}
		return undefined;
	}

	findTabPane(tabId: string): 'left' | 'right' | null {
		const pane = this.findPaneForTab(tabId);
		if (!pane) return null;
		return pane.id as 'left' | 'right';
	}

	getAllTabs(): Tab[] {
		return this.panes.flatMap(p => p.tabs);
	}

	getActiveTabsForSidebar(): Tab[] {
		return this.panes
			.map(pane => pane.tabs.find(t => t.id === pane.activeTabId))
			.filter((t): t is Tab => t !== undefined);
	}

	// ============================================================================
	// Navigation History
	// ============================================================================

	private pushToHistory(tabId: string): void {
		if (this._isNavigatingHistory) return;

		if (this._historyIndex < this._history.length - 1) {
			this._history = this._history.slice(0, this._historyIndex + 1);
		}

		if (this._history[this._history.length - 1] !== tabId) {
			this._history = [...this._history, tabId];
			this._historyIndex = this._history.length - 1;
		}
	}

	canGoBack(): boolean {
		return this._historyIndex > 0;
	}

	canGoForward(): boolean {
		return this._historyIndex < this._history.length - 1;
	}

	goBack(): void {
		if (!this.canGoBack()) return;

		this._isNavigatingHistory = true;
		this._historyIndex--;
		const tabId = this._history[this._historyIndex];

		this.setActiveTab(tabId);
		this._isNavigatingHistory = false;
	}

	goForward(): void {
		if (!this.canGoForward()) return;

		this._isNavigatingHistory = true;
		this._historyIndex++;
		const tabId = this._history[this._historyIndex];

		this.setActiveTab(tabId);
		this._isNavigatingHistory = false;
	}

	// ============================================================================
	// Split Screen Methods
	// ============================================================================

	enableSplit(): void {
		if (this.isSplit) return;

		const currentPane = this.panes[0];
		this.panes = [
			{ ...currentPane, width: 50 },
			{ id: 'right', tabs: [], activeTabId: null, width: 50 }
		];

		this.persistTabState();
		this.syncActiveToUrl(false);
	}

	disableSplit(): void {
		if (!this.isSplit) return;

		// Merge all tabs to single pane
		const allTabs = this.panes.flatMap(p => p.tabs);
		const activeId = this.activePane?.activeTabId ?? allTabs[0]?.id ?? null;

		this.panes = [{ id: 'left', tabs: allTabs, activeTabId: activeId, width: 100 }];
		this.activePaneId = 'left';

		this.persistTabState();
		this.syncActiveToUrl(false);
	}

	toggleSplit(): void {
		if (this.isSplit) {
			this.disableSplit();
		} else {
			this.enableSplit();
		}
	}

	setActivePane(paneId: 'left' | 'right'): void {
		if (!this.panes.some(p => p.id === paneId)) return;
		this.activePaneId = paneId;
		this.persistTabState();
	}

	// Backwards compatibility aliases
	openTabInPane(input: Omit<Tab, 'id' | 'createdAt'>, paneId: 'left' | 'right'): string {
		return this.openTab(input, paneId);
	}

	closeTabInPane(tabId: string, _paneId: 'left' | 'right'): void {
		this.closeTab(tabId);
	}

	moveTabToPane(tabId: string, targetPaneId: 'left' | 'right'): void {
		if (!this.isSplit) return;

		const sourcePaneId = this.findTabPane(tabId);
		if (!sourcePaneId || sourcePaneId === targetPaneId) return;

		const sourcePane = this.panes.find(p => p.id === sourcePaneId);
		const targetPane = this.panes.find(p => p.id === targetPaneId);
		if (!sourcePane || !targetPane) return;

		const tab = sourcePane.tabs.find(t => t.id === tabId);
		if (!tab) return;

		// Remove from source
		const newSourceTabs = sourcePane.tabs.filter(t => t.id !== tabId);
		const newSourceActiveId = sourcePane.activeTabId === tabId
			? (newSourceTabs[0]?.id || null)
			: sourcePane.activeTabId;

		// Add to target
		const newTargetTabs = [...targetPane.tabs, tab];

		this.panes = this.panes.map(p => {
			if (p.id === sourcePaneId) {
				return { ...p, tabs: newSourceTabs, activeTabId: newSourceActiveId };
			}
			if (p.id === targetPaneId) {
				return { ...p, tabs: newTargetTabs, activeTabId: tabId };
			}
			return p;
		});

		this.activePaneId = targetPaneId;

		// If source pane is now empty, collapse split
		if (newSourceTabs.length === 0) {
			this.disableSplit();
		} else {
			this.persistTabState();
			this.syncActiveToUrl(false);
		}
	}

	setPaneWidth(leftWidth: number): void {
		if (!this.isSplit) return;

		const clampedWidth = Math.max(20, Math.min(80, leftWidth));

		this.panes = [
			{ ...this.panes[0], width: clampedWidth },
			{ ...this.panes[1], width: 100 - clampedWidth }
		];

		this.persistTabState();
	}

	openChatContext(chatId: string, currentPaneId: 'left' | 'right' | null): string {
		const targetPaneId = currentPaneId === 'right' ? 'left' : 'right';
		const entityId = chatId.startsWith('chat_') ? chatId : `chat_${chatId}`;
		const route = `/chat/${entityId}?view=context`;

		// Check if context view is already open for this chat
		const existing = this.findTab((t) => t.route === route);
		if (existing) {
			this.setActiveTab(existing.tab.id);
			return existing.tab.id;
		}

		if (!this.isSplit) {
			this.enableSplit();
		}

		return this.openTab(
			{
				type: 'chat',
				label: 'Context',
				route,
				icon: 'ri:information-line'
			},
			targetPaneId
		);
	}

	// ============================================================================
	// Preferences
	// ============================================================================

	setFallbackPreference(pref: FallbackView): void {
		this.fallbackPreference = pref;
		if (typeof window !== 'undefined') {
			localStorage.setItem('virtues-fallback-preference', pref);
		}
	}

	// ============================================================================
	// Debug
	// ============================================================================

	debug(): void {
		console.log('[SpaceStore Debug]', {
			spaces: this.spaces,
			activeSpaceId: this.activeSpaceId,
			views: Object.fromEntries(this.views),
			panes: this.panes,
			activePaneId: this.activePaneId,
			isSplit: this.isSplit,
			activeTab: this.activeTab,
			registry: Object.fromEntries(this.registry)
		});
	}
}

// ============================================================================
// Export singleton
// ============================================================================

export const spaceStore = new SpaceStore();

// Expose to window for debugging
if (typeof window !== 'undefined') {
	(window as unknown as { spaceStore: SpaceStore }).spaceStore = spaceStore;
}
