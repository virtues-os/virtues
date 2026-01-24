/**
 * WindowTabs Store
 * 
 * Manages the tab state for the application. Uses the tabs module for:
 * - Type definitions (tabs/types.ts)
 * - Route parsing (tabs/registry.ts)
 * - URL serialization (tabs/urlSerializer.ts)
 */

// Import from the tabs module
import {
	type Tab,
	type TabType,
	type FallbackView,
	type PaneState,
	type SplitState,
	getTabDomain,
} from '$lib/tabs/types';
import { parseRoute } from '$lib/tabs/registry';
import {
	serializeToUrl as serializeStateToUrl,
	deserializeFromUrl as deserializeStateFromUrl,
	hasUrlTabParams,
} from '$lib/tabs/urlSerializer';

// Re-export types for backwards compatibility
export type { Tab, TabType, FallbackView, PaneState, SplitState };
export { parseRoute };

const STORAGE_KEY = 'virtues-window-tabs';
const STORAGE_VERSION = 2; // Increment to force migration

type TabInput = Omit<Tab, 'id' | 'createdAt'>;

class WindowTabsStore {
	tabs = $state<Tab[]>([]);
	activeTabId = $state<string | null>(null);
	private initialized = false;

	// Fallback view preference (what to show when all tabs are closed)
	fallbackPreference = $state<FallbackView>('empty');

	// Guard to prevent URL sync feedback loop
	// Set to true when tabs are changed programmatically (e.g., sidebar click)
	// The layout's URL sync $effect should check this and skip if true
	private _skipUrlSync = false;

	// Navigation history (stores tab IDs)
	private _history = $state<string[]>([]);
	private _historyIndex = $state<number>(-1);
	private _isNavigatingHistory = false; // Prevents pushing to history during back/forward

	// Split screen state
	split = $state<SplitState>({
		enabled: false,
		panes: null,
		activePaneId: 'left'
	});

	// Global drag state for tab splitting UI
	isDragging = $state(false);

	// Check and clear the skip flag (call this in URL sync $effect)
	shouldSkipUrlSync(): boolean {
		if (this._skipUrlSync) {
			this._skipUrlSync = false;
			return true;
		}
		return false;
	}

	constructor() {
		// Note: During SSR, window is undefined. We use init() for client-side initialization.
		this.init();
	}

	// Initialize store - safe to call multiple times
	init(): void {
		if (this.initialized) return;
		if (typeof window === 'undefined') return;

		this.initialized = true;
		this.loadFallbackPreference();
		this.restore();
	}

	// Get active tab (works in both split and non-split modes)
	get activeTab(): Tab | undefined {
		if (this.split.enabled && this.split.panes) {
			const pane = this.split.panes.find((p) => p.id === this.split.activePaneId);
			if (pane) {
				return pane.tabs.find((t) => t.id === pane.activeTabId);
			}
			return undefined;
		}
		return this.tabs.find((t) => t.id === this.activeTabId);
	}

	// Get active tabs from ALL visible panes (for sidebar highlighting in split view)
	getActiveTabsForSidebar(): Tab[] {
		if (this.split.enabled && this.split.panes) {
			const activeTabs: Tab[] = [];
			for (const pane of this.split.panes) {
				const activeTab = pane.tabs.find((t) => t.id === pane.activeTabId);
				if (activeTab) {
					activeTabs.push(activeTab);
				}
			}
			return activeTabs;
		}
		const singleActiveTab = this.tabs.find((t) => t.id === this.activeTabId);
		return singleActiveTab ? [singleActiveTab] : [];
	}

	// Get all tabs regardless of split mode
	getAllTabs(): Tab[] {
		if (this.split.enabled && this.split.panes) {
			return [...this.split.panes[0].tabs, ...this.split.panes[1].tabs];
		}
		return this.tabs;
	}

	// Find a tab by predicate, checking both panes if split
	findTab(predicate: (tab: Tab) => boolean): { tab: Tab; paneId: 'left' | 'right' | null } | undefined {
		if (this.split.enabled && this.split.panes) {
			const inLeft = this.split.panes[0].tabs.find(predicate);
			if (inLeft) return { tab: inLeft, paneId: 'left' };
			const inRight = this.split.panes[1].tabs.find(predicate);
			if (inRight) return { tab: inRight, paneId: 'right' };
			return undefined;
		}
		const found = this.tabs.find(predicate);
		return found ? { tab: found, paneId: null } : undefined;
	}

	// Find which pane a tab is in
	findTabPane(tabId: string): 'left' | 'right' | null {
		if (!this.split.enabled || !this.split.panes) return null;
		if (this.split.panes[0].tabs.some((t) => t.id === tabId)) return 'left';
		if (this.split.panes[1].tabs.some((t) => t.id === tabId)) return 'right';
		return null;
	}

