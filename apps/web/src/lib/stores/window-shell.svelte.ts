/**
 * Window Shell Store
 *
 * The tab/window/navigation shell. SINGLE source of truth for panes, splits,
 * tabs, URL sync, and the entity metadata registry.
 *
 * Architecture: Always-Panes Model
 * - Every tab lives in a pane
 * - Single-pane mode = panes.length === 1
 * - This eliminates 30+ conditional checks for split mode
 *
 * Features:
 * - Tabs (with full split-screen support)
 * - Entity metadata registry (lazy-loaded cache)
 */

import {
	type ViewEntity
} from '$lib/api/client';
import {
	type Tab,
	type TabType,
	type PaneState,
	entityIdToRoute,
	routeToEntityId
} from '$lib/tabs/types';
import { parseRoute } from '$lib/tabs/registry';
import { pushState, replaceState } from '$app/navigation';

// Re-export types for convenience
export type { Tab, TabType, PaneState };
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
	day: { type: 'day', icon: 'ri:calendar-line', routePrefix: '/day' },
	year: { type: 'year', icon: 'ri:calendar-line', routePrefix: '/year' },
	source: { type: 'source', icon: 'ri:database-2-line', routePrefix: '/sources' },
	file: { type: 'drive', icon: 'ri:file-line', routePrefix: '/drive' },
	space: { type: 'space', icon: 'ri:layout-masonry-line', routePrefix: '/space' }
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
const TAB_STORAGE_VERSION = 9; // System sections moved from DB to frontend constants
const WORKSPACE_STORAGE_KEY = 'virtues-active-workspace'; // Legacy — only used by migration cleanup

class WindowShellStore {
	// ============================================================================
	// Shell scope key (single workspace — always space_system)
	// ============================================================================
	activeShellId = $state<string>('space_system');

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
	// Swipe progress — kept as no-op to avoid breaking UnifiedSidebar touch handlers.
	// With a single space there's nothing to swipe to, so this never changes.
	swipeProgress = $state(0);

	smartSectionCache = $state<Map<string, ViewEntity[]>>(new Map());
	viewCacheVersion = $state<number>(0); // Incremented when cache is invalidated
	registry = $state<Map<string, EntityMetadata>>(new Map());

	private initialized = false;
	private urlSyncEnabled = false;
	private _skipUrlSync = false;

	// ============================================================================
	// Shell scope getters
	// ============================================================================
	// Single-workspace model: the shell is always scoped to space_system.
	get isSystemSpace(): boolean {
		return this.activeShellId === 'space_system';
	}

	// ============================================================================
	// Initialization
	// ============================================================================

