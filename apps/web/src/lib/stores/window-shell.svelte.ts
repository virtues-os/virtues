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
import { mobileLayout } from '$lib/stores/mobileLayout.svelte';

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
	notebook: { type: 'notebook', icon: 'ri:booklet-line', routePrefix: '/notebook' }
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
const TAB_STORAGE_VERSION = 10; // Per-tab navigation history (browser model)
const HISTORY_CAP = 50; // Max routes kept in a single tab's history stack

// Input shape for tab creation — history/historyIndex are seeded internally.
type TabInput = Omit<Tab, 'id' | 'createdAt' | 'history' | 'historyIndex'>;
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
				// Walk per-tab history when the URL is an adjacent entry (OS back/forward
				// mirrors the in-app buttons); otherwise focus-existing / restore.
				this.reconcilePaneToRoute('left', path);
			}

			if (rightRoute && mobileLayout.isMobile) {
				// No split on the phone shell — a ?right= deep link opens the
				// right-hand route as a normal tab instead.
				this.openTabFromRoute(rightRoute, { focusExisting: true });
			} else if (rightRoute) {
				if (!this.isSplit) {
					this.enableSplit();
				}
				this.reconcilePaneToRoute('right', rightRoute);
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

		// Preserve route-level params (e.g. ?page=N) — only ?right= is the shell's.
		const routeParams = new URLSearchParams(searchParams);
		routeParams.delete('right');
		const routeWithParams = routeParams.size > 0 ? `${path}?${routeParams}` : path;

		this.handleDeepLink(routeWithParams, rightRoute);
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

				// v9 (no per-tab history) and v10 both migrate forward: dedup tab ids
				// and seed each tab with a history stack. Only older formats clean-slate.
				if (data.version >= 9 && Array.isArray(data.panes)) {
					const migratedPanes = data.panes.map((pane: PaneState) => {
						const seenIds = new Set<string>();
						const uniqueTabs = pane.tabs
							.filter((tab: Tab) => {
								if (seenIds.has(tab.id)) {
									console.warn(`[WindowShellStore] Removing duplicate tab: ${tab.id}`);
									return false;
								}
								seenIds.add(tab.id);
								return true;
							})
							.map((tab: Tab) => this.ensureHistory(tab));
						return { ...pane, tabs: uniqueTabs };
					});

					this.panes = migratedPanes;
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
		// Fresh sessions land on Home (the "Return" surface), not an empty chat.
		this.openTab({ type: 'home', label: 'Home', route: '/home', icon: 'ri:home-5-line' });
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

	openTab(input: TabInput, paneId?: string): string {
		const id = crypto.randomUUID();
		// Seed the history stack with the tab's opening route.
		const tab: Tab = {
			...input,
			id,
			history: [input.route],
			historyIndex: 0,
			createdAt: Date.now()
		};
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

	/**
	 * Route → tab dispatcher. Behavior depends on options:
	 * - default          → navigate the active tab IN PLACE (browser model)
	 * - forceNew: true   → always create a new tab
	 * - focusExisting    → focus an already-open matching tab, else create (IDE model;
	 *                       used by deep-link / popstate restore)
	 */
	openTabFromRoute(route: string, options?: {
		label?: string;
		forceNew?: boolean;
		focusExisting?: boolean;
		preferEmptyPane?: boolean;
		paneId?: 'left' | 'right';
	}): string {
		if (options?.forceNew) {
			return this.createTabFromRoute(route, options);
		}

		if (options?.focusExisting) {
			const parsed = parseRoute(route);
			const effectiveRoute = parsed.normalizedRoute || route;

			let result: { tab: Tab; paneId: string } | undefined;
			if (parsed.entityId) {
				result = this.findTab((t) => t.route === effectiveRoute);
			} else if (parsed.virtuesPage) {
				result = this.findTab((t) => t.type === 'virtues' && t.virtuesPage === parsed.virtuesPage);
			} else if (parsed.storagePath) {
				result = this.findTab((t) => t.type === 'drive' && t.storagePath === parsed.storagePath);
			} else {
				result = this.findTab((t) => t.type === parsed.type && !t.virtuesPage && !t.storagePath && !routeToEntityId(t.route));
			}

			if (result) {
				if (result.tab.route !== effectiveRoute) {
					this.updateTab(result.tab.id, { route: effectiveRoute });
				}
				this.setActiveTab(result.tab.id);
				return result.tab.id;
			}
			return this.createTabFromRoute(route, options);
		}

		// Default: navigate the active tab in place.
		return this.navigate(route, {
			label: options?.label,
			paneId: options?.paneId,
			preferEmptyPane: options?.preferEmptyPane
		});
	}

	/**
	 * Navigate the active tab of the target pane IN PLACE — swap its content and
	 * push onto its history stack (browser model). Falls back to creating a tab
	 * when the target pane has no active tab (e.g. an empty split pane).
	 */
	navigate(route: string, options?: {
		label?: string;
		paneId?: 'left' | 'right';
		preferEmptyPane?: boolean;
	}): string {
		const { effectiveRoute, fields } = this.identityFromRoute(route, options?.label);

		let targetPaneId = options?.paneId ?? this.activePaneId;
		if (options?.preferEmptyPane && this.isSplit) {
			if (this.panes[0].tabs.length === 0) targetPaneId = 'left';
			else if (this.panes[1]?.tabs.length === 0) targetPaneId = 'right';
		}

		const pane = this.panes.find(p => p.id === targetPaneId);
		const activeTab = pane?.tabs.find(t => t.id === pane.activeTabId);

		// No tab to navigate — create one instead.
		if (!pane || !activeTab) {
			return this.createTabFromRoute(route, { label: options?.label, paneId: options?.paneId, preferEmptyPane: options?.preferEmptyPane });
		}

		// Truncate any forward history, push the new route, cap depth.
		const base = activeTab.history.slice(0, activeTab.historyIndex + 1);
		base.push(effectiveRoute);
		const { history, index } = this.capHistory(base, base.length - 1);

		this.updatePane(targetPaneId, p => ({
			...p,
			tabs: p.tabs.map(t => t.id === activeTab.id ? {
				...t,
				...fields,
				route: effectiveRoute,
				history,
				historyIndex: index,
				scrollPosition: 0
			} : t)
		}));

		this.activePaneId = targetPaneId;
		this.persistTabState();
		this.syncActiveToUrl(true);
		return activeTab.id;
	}

	/** Move the active tab of a pane back one step in its history. */
	goBack(paneId?: string): void {
		const targetPaneId = paneId ?? this.activePaneId;
		const pane = this.panes.find(p => p.id === targetPaneId);
		const tab = pane?.tabs.find(t => t.id === pane.activeTabId);
		if (!tab) return;
		this.applyHistoryIndex(targetPaneId, tab.id, tab.historyIndex - 1);
	}

	/** Move the active tab of a pane forward one step in its history. */
	goForward(paneId?: string): void {
		const targetPaneId = paneId ?? this.activePaneId;
		const pane = this.panes.find(p => p.id === targetPaneId);
		const tab = pane?.tabs.find(t => t.id === pane.activeTabId);
		if (!tab) return;
		this.applyHistoryIndex(targetPaneId, tab.id, tab.historyIndex + 1);
	}

	canGoBack(paneId?: string): boolean {
		const targetPaneId = paneId ?? this.activePaneId;
		const pane = this.panes.find(p => p.id === targetPaneId);
		const tab = pane?.tabs.find(t => t.id === pane.activeTabId);
		return !!tab && tab.historyIndex > 0;
	}

	canGoForward(paneId?: string): boolean {
		const targetPaneId = paneId ?? this.activePaneId;
		const pane = this.panes.find(p => p.id === targetPaneId);
		const tab = pane?.tabs.find(t => t.id === pane.activeTabId);
		return !!tab && tab.historyIndex < tab.history.length - 1;
	}

	// ── History helpers ──────────────────────────────────────────────────────

	/** Parse a route into the identity fields a tab carries. */
	private identityFromRoute(route: string, label?: string): {
		effectiveRoute: string;
		fields: Pick<Tab, 'type' | 'label' | 'icon' | 'storagePath' | 'virtuesPage'>;
	} {
		const parsed = parseRoute(route);
		const effectiveRoute = parsed.normalizedRoute || route;
		return {
			effectiveRoute,
			fields: {
				type: parsed.type,
				label: label || parsed.label,
				icon: parsed.icon,
				storagePath: parsed.storagePath,
				virtuesPage: parsed.virtuesPage
			}
		};
	}

	/** Cap a history stack to HISTORY_CAP, dropping from the front. */
	private capHistory(history: string[], index: number): { history: string[]; index: number } {
		if (history.length <= HISTORY_CAP) return { history, index };
		const overflow = history.length - HISTORY_CAP;
		return { history: history.slice(overflow), index: Math.max(0, index - overflow) };
	}

	/** Create a brand-new tab from a route (honors preferEmptyPane in split). */
	private createTabFromRoute(route: string, options?: {
		label?: string;
		preferEmptyPane?: boolean;
		paneId?: 'left' | 'right';
	}): string {
		const { effectiveRoute, fields } = this.identityFromRoute(route, options?.label);
		let targetPaneId = options?.paneId ?? this.activePaneId;
		if (options?.preferEmptyPane && this.isSplit) {
			if (this.panes[0].tabs.length === 0) targetPaneId = 'left';
			else if (this.panes[1]?.tabs.length === 0) targetPaneId = 'right';
		}
		return this.openTab({ ...fields, route: effectiveRoute }, targetPaneId);
	}

	/** Move a specific tab to a history index, re-deriving its identity fields. */
	private applyHistoryIndex(paneId: string, tabId: string, newIndex: number): void {
		const pane = this.panes.find(p => p.id === paneId);
		const tab = pane?.tabs.find(t => t.id === tabId);
		if (!tab) return;
		if (newIndex < 0 || newIndex >= tab.history.length) return;

		const route = tab.history[newIndex];
		const { fields } = this.identityFromRoute(route);

		this.updatePane(paneId, p => ({
			...p,
			tabs: p.tabs.map(t => t.id === tabId ? {
				...t,
				...fields,
				route,
				historyIndex: newIndex,
				scrollPosition: 0
			} : t)
		}));

		this.persistTabState();
		this.syncActiveToUrl(true);
	}

	/**
	 * Reconcile a pane's active tab to a route arriving from the URL (popstate).
	 * If the route is an adjacent history entry, walk the index (so OS back/forward
	 * mirror the in-app buttons); otherwise focus-existing or create.
	 */
	private reconcilePaneToRoute(paneId: 'left' | 'right', route: string): void {
		const pane = this.panes.find(p => p.id === paneId);
		const tab = pane?.tabs.find(t => t.id === pane.activeTabId);
		if (pane && tab) {
			const parsed = parseRoute(route);
			const effectiveRoute = parsed.normalizedRoute || route;
			if (tab.route === effectiveRoute) return;
			if (tab.history[tab.historyIndex - 1] === effectiveRoute) {
				this.applyHistoryIndex(paneId, tab.id, tab.historyIndex - 1);
				return;
			}
			if (tab.history[tab.historyIndex + 1] === effectiveRoute) {
				this.applyHistoryIndex(paneId, tab.id, tab.historyIndex + 1);
				return;
			}
		}
		// Not adjacent (or no active tab) — restore by focusing/creating.
		this.openTabFromRoute(route, { focusExisting: true, paneId });
	}

	/** Normalize a persisted tab so it always carries a valid history stack. */
	private ensureHistory(tab: Tab): Tab {
		const history = Array.isArray(tab.history) && tab.history.length > 0 ? tab.history : [tab.route];
		let index = typeof tab.historyIndex === 'number' ? tab.historyIndex : history.length - 1;
		index = Math.max(0, Math.min(index, history.length - 1));
		return { ...tab, history, historyIndex: index, route: history[index] ?? tab.route };
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
			tabs: p.tabs.map(t => {
				if (t.id !== tabId) return t;
				const next = { ...t, ...updates };
				// A route change here is an identity refinement of the SAME viewport
				// (e.g. new chat "/" → "/chat/chat_xyz"), not a navigation. Replace the
				// current history slot in place rather than pushing a new entry, so the
				// back button never lands on the pre-refinement route.
				if (routeChanged && updates.route) {
					const history = [...t.history];
					history[t.historyIndex] = updates.route;
					next.history = history;
				}
				return next;
			})
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
		// The phone shell shows a single pane; split has no touch affordances
		// there (resize handle is hover/mouse-only), so refuse to enter it.
		if (mobileLayout.isMobile) return;
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

	/**
	 * Open a route in the pane BESIDE the active one, splitting if needed.
	 * This is the "open" gesture for references (⌘-click / embed click): it keeps
	 * the pane you were working in visible instead of burying it behind a tab.
	 */
	openRouteBeside(route: string, label?: string): string {
		const other: 'left' | 'right' = this.activePaneId === 'right' ? 'left' : 'right';
		if (!this.isSplit) this.enableSplit();
		return this.openTabFromRoute(route, { paneId: other, forceNew: true, label });
	}

	// Backwards compatibility aliases
	openTabInPane(input: TabInput, paneId: 'left' | 'right'): string {
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
	openAside(input: TabInput): string {
		// Dedupe: if a tab for this route is already open anywhere, activate it.
		const existing = this.findTab((t) => t.route === input.route);
		if (existing) {
			this.setActiveTab(existing.tab.id);
			return existing.tab.id;
		}

		// No split on the phone shell — "beside" degrades to a normal tab.
		if (mobileLayout.isMobile) {
			return this.openTab(input, this.activePaneId);
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