	// Open a new tab and make it active
	openTab(input: TabInput): string {
		const id = crypto.randomUUID();
		const tab: Tab = {
			...input,
			id,
			createdAt: Date.now()
		};

		this.tabs = [...this.tabs, tab];
		this.activeTabId = id;
		this.pushToHistory(id);
		this.persist();

		return id;
	}

	// Close a tab (works in both split and non-split modes)
	closeTab(id: string): void {
		// In split mode, delegate to pane-specific close
		if (this.split.enabled && this.split.panes) {
			const paneId = this.findTabPane(id);
			if (paneId) {
				this.closeTabInPane(id, paneId);
			}
			return;
		}

		const index = this.tabs.findIndex((t) => t.id === id);
		if (index === -1) return;

		// If closing the active tab, activate an adjacent one
		if (this.activeTabId === id) {
			if (this.tabs.length === 1) {
				// Last tab - check fallback preference
				this.tabs = [];
				this.activeTabId = null;
				
				const pref = this.fallbackPreference;
				if (pref === 'chat') {
					this.openTabFromRoute('/');
				} else if (pref === 'conway') {
					this.openTabFromRoute('/life');
				} else if (pref === 'dog-jump') {
					this.openTabFromRoute('/jump');
				} else if (pref === 'wiki-today') {
					const today = new Date().toISOString().split('T')[0];
					this.openTabFromRoute(`/wiki/${today}`);
				}
				// If 'empty', stay at 0 tabs - SplitContainer shows DiscoveryPage
				return;
			} else if (index === this.tabs.length - 1) {
				// Last in list - activate previous
				this.activeTabId = this.tabs[index - 1].id;
			} else {
				// Activate next
				this.activeTabId = this.tabs[index + 1].id;
			}
		}

		this.tabs = this.tabs.filter((t) => t.id !== id);
		this.persist();
	}

	// Close all tabs except the specified one
	closeOtherTabs(id: string, paneId?: 'left' | 'right'): void {
		if (this.split.enabled && this.split.panes && paneId) {
			const paneIndex = paneId === 'left' ? 0 : 1;
			const pane = this.split.panes[paneIndex];
			const tabToKeep = pane.tabs.find((t) => t.id === id);
			if (!tabToKeep) return;

			const newPanes = [...this.split.panes] as [PaneState, PaneState];
			newPanes[paneIndex] = {
				...newPanes[paneIndex],
				tabs: [tabToKeep],
				activeTabId: id
			};
			this.split = { ...this.split, panes: newPanes };
			this.persist();
			return;
		}

		const tabToKeep = this.tabs.find((t) => t.id === id);
		if (!tabToKeep) return;

		this.tabs = [tabToKeep];
		this.activeTabId = id;
		this.persist();
	}

	// Close all tabs to the right of the specified one
	closeTabsToRight(id: string, paneId?: 'left' | 'right'): void {
		if (this.split.enabled && this.split.panes && paneId) {
			const paneIndex = paneId === 'left' ? 0 : 1;
			const pane = this.split.panes[paneIndex];
			const index = pane.tabs.findIndex((t) => t.id === id);
			if (index === -1) return;

			const newTabs = pane.tabs.slice(0, index + 1);
			const newActiveId = newTabs.some((t) => t.id === pane.activeTabId)
				? pane.activeTabId
				: newTabs[newTabs.length - 1]?.id || null;

			const newPanes = [...this.split.panes] as [PaneState, PaneState];
			newPanes[paneIndex] = {
				...newPanes[paneIndex],
				tabs: newTabs,
				activeTabId: newActiveId
			};
			this.split = { ...this.split, panes: newPanes };
			this.persist();
			return;
		}

		const index = this.tabs.findIndex((t) => t.id === id);
		if (index === -1) return;

		const newTabs = this.tabs.slice(0, index + 1);
		if (!newTabs.some((t) => t.id === this.activeTabId)) {
			this.activeTabId = newTabs[newTabs.length - 1]?.id || null;
		}
		this.tabs = newTabs;
		this.persist();
	}

	// Push a tab ID to navigation history
	private pushToHistory(tabId: string): void {
		if (this._isNavigatingHistory) return;

		// If we're not at the end of history, truncate forward history
		if (this._historyIndex < this._history.length - 1) {
			this._history = this._history.slice(0, this._historyIndex + 1);
		}

		// Don't push duplicates
		if (this._history[this._history.length - 1] !== tabId) {
			this._history = [...this._history, tabId];
			this._historyIndex = this._history.length - 1;
		}
	}