	async init(): Promise<void> {
		if (this.initialized) return;
		if (typeof window === 'undefined') return;

		this.initialized = true;

		try {
			this.restoreTabState();
		} catch (e) {
			console.error('[WindowShellStore] Failed to initialize:', e);
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
	// Smart Section Cache (sidebar Chats/Pages live links)
	// ============================================================================

	/**
	 * Invalidate the smart section cache.
	 * Use this when entities are created/updated/deleted to refresh smart sections.
	 * @param namespace - Optional namespace (e.g., 'chat', 'page'). When omitted,
	 *                    clears the entire smart section cache.
	 */
	invalidateViewCache(namespace?: string): void {
		if (!namespace) {
			this.smartSectionCache = new Map();
			this.viewCacheVersion++;
			return;
		}

		// System sections re-fetch on version bump.
		this.viewCacheVersion++;
	}

	/**
	 * Update the smart section cache (called by SystemSection component)
	 */
	updateSmartSectionCache(sectionId: string, entities: ViewEntity[]): void {
		const newCache = new Map(this.smartSectionCache);
		newCache.set(sectionId, entities);
		this.smartSectionCache = newCache;
	}

	// ============================================================================
	// Entity Registry
	// ============================================================================

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
		// Single global key — multi-space carousel removed.
		return TAB_STORAGE_KEY_PREFIX;
	}

	private persistTabState(): void {
		if (typeof window === 'undefined') return;

		const data = {
			version: TAB_STORAGE_VERSION,
			panes: this.panes,
			activePaneId: this.activePaneId,
		};

		try {
			localStorage.setItem(this.getTabStorageKey(), JSON.stringify(data));
		} catch (e) {
			console.warn('[WindowShellStore] Failed to persist tab state:', e);
		}
	}

	private restoreTabState(): void {
		if (typeof window === 'undefined') return;

		const storageKey = this.getTabStorageKey();

		try {
			// One-time migration: if the global key doesn't exist but a per-space
			// key does, adopt the most recent one and clean up the rest.
			if (!localStorage.getItem(storageKey)) {
				this.migratePerSpaceTabKeys(storageKey);
			}

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
								console.warn(`[WindowShellStore] Removing duplicate tab: ${tab.id}`);
								return false;
							}
							seenIds.add(tab.id);
							return true;
						});
						return { ...pane, tabs: uniqueTabs };
					});

					this.panes = deduplicatedPanes;
					this.activePaneId = data.activePaneId || 'left';
					return;
				}

				// Older versions - clear and start fresh (clean slate approach)
				localStorage.removeItem(storageKey);
			}
		} catch (e) {
			console.warn('[WindowShellStore] Failed to restore tab state:', e);
		}

		// Default: single pane with no tabs
		this.panes = [{ id: 'left', tabs: [], activeTabId: null, width: 100 }];
		this.activePaneId = 'left';
		this.openDefaultTab();
	}

	private openDefaultTab(): void {
		// Always open a new chat when there are no tabs
		this.openTab({ type: 'chat', label: 'New Chat', route: '/chat', icon: 'ri:chat-1-line' });
	}

	/**
	 * One-time migration: adopt the most recent per-space tab key and
	 * clean up all old per-space keys. Only runs once (when the global
	 * key doesn't exist yet).
	 */
	private migratePerSpaceTabKeys(globalKey: string): void {
		const prefix = `${TAB_STORAGE_KEY_PREFIX}-`;
		let bestKey: string | null = null;
		let bestVersion = -1;

		for (let i = 0; i < localStorage.length; i++) {
			const key = localStorage.key(i);
			if (!key || !key.startsWith(prefix)) continue;

			try {
				const data = JSON.parse(localStorage.getItem(key) || '');
				const version = data.version ?? 0;
				if (version > bestVersion) {
					bestVersion = version;
					bestKey = key;
				}
			} catch {
				// skip malformed entries
			}
		}

		if (bestKey) {
			const data = localStorage.getItem(bestKey);
			if (data) {
				localStorage.setItem(globalKey, data);
			}
		}

		// Clean up all per-space keys
		const toRemove: string[] = [];
		for (let i = 0; i < localStorage.length; i++) {
			const key = localStorage.key(i);
			if (key && key.startsWith(prefix)) {
				toRemove.push(key);
			}
		}
		for (const key of toRemove) {
			localStorage.removeItem(key);
		}

		// Also clean up the workspace selector key
		localStorage.removeItem(WORKSPACE_STORAGE_KEY);
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
		// Use normalized route if available (e.g., /day → /day/day_2026-01-25)
		const effectiveRoute = parsed.normalizedRoute || route;

		// Find existing tab if not forcing new — focus it (IDE model)
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
				// If the existing list-view tab has a sibling sub-route (e.g. /actions/templates
				// vs. /actions), refresh its route so URL/state reflect the requested sub-tab.
				if (result.tab.route !== effectiveRoute) {
					this.updateTab(result.tab.id, { route: effectiveRoute });
				}
				this.setActiveTab(result.tab.id);
				return result.tab.id;
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

	/**
	 * Open a tab in the opposite pane — auto-enables split, dedupes by route.
	 *
	 * Generic helper used by `openChatContext` and anywhere else that needs
	 * the "click something, see its detail beside the list" pattern.
	 * Returns the tab id (new or existing).
	 */
	openAside(input: Omit<Tab, 'id' | 'createdAt'>): string {
		// Dedupe: if a tab for this route is already open anywhere, activate it.
		const existing = this.findTab((t) => t.route === input.route);
		if (existing) {
			this.setActiveTab(existing.tab.id);
			return existing.tab.id;
		}

		// Target the *other* pane from the currently active one.
		const targetPaneId: 'left' | 'right' =
			this.activePaneId === 'right' ? 'left' : 'right';

		if (!this.isSplit) {
			this.enableSplit();
		}

		return this.openTab(input, targetPaneId);
	}

	openChatContext(chatId: string, _currentPaneId: 'left' | 'right' | null): string {
		const entityId = chatId.startsWith('chat_') ? chatId : `chat_${chatId}`;
		return this.openAside({
			type: 'chat',
			label: 'Context',
			route: `/chat/${entityId}?view=context`,
			icon: 'ri:information-line'
		});
	}

	// ============================================================================
	// Debug
	// ============================================================================

	debug(): void {
		console.log('[WindowShellStore Debug]', {
			activeShellId: this.activeShellId,
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

export const windowShellStore = new WindowShellStore();

// Expose to window for debugging
if (typeof window !== 'undefined') {
	(window as unknown as { windowShellStore: WindowShellStore }).windowShellStore = windowShellStore;
}