	// Check if we can go back in history
	canGoBack(): boolean {
		return this._historyIndex > 0;
	}

	// Check if we can go forward in history
	canGoForward(): boolean {
		return this._historyIndex < this._history.length - 1;
	}

	// Go back in navigation history
	goBack(): void {
		if (!this.canGoBack()) return;

		this._isNavigatingHistory = true;
		this._historyIndex--;
		const tabId = this._history[this._historyIndex];

		// Find and activate the tab
		const result = this.findTab((t) => t.id === tabId);
		if (result) {
			if (result.paneId) {
				this.setActiveTabInPane(result.tab.id, result.paneId);
			} else {
				this.activeTabId = result.tab.id;
				this.persist();
			}
		}

		this._isNavigatingHistory = false;
	}

	// Go forward in navigation history
	goForward(): void {
		if (!this.canGoForward()) return;

		this._isNavigatingHistory = true;
		this._historyIndex++;
		const tabId = this._history[this._historyIndex];

		// Find and activate the tab
		const result = this.findTab((t) => t.id === tabId);
		if (result) {
			if (result.paneId) {
				this.setActiveTabInPane(result.tab.id, result.paneId);
			} else {
				this.activeTabId = result.tab.id;
				this.persist();
			}
		}

		this._isNavigatingHistory = false;
	}

	// Set the active tab (works in both split and non-split modes)
	setActiveTab(id: string): void {
		console.log('[WindowTabs] setActiveTab:', id, 'split.enabled:', this.split.enabled);

		if (this.split.enabled && this.split.panes) {
			// In split mode, find which pane the tab is in and activate there
			const paneId = this.findTabPane(id);
			console.log('[WindowTabs] Split mode, found in pane:', paneId);
			if (paneId) {
				this.setActiveTabInPane(id, paneId);
			}
			return;
		}

		const found = this.tabs.some((t) => t.id === id);
		console.log('[WindowTabs] Tab found in tabs array:', found, 'tabs.length:', this.tabs.length);
		if (found) {
			this.pushToHistory(id);
			this.activeTabId = id;
			this.persist();
		}
	}

	// Update a tab's properties (works in both split and non-split modes)
	updateTab(id: string, updates: Partial<Omit<Tab, 'id' | 'createdAt'>>): void {
		// If updating the route, set skip flag to prevent URL sync feedback loop
		if (updates.route) {
			this._skipUrlSync = true;
		}

		if (this.split.enabled && this.split.panes) {
			const paneId = this.findTabPane(id);
			if (paneId) {
				const paneIndex = paneId === 'left' ? 0 : 1;
				const newPanes = [...this.split.panes] as [PaneState, PaneState];
				newPanes[paneIndex] = {
					...newPanes[paneIndex],
					tabs: newPanes[paneIndex].tabs.map((t) => (t.id === id ? { ...t, ...updates } : t))
				};
				this.split = { ...this.split, panes: newPanes };
				this.persist();
			}
			return;
		}

		this.tabs = this.tabs.map((t) => (t.id === id ? { ...t, ...updates } : t));
		this.persist();
	}

	// Toggle pin status for a tab
	togglePin(id: string): void {
		const tab = this.getAllTabs().find((t) => t.id === id);
		if (!tab) return;

		this.updateTab(id, { pinned: !tab.pinned });

		// Re-sort tabs to put pinned tabs first
		if (this.split.enabled && this.split.panes) {
			const paneId = this.findTabPane(id);
			if (paneId) {
				const paneIndex = paneId === 'left' ? 0 : 1;
				const pane = this.split.panes[paneIndex];
				const sortedTabs = [...pane.tabs].sort((a, b) => {
					if (a.pinned && !b.pinned) return -1;
					if (!a.pinned && b.pinned) return 1;
					return 0;
				});
				const newPanes = [...this.split.panes] as [PaneState, PaneState];
				newPanes[paneIndex] = { ...newPanes[paneIndex], tabs: sortedTabs };
				this.split = { ...this.split, panes: newPanes };
				this.persist();
			}
		} else {
			this.tabs = [...this.tabs].sort((a, b) => {
				if (a.pinned && !b.pinned) return -1;
				if (!a.pinned && b.pinned) return 1;
				return 0;
			});
			this.persist();
		}
	}

	// Open in a new tab (convenience method)
	openInNewTab(route: string, type: Tab['type'], label: string, icon?: string): string {
		return this.openTab({ type, label, route, icon });
	}

	// Open or focus an existing tab by conversationId
	openOrFocusChat(conversationId: string, label?: string): void {
		const existing = this.tabs.find((t) => t.conversationId === conversationId);

		if (existing) {
			this.setActiveTab(existing.id);
		} else {
			this.openTab({
				type: 'chat',
				label: label || 'Chat',
				route: `/?conversationId=${conversationId}`,
				conversationId,
				icon: 'ri:chat-1-line'
			});
		}
	}

	// Open or focus a tab by route (for non-chat tabs)
	openOrFocusRoute(route: string, type: Tab['type'], label: string, icon?: string): void {
		const existing = this.tabs.find((t) => t.route === route);

		if (existing) {
			this.setActiveTab(existing.id);
		} else {
			this.openTab({ type, label, route, icon });
		}
	}

	// Open a tab from a route string, parsing it to determine type and metadata
	openTabFromRoute(route: string, options?: { forceNew?: boolean; label?: string; preferEmptyPane?: boolean; paneId?: 'left' | 'right' }): string {
		// Set skip flag to prevent URL sync from reverting this change
		this._skipUrlSync = true;

		const parsed = parseRoute(route);
		const targetDomain = getTabDomain(parsed.type);

		// Find existing tab based on type-specific matching (works in split and non-split modes)
		if (!options?.forceNew) {
			let result: { tab: Tab; paneId: 'left' | 'right' | null } | undefined;

			if (parsed.type === 'chat' && parsed.conversationId) {
				// Match chat by conversationId
				result = this.findTab((t) => t.conversationId === parsed.conversationId);
			} else if (parsed.type === 'page-detail' && parsed.pageId) {
				// Match page by pageId
				result = this.findTab((t) => t.type === 'page-detail' && t.pageId === parsed.pageId);
			} else if (parsed.type === 'wiki' && parsed.slug) {
				// Match wiki by slug
				result = this.findTab((t) => t.type === 'wiki' && t.slug === parsed.slug);
			} else if (parsed.type === 'wiki-list' && parsed.wikiCategory) {
				// Match wiki-list by category
				result = this.findTab((t) => t.type === 'wiki-list' && t.wikiCategory === parsed.wikiCategory);
			} else if (parsed.type === 'data-sources' && parsed.sourceId) {
				// Match data-source detail by sourceId
				result = this.findTab((t) => t.type === 'data-sources' && t.sourceId === parsed.sourceId);
			} else if (parsed.type === 'profile' && parsed.profileSection) {
				// Match profile by section
				result = this.findTab((t) => t.type === 'profile' && t.profileSection === parsed.profileSection);
			} else {
				// For singleton routes (history, usage, etc.), match by type
				result = this.findTab((t) => t.type === parsed.type && !t.conversationId && !t.slug && !t.sourceId);
			}

			if (result) {
				if (result.paneId) {
					// In split mode, activate in the pane where found
					this.setActiveTabInPane(result.tab.id, result.paneId);
				} else {
					// Non-split mode
					this.setActiveTab(result.tab.id);
				}
				return result.tab.id;
			}

			// Hybrid navigation: if current tab is same domain, navigate in place
			const currentTab = this.activeTab;
			if (currentTab && !currentTab.pinned) {
				const currentDomain = getTabDomain(currentTab.type);
				if (currentDomain === targetDomain) {
					// Same domain - update current tab in place
					const updates: Partial<Tab> = {
						type: parsed.type,
						label: options?.label || parsed.label,
						route,
						icon: parsed.icon,
						conversationId: parsed.conversationId,
						pageId: parsed.pageId,
						slug: parsed.slug,
						sourceId: parsed.sourceId,
						wikiCategory: parsed.wikiCategory,
						profileSection: parsed.profileSection
					};
					this.updateTab(currentTab.id, updates);
					return currentTab.id;
				}
			}
		}

		// Create new tab - in split mode, prefer empty pane, otherwise active pane
		// Use provided label if available (e.g., from sidebar with chat title), otherwise use parsed label
		const tabInput = {
			type: parsed.type,
			label: options?.label || parsed.label,
			route,
			icon: parsed.icon,
			conversationId: parsed.conversationId,
			linkedConversationId: parsed.linkedConversationId,
			pageId: parsed.pageId,
			slug: parsed.slug,
			sourceId: parsed.sourceId,
			wikiCategory: parsed.wikiCategory,
			profileSection: parsed.profileSection
		};

		if (this.split.enabled && this.split.panes) {
			// If explicit paneId is provided, use it
			if (options?.paneId) {
				return this.openTabInPane(tabInput, options.paneId);
			}

			// If preferEmptyPane is true (e.g., from sidebar), prefer opening in empty pane
			if (options?.preferEmptyPane) {
				const leftPane = this.split.panes[0];
				const rightPane = this.split.panes[1];

				if (leftPane.tabs.length === 0) {
					return this.openTabInPane(tabInput, 'left');
				}
				if (rightPane.tabs.length === 0) {
					return this.openTabInPane(tabInput, 'right');
				}
			}

			// Otherwise open in active pane
			return this.openTabInPane(tabInput, this.split.activePaneId);
		}

		return this.openTab(tabInput);
	}

	// Sync tab state from URL (called when URL changes via browser navigation)
	syncFromUrl(route: string): void {
		const parsed = parseRoute(route);

		// Check if there's already a matching active tab
		const activeTab = this.activeTab;
		if (activeTab) {
			// If the active tab already matches the route, do nothing
			if (activeTab.route === route) return;

			// For chat tabs, check conversationId match
			if (parsed.type === 'chat' && activeTab.type === 'chat') {
				if (parsed.conversationId === activeTab.conversationId) return;
			}
		}

		// Open or focus the tab for this route
		this.openTabFromRoute(route);
	}

	// Get the route of the active tab (for URL syncing)
	get activeRoute(): string | null {
		return this.activeTab?.route || null;
	}

	// Update the active tab's route (used when external navigation like OAuth callbacks occur)
	updateActiveTabRoute(newRoute: string): void {
		const activeTab = this.activeTab;
		if (activeTab) {
			console.log('[WindowTabs] Updating active tab route:', { from: activeTab.route, to: newRoute });
			this.updateTab(activeTab.id, { route: newRoute });
		}
	}

	// Reorder tabs (for drag and drop)
	reorderTabs(fromIndex: number, toIndex: number): void {
		if (fromIndex === toIndex) return;
		if (fromIndex < 0 || fromIndex >= this.tabs.length) return;
		if (toIndex < 0 || toIndex >= this.tabs.length) return;

		const newTabs = [...this.tabs];
		const [moved] = newTabs.splice(fromIndex, 1);
		newTabs.splice(toIndex, 0, moved);
		this.tabs = newTabs;
		this.persist();
	}

	// Reorder tabs within a specific pane (for split mode drag and drop)
	reorderTabsInPane(fromIndex: number, toIndex: number, paneId: 'left' | 'right'): void {
		if (!this.split.enabled || !this.split.panes) return;

		const paneIndex = paneId === 'left' ? 0 : 1;
		const pane = this.split.panes[paneIndex];

		if (fromIndex === toIndex) return;
		if (fromIndex < 0 || fromIndex >= pane.tabs.length) return;
		if (toIndex < 0 || toIndex >= pane.tabs.length) return;

		const newTabs = [...pane.tabs];
		const [moved] = newTabs.splice(fromIndex, 1);
		newTabs.splice(toIndex, 0, moved);

		const newPanes = [...this.split.panes] as [PaneState, PaneState];
		newPanes[paneIndex] = { ...newPanes[paneIndex], tabs: newTabs };
		this.split = { ...this.split, panes: newPanes };
		this.persist();
	}

	// Save scroll position for a tab
	saveScrollPosition(id: string, position: number): void {
		this.updateTab(id, { scrollPosition: position });
	}

	// Persistence
	private persist(): void {
		if (typeof window === 'undefined') return;

		const data = {
			version: STORAGE_VERSION,
			tabs: this.tabs,
			activeTabId: this.activeTabId,
			split: this.split
		};

		try {
			localStorage.setItem(STORAGE_KEY, JSON.stringify(data));
		} catch (e) {
			console.warn('[WindowTabs] Failed to persist to localStorage:', e);
		}
	}

	private restore(): void {
		if (typeof window === 'undefined') return;

		try {
			const stored = localStorage.getItem(STORAGE_KEY);
			if (stored) {
				const data = JSON.parse(stored);

				// Migration: Clear stale state from older versions
				if (!data.version || data.version < STORAGE_VERSION) {
					console.log('[WindowTabs] Migrating from version', data.version || 1, 'to', STORAGE_VERSION);
					localStorage.removeItem(STORAGE_KEY);
					// Fall through to create fresh state
				} else {
					// Restore split state if present and valid
					if (data.split?.enabled && data.split.panes) {
						const leftTabs = data.split.panes[0]?.tabs || [];
						const rightTabs = data.split.panes[1]?.tabs || [];
						const totalTabs = leftTabs.length + rightTabs.length;

						// Only restore split mode if there are actual tabs
						if (totalTabs > 0) {
							this.split = data.split;
							// In split mode, tabs are stored in panes
							this.tabs = [];
							this.activeTabId = null;
							return;
						}
						// If split mode has no tabs, fall through to create fresh state
						console.warn('[WindowTabs] Split mode had no tabs, resetting state');
					}

					// Regular (non-split) restore
					if (Array.isArray(data.tabs) && data.tabs.length > 0) {
						this.tabs = data.tabs;
						this.activeTabId = data.activeTabId || data.tabs[0]?.id || null;
						// Ensure split is disabled
						this.split = { enabled: false, panes: null, activePaneId: 'left' };
						return;
					}
				}
			}
		} catch (e) {
			console.warn('[WindowTabs] Failed to restore from localStorage:', e);
		}

		// Default: use fallback preference (but default to chat for first-time users)
		this.tabs = [];
		this.activeTabId = null;
		this.split = { enabled: false, panes: null, activePaneId: 'left' };
		
		// For first-time users, default to chat. Otherwise respect preference.
		const pref = this.fallbackPreference;
		if (pref === 'chat' || pref === 'empty') {
			// First launch or empty preference - open chat as a sensible default
			this.openTab({ type: 'chat', label: 'New Chat', route: '/', icon: 'ri:chat-1-line' });
		} else if (pref === 'conway') {
			this.openTabFromRoute('/life');
		} else if (pref === 'dog-jump') {
			this.openTabFromRoute('/jump');
		} else if (pref === 'wiki-today') {
			const today = new Date().toISOString().split('T')[0];
			this.openTabFromRoute(`/wiki/${today}`);
		}
	}

	// Clear all tabs and reset (useful for testing/debugging)
	reset(): void {
		this.tabs = [];
		this.activeTabId = null;
		this.split = { enabled: false, panes: null, activePaneId: 'left' };
		localStorage.removeItem(STORAGE_KEY);
		
		// Respect fallback preference
		const pref = this.fallbackPreference;
		if (pref === 'chat') {
			this.openTab({ type: 'chat', label: 'New Chat', route: '/', icon: 'ri:chat-1-line' });
		} else if (pref === 'conway') {
			this.openTabFromRoute('/life');
		} else if (pref === 'dog-jump') {
			this.openTabFromRoute('/jump');
		} else if (pref === 'wiki-today') {
			const today = new Date().toISOString().split('T')[0];
			this.openTabFromRoute(`/wiki/${today}`);
		}
		// If 'empty', stay at 0 tabs
	}

	// Close all tabs (used for logout)
	closeAllTabs(): void {
		this.tabs = [];
		this.activeTabId = null;
		this.split = { enabled: false, panes: null, activePaneId: 'left' };
		this._history = [];
		this._historyIndex = -1;
		localStorage.removeItem(STORAGE_KEY);
	}

	// Debug helper - call windowTabs.debug() in console to see state
	debug(): { tabs: Tab[]; activeTabId: string | null; split: SplitState; allTabs: Tab[]; localStorage: string | null } {
		const state = {
			tabs: [...this.tabs],
			activeTabId: this.activeTabId,
			split: JSON.parse(JSON.stringify(this.split)),
			allTabs: this.getAllTabs(),
			localStorage: localStorage.getItem(STORAGE_KEY)
		};
		console.log('[WindowTabs Debug]', state);
		return state;
	}

	// Set the fallback view preference
	setFallbackPreference(pref: FallbackView): void {
		this.fallbackPreference = pref;
		// Persist to localStorage for quick access on next load
		if (typeof window !== 'undefined') {
			localStorage.setItem('virtues-fallback-preference', pref);
		}
	}

	// Load fallback preference from localStorage
	loadFallbackPreference(): void {
		if (typeof window !== 'undefined') {
			const stored = localStorage.getItem('virtues-fallback-preference');
			if (stored && ['empty', 'chat', 'conway', 'dog-jump', 'wiki-today'].includes(stored)) {
				this.fallbackPreference = stored as FallbackView;
			}
		}
	}

	// ============ Split Screen Methods ============

	// Get the active pane
	get activePane(): PaneState | null {
		if (!this.split.enabled || !this.split.panes) return null;
		return this.split.panes.find((p) => p.id === this.split.activePaneId) || null;
	}

	// Get left pane
	get leftPane(): PaneState | null {
		return this.split.panes?.[0] || null;
	}

	// Get right pane
	get rightPane(): PaneState | null {
		return this.split.panes?.[1] || null;
	}

	// Enable split view
	enableSplit(): void {
		if (this.split.enabled) return;

		// Move all current tabs to left pane
		this.split = {
			enabled: true,
			panes: [
				{ id: 'left', tabs: [...this.tabs], activeTabId: this.activeTabId, width: 50 },
				{ id: 'right', tabs: [], activeTabId: null, width: 50 }
			],
			activePaneId: 'left'
		};

		// Clear the main tabs array since tabs are now in panes
		this.tabs = [];
		this.activeTabId = null;

		this.persist();
	}

	// Disable split view (merge tabs back to single pane)
	disableSplit(): void {
		if (!this.split.enabled || !this.split.panes) return;

		// Merge all tabs from both panes
		const allTabs = [...this.split.panes[0].tabs, ...this.split.panes[1].tabs];

		// Use left pane's active tab, or first tab overall
		const newActiveId =
			this.split.panes[0].activeTabId ||
			this.split.panes[1].activeTabId ||
			allTabs[0]?.id ||
			null;

		this.tabs = allTabs;
		this.activeTabId = newActiveId;
		this.split = { enabled: false, panes: null, activePaneId: 'left' };

		this.persist();
	}

	// Toggle split view
	toggleSplit(): void {
		if (this.split.enabled) {
			this.disableSplit();
		} else {
			this.enableSplit();
		}
	}

	// Set the active pane
	setActivePane(paneId: 'left' | 'right'): void {
		if (!this.split.enabled) return;
		this.split = { ...this.split, activePaneId: paneId };
		this.persist();
	}

	// Move a tab from one pane to another
	moveTabToPane(tabId: string, targetPaneId: 'left' | 'right'): void {
		if (!this.split.enabled || !this.split.panes) return;

		const leftPane = this.split.panes[0];
		const rightPane = this.split.panes[1];

		// Find which pane the tab is in
		const inLeft = leftPane.tabs.some((t) => t.id === tabId);
		const inRight = rightPane.tabs.some((t) => t.id === tabId);

		if (!inLeft && !inRight) return;

		const sourcePaneId = inLeft ? 'left' : 'right';
		if (sourcePaneId === targetPaneId) return;

		const sourcePane = sourcePaneId === 'left' ? leftPane : rightPane;
		const targetPane = targetPaneId === 'left' ? leftPane : rightPane;

		const tab = sourcePane.tabs.find((t) => t.id === tabId);
		if (!tab) return;

		// Remove from source
		const newSourceTabs = sourcePane.tabs.filter((t) => t.id !== tabId);
		let newSourceActiveId = sourcePane.activeTabId;
		if (newSourceActiveId === tabId) {
			newSourceActiveId = newSourceTabs[0]?.id || null;
		}

		// Add to target
		const newTargetTabs = [...targetPane.tabs, tab];

		// Update panes
		const newLeftPane: PaneState =
			sourcePaneId === 'left'
				? { ...leftPane, tabs: newSourceTabs, activeTabId: newSourceActiveId }
				: { ...leftPane, tabs: newTargetTabs, activeTabId: tabId };

		const newRightPane: PaneState =
			sourcePaneId === 'right'
				? { ...rightPane, tabs: newSourceTabs, activeTabId: newSourceActiveId }
				: { ...rightPane, tabs: newTargetTabs, activeTabId: tabId };

		this.split = {
			...this.split,
			panes: [newLeftPane, newRightPane],
			activePaneId: targetPaneId
		};

		// Auto-close split if source pane is now empty
		if (newSourceTabs.length === 0) {
			this.disableSplit();
		} else {
			this.persist();
		}
	}

	// Set pane width (left pane percentage)
	setPaneWidth(leftWidth: number): void {
		if (!this.split.enabled || !this.split.panes) return;

		const clampedWidth = Math.max(20, Math.min(80, leftWidth));

		this.split = {
			...this.split,
			panes: [
				{ ...this.split.panes[0], width: clampedWidth },
				{ ...this.split.panes[1], width: 100 - clampedWidth }
			]
		};

		this.persist();
	}

	// Open a tab in a specific pane (when split is enabled)
	openTabInPane(input: TabInput, paneId: 'left' | 'right'): string {
		if (!this.split.enabled || !this.split.panes) {
			// Not in split mode, use regular openTab
			return this.openTab(input);
		}

		const id = crypto.randomUUID();
		const tab: Tab = {
			...input,
			id,
			createdAt: Date.now()
		};

		this.pushToHistory(id);

		const paneIndex = paneId === 'left' ? 0 : 1;
		const newPanes = [...this.split.panes] as [PaneState, PaneState];
		newPanes[paneIndex] = {
			...newPanes[paneIndex],
			tabs: [...newPanes[paneIndex].tabs, tab],
			activeTabId: id
		};

		this.split = {
			...this.split,
			panes: newPanes,
			activePaneId: paneId
		};

		this.persist();
		return id;
	}

	// Close a tab in a specific pane
	closeTabInPane(tabId: string, paneId: 'left' | 'right'): void {
		if (!this.split.enabled || !this.split.panes) return;

		const paneIndex = paneId === 'left' ? 0 : 1;
		const otherPaneIndex = paneId === 'left' ? 1 : 0;
		const pane = this.split.panes[paneIndex];
		const otherPane = this.split.panes[otherPaneIndex];
		const tabIndex = pane.tabs.findIndex((t) => t.id === tabId);

		if (tabIndex === -1) return;

		const newTabs = pane.tabs.filter((t) => t.id !== tabId);

		// If this was the last tab in this pane, collapse to the other pane only
		if (newTabs.length === 0) {
			// Use only the other pane's tabs (the closed tab is discarded)
			this.tabs = [...otherPane.tabs];
			this.activeTabId = otherPane.activeTabId || otherPane.tabs[0]?.id || null;
			this.split = { enabled: false, panes: null, activePaneId: 'left' };
			this.persist();
			return;
		}

		// Determine new active tab for this pane
		let newActiveId = pane.activeTabId;
		if (newActiveId === tabId) {
			if (tabIndex === newTabs.length) {
				// Was last, activate previous
				newActiveId = newTabs[tabIndex - 1]?.id || null;
			} else {
				// Activate next
				newActiveId = newTabs[tabIndex]?.id || null;
			}
		}

		const newPanes = [...this.split.panes] as [PaneState, PaneState];
		newPanes[paneIndex] = {
			...newPanes[paneIndex],
			tabs: newTabs,
			activeTabId: newActiveId
		};

		this.split = { ...this.split, panes: newPanes };
		this.persist();
	}

	// Set active tab within a pane
	setActiveTabInPane(tabId: string, paneId: 'left' | 'right'): void {
		if (!this.split.enabled || !this.split.panes) return;

		const paneIndex = paneId === 'left' ? 0 : 1;
		const pane = this.split.panes[paneIndex];

		if (!pane.tabs.some((t) => t.id === tabId)) return;

		this.pushToHistory(tabId);

		const newPanes = [...this.split.panes] as [PaneState, PaneState];
		newPanes[paneIndex] = { ...newPanes[paneIndex], activeTabId: tabId };

		this.split = {
			...this.split,
			panes: newPanes,
			activePaneId: paneId
		};

		this.persist();
	}

	// Open session context in split view (opposite pane from chat)
	openSessionContext(conversationId: string, currentPaneId: 'left' | 'right' | null): string {
		// If not in split mode (null) or chat is in left pane, context goes to right
		// After enableSplit(), existing tabs go to left pane, so null should target right
		const targetPaneId = currentPaneId === 'right' ? 'left' : 'right';

		// Check for existing context tab for this conversation
		const existing = this.findTab(
			(t) => t.type === 'session-context' && t.linkedConversationId === conversationId
		);
		if (existing) {
			if (existing.paneId) {
				this.setActiveTabInPane(existing.tab.id, existing.paneId);
			} else {
				this.setActiveTab(existing.tab.id);
			}
			return existing.tab.id;
		}

		// Enable split if needed
		if (!this.split.enabled) {
			this.enableSplit();
		}

		// Open in opposite pane
		return this.openTabInPane(
			{
				type: 'session-context',
				label: 'Context',
				route: `/context/${conversationId}`,
				icon: 'ri:information-line',
				linkedConversationId: conversationId
			},
			targetPaneId
		);
	}

	// ============ URL Serialization Methods ============
	// These delegate to the extracted urlSerializer module

	/**
	 * Serialize the entire tab state to URL query parameters.
	 * Format: ?tabs=type_id,type_id&active=0&split=true&tabs2=type_id&active2=0
	 */
	serializeToUrl(): string {
		return serializeStateToUrl({
			tabs: this.tabs,
			activeTabId: this.activeTabId,
			split: this.split
		});
	}

	/**
	 * Deserialize URL query parameters to restore tab state.
	 * Call this on initial page load to restore state from URL.
	 */
	deserializeFromUrl(url: string): void {
		const state = deserializeStateFromUrl(url);
		if (!state) {
			return;
		}

		// Apply the restored state
		this.tabs = state.tabs;
		this.activeTabId = state.activeTabId;
		this.split = state.split;

		this.persist();
		console.log('[WindowTabs] Restored', this.getAllTabs().length, 'tabs from URL');
	}

	/**
	 * Check if the current URL has tab params that should be restored.
	 */
	hasUrlTabParams(url: string): boolean {
		return hasUrlTabParams(url);
	}
}

export const windowTabs = new WindowTabsStore();

// Expose to window for debugging (browser only)
if (typeof window !== 'undefined') {
	(window as unknown as { windowTabs: WindowTabsStore }).windowTabs = windowTabs;
}
