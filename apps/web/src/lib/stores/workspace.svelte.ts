/**
 * Workspace Store
 *
 * UNIFIED state management for the Knowledge OS. This is the SINGLE source of truth.
 * All components should import from here, NOT from workspaceStore.
 *
 * Features:
 * - Workspaces (swipeable contexts like Arc browser)
 * - Explorer tree (unified hierarchy via explorer_nodes)
 * - Tabs (with full split-screen support)
 * - Entity metadata registry (lazy-loaded cache)
 */

import {
	listWorkspaces,
	getWorkspace,
	createWorkspace as apiCreateWorkspace,
	updateWorkspace as apiUpdateWorkspace,
	deleteWorkspace as apiDeleteWorkspace,
	saveWorkspaceTabState,
	getWorkspaceTree,
	createExplorerNode,
	updateExplorerNode,
	deleteExplorerNode,
	moveExplorerNodes,
	resolveView,
	type Workspace,
	type WorkspaceSummary,
	type ExplorerNode,
	type ExplorerTreeNode,
	type ViewConfig,
	type ViewEntity
} from '$lib/api/client';
import {
	type Tab,
	type TabType,
	type FallbackView,
	type PaneState,
	type SplitState,
	getTabDomain
} from '$lib/tabs/types';
import { parseRoute } from '$lib/tabs/registry';
import { applyWorkspaceTheming, clearWorkspaceTheming } from '$lib/utils/theme';

// Re-export types for convenience
export type { Tab, TabType, FallbackView, PaneState, SplitState };
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

export interface TabState {
	tabs: Tab[];
	activeTabId: string | null;
	split: SplitState;
}

// ============================================================================
// Entity Type Utilities
// ============================================================================

const ENTITY_TYPE_MAP: Record<string, { type: string; icon: string; routePrefix: string }> = {
	page: { type: 'page', icon: 'ri:file-text-line', routePrefix: '/pages' },
	chat: { type: 'chat', icon: 'ri:chat-1-line', routePrefix: '/' },
	person: { type: 'wiki_person', icon: 'ri:user-line', routePrefix: '/wiki' },
	place: { type: 'wiki_place', icon: 'ri:map-pin-line', routePrefix: '/wiki' },
	org: { type: 'wiki_org', icon: 'ri:building-line', routePrefix: '/wiki' },
	thing: { type: 'wiki_thing', icon: 'ri:box-3-line', routePrefix: '/wiki' },
	day: { type: 'wiki_day', icon: 'ri:calendar-line', routePrefix: '/wiki' },
	file: { type: 'drive_file', icon: 'ri:file-line', routePrefix: '/data/drive' },
	source: { type: 'source_connection', icon: 'ri:database-2-line', routePrefix: '/data/sources' },
	sys: { type: 'system', icon: 'ri:settings-3-line', routePrefix: '' }
};

/**
 * Get entity type info from an entity ID prefix
 */
export function getEntityTypeFromId(entityId: string): { type: string; icon: string; routePrefix: string } {
	const prefix = entityId.split('_')[0];
	return ENTITY_TYPE_MAP[prefix] || { type: 'unknown', icon: 'ri:question-line', routePrefix: '' };
}

/**
 * Build a route from an entity ID
 */
export function getRouteFromEntityId(entityId: string): string {
	const { routePrefix } = getEntityTypeFromId(entityId);
	if (entityId.startsWith('chat_')) {
		// Strip the chat_ prefix to get the actual conversationId (UUID)
		const conversationId = entityId.slice(5);
		return `/?conversationId=${conversationId}`;
	}
	if (entityId.startsWith('sys_')) {
		// Map system IDs to their routes
		const systemRoutes: Record<string, string> = {
			sys_settings: '/profile/account',
			sys_history: '/history',
			sys_usage: '/profile/usage',
			sys_assistant: '/profile/assistant',
			sys_drive: '/data/drive',
			sys_sources: '/data/sources',
			sys_add_source: '/data/sources/add'
		};
		return systemRoutes[entityId] || '/';
	}
	return `${routePrefix}/${entityId}`;
}

// ============================================================================
// Store Class
// ============================================================================

const TAB_STORAGE_KEY = 'virtues-window-tabs';
const TAB_STORAGE_VERSION = 3; // Increment to force migration

class WorkspaceStore {
	// Workspaces
	workspaces = $state<WorkspaceSummary[]>([]);
	activeWorkspaceId = $state<string>('ws_system');

	// Trees for ALL workspaces (enables smooth swiping)
	trees = $state<Map<string, ExplorerTreeNode[]>>(new Map());
	expandedNodeIds = $state<Set<string>>(new Set());
	
	// Convenience getter for active workspace's tree
	get tree(): ExplorerTreeNode[] {
		return this.trees.get(this.activeWorkspaceId) || [];
	}

	// Tab state (single source of truth)
	tabs = $state<Tab[]>([]);
	activeTabId = $state<string | null>(null);
	split = $state<SplitState>({
		enabled: false,
		panes: null,
		activePaneId: 'left'
	});

	// Fallback view preference
	fallbackPreference = $state<FallbackView>('empty');

	// Drag state for tab reordering UI
	isDragging = $state(false);

	// Navigation history
	private _history = $state<string[]>([]);
	private _historyIndex = $state<number>(-1);
	private _isNavigatingHistory = false;

	// View cache: nodeId -> resolved entities
	viewCache = $state<Map<string, ViewEntity[]>>(new Map());

	// Entity metadata registry (lazy-loaded)
	registry = $state<Map<string, EntityMetadata>>(new Map());

	// Loading states
	loading = $state(false);
	treeLoading = $state(false);

	// Initialization flag
	private initialized = false;

	// Derived
	get activeWorkspace(): WorkspaceSummary | undefined {
		return this.workspaces.find((w) => w.id === this.activeWorkspaceId);
	}

	get isSystemWorkspace(): boolean {
		return this.activeWorkspaceId === 'ws_system';
	}

	get isLocked(): boolean {
		return this.activeWorkspace?.is_locked ?? false;
	}

	get activeTab(): Tab | undefined {
		if (this.split.enabled && this.split.panes) {
			const pane = this.split.panes.find((p) => p.id === this.split.activePaneId);
			return pane?.tabs.find((t) => t.id === pane.activeTabId);
		}
		return this.tabs.find((t) => t.id === this.activeTabId);
	}

	// ============================================================================
	// Initialization
	// ============================================================================

	async init(): Promise<void> {
		if (this.initialized) return;
		if (typeof window === 'undefined') return;

		this.initialized = true;

		try {
			// Load workspaces
			await this.loadWorkspaces();

			// Load trees for ALL workspaces (enables smooth swiping)
			await this.loadAllTrees();

			// Restore tab state from localStorage (temporary until we fully migrate to SQLite)
			this.restoreTabState();

			// Apply workspace-specific theming
			const workspace = this.activeWorkspace;
			if (workspace) {
				applyWorkspaceTheming(workspace.accent_color, workspace.theme_mode);
			}
		} catch (e) {
			console.error('[WorkspaceStore] Failed to initialize:', e);
		}
	}

	// ============================================================================
	// Workspace Management
	// ============================================================================

	async loadWorkspaces(): Promise<void> {
		this.loading = true;
		try {
			const response = await listWorkspaces();
			this.workspaces = response.workspaces;

			// Default to system workspace if current doesn't exist
			if (!this.workspaces.find((w) => w.id === this.activeWorkspaceId)) {
				this.activeWorkspaceId = 'ws_system';
			}
		} catch (e) {
			console.error('[WorkspaceStore] Failed to load workspaces:', e);
		} finally {
			this.loading = false;
		}
	}

	async createWorkspace(name: string, icon?: string, accentColor?: string): Promise<Workspace | null> {
		try {
			const workspace = await apiCreateWorkspace(name, icon, accentColor);
			await this.loadWorkspaces();
			return workspace;
		} catch (e) {
			console.error('[WorkspaceStore] Failed to create workspace:', e);
			return null;
		}
	}

	async updateWorkspace(
		id: string,
		updates: { name?: string; icon?: string; accent_color?: string; theme_mode?: string }
	): Promise<void> {
		try {
			await apiUpdateWorkspace(id, updates);
			await this.loadWorkspaces();
		} catch (e) {
			console.error('[WorkspaceStore] Failed to update workspace:', e);
		}
	}

	async deleteWorkspace(id: string): Promise<void> {
		// Can't delete system workspace
		const workspace = this.workspaces.find((w) => w.id === id);
		if (workspace?.is_system) {
			console.warn('[WorkspaceStore] Cannot delete system workspace');
			return;
		}

		try {
			// If deleting active workspace, switch to system first
			if (this.activeWorkspaceId === id) {
				await this.switchWorkspace('ws_system');
			}

			await apiDeleteWorkspace(id);
			await this.loadWorkspaces();
		} catch (e) {
			console.error('[WorkspaceStore] Failed to delete workspace:', e);
		}
	}

	async switchWorkspace(workspaceId: string): Promise<void> {
		if (workspaceId === this.activeWorkspaceId) return;

		// Save current tab state before switching
		await this.persistTabState();

		// Switch
		this.activeWorkspaceId = workspaceId;

		// Only load tree if not already cached
		if (!this.trees.has(workspaceId)) {
			await this.loadTree(workspaceId);
		}

		// Restore tab state for new workspace
		this.restoreTabState();

		// Clear view cache
		this.viewCache = new Map();

		// Apply workspace-specific theming
		const workspace = this.activeWorkspace;
		if (workspace) {
			applyWorkspaceTheming(workspace.accent_color, workspace.theme_mode);
		} else {
			clearWorkspaceTheming();
		}
	}

	// Navigate to adjacent workspace
	navigateWorkspace(direction: 'prev' | 'next'): void {
		const currentIndex = this.workspaces.findIndex((w) => w.id === this.activeWorkspaceId);
		if (currentIndex === -1) return;

		let newIndex: number;
		if (direction === 'prev') {
			newIndex = currentIndex > 0 ? currentIndex - 1 : this.workspaces.length - 1;
		} else {
			newIndex = currentIndex < this.workspaces.length - 1 ? currentIndex + 1 : 0;
		}

		this.switchWorkspace(this.workspaces[newIndex].id);
	}

	// ============================================================================
	// Tree Management
	// ============================================================================

	async loadTree(workspaceId: string): Promise<void> {
		this.treeLoading = true;
		try {
			const response = await getWorkspaceTree(workspaceId);
			// Update the trees map
			const newTrees = new Map(this.trees);
			newTrees.set(workspaceId, response.nodes);
			this.trees = newTrees;
		} catch (e) {
			console.error('[WorkspaceStore] Failed to load tree:', e);
			const newTrees = new Map(this.trees);
			newTrees.set(workspaceId, []);
			this.trees = newTrees;
		} finally {
			this.treeLoading = false;
		}
	}

	async loadAllTrees(): Promise<void> {
		// Load trees for all workspaces in parallel
		await Promise.all(
			this.workspaces.map(ws => this.loadTree(ws.id))
		);
	}

	async refreshTree(): Promise<void> {
		await this.loadTree(this.activeWorkspaceId);
	}
	
	// Get tree for a specific workspace
	getTreeForWorkspace(workspaceId: string): ExplorerTreeNode[] {
		return this.trees.get(workspaceId) || [];
	}

	toggleNodeExpanded(nodeId: string): void {
		const newSet = new Set(this.expandedNodeIds);
		if (newSet.has(nodeId)) {
			newSet.delete(nodeId);
		} else {
			newSet.add(nodeId);
		}
		this.expandedNodeIds = newSet;
	}

	isNodeExpanded(nodeId: string): boolean {
		return this.expandedNodeIds.has(nodeId);
	}

	// ============================================================================
	// Node CRUD
	// ============================================================================

	async createFolder(name: string, parentId?: string): Promise<ExplorerNode | null> {
		if (this.isLocked) {
			console.warn('[WorkspaceStore] Cannot modify locked workspace');
			return null;
		}

		try {
			const node = await createExplorerNode(this.activeWorkspaceId, 'folder', {
				name,
				parent_id: parentId
			});
			await this.refreshTree();
			return node;
		} catch (e) {
			console.error('[WorkspaceStore] Failed to create folder:', e);
			return null;
		}
	}

	async createView(name: string, config: ViewConfig, parentId?: string): Promise<ExplorerNode | null> {
		if (this.isLocked) {
			console.warn('[WorkspaceStore] Cannot modify locked workspace');
			return null;
		}

		try {
			const node = await createExplorerNode(this.activeWorkspaceId, 'view', {
				name,
				parent_id: parentId,
				view_config_json: JSON.stringify(config)
			});
			await this.refreshTree();
			return node;
		} catch (e) {
			console.error('[WorkspaceStore] Failed to create view:', e);
			return null;
		}
	}

	async createShortcut(entityId: string, parentId?: string): Promise<ExplorerNode | null> {
		if (this.isLocked) {
			console.warn('[WorkspaceStore] Cannot modify locked workspace');
			return null;
		}

		try {
			const node = await createExplorerNode(this.activeWorkspaceId, 'shortcut', {
				entity_id: entityId,
				parent_id: parentId
			});
			await this.refreshTree();
			return node;
		} catch (e) {
			console.error('[WorkspaceStore] Failed to create shortcut:', e);
			return null;
		}
	}

	async renameNode(nodeId: string, newName: string): Promise<void> {
		if (this.isLocked) return;

		try {
			await updateExplorerNode(nodeId, { name: newName });
			await this.refreshTree();
		} catch (e) {
			console.error('[WorkspaceStore] Failed to rename node:', e);
		}
	}

	async moveNode(nodeId: string, newParentId: string | null, sortOrder: number): Promise<void> {
		if (this.isLocked) return;

		try {
			await moveExplorerNodes([nodeId], newParentId, sortOrder);
			await this.refreshTree();
		} catch (e) {
			console.error('[WorkspaceStore] Failed to move node:', e);
		}
	}

	async deleteNode(nodeId: string): Promise<void> {
		if (this.isLocked) return;

		try {
			await deleteExplorerNode(nodeId);
			await this.refreshTree();
		} catch (e) {
			console.error('[WorkspaceStore] Failed to delete node:', e);
		}
	}

	// ============================================================================
	// View Resolution
	// ============================================================================

	async resolveViewNode(node: ExplorerTreeNode): Promise<ViewEntity[]> {
		if (node.node_type !== 'view' || !node.view_config_json) {
			return [];
		}

		// Check cache
		const cached = this.viewCache.get(node.id);
		if (cached) return cached;

		try {
			const config = JSON.parse(node.view_config_json) as ViewConfig;
			const response = await resolveView(config, this.activeWorkspaceId);

			// Cache result
			const newCache = new Map(this.viewCache);
			newCache.set(node.id, response.entities);
			this.viewCache = newCache;

			return response.entities;
		} catch (e) {
			console.error('[WorkspaceStore] Failed to resolve view:', e);
			return [];
		}
	}

	// ============================================================================
	// Entity Registry
	// ============================================================================

	/**
	 * Get entity metadata from registry, fetching if needed
	 */
	async getEntityMetadata(entityId: string): Promise<EntityMetadata | null> {
		// Check cache
		const cached = this.registry.get(entityId);
		if (cached) return cached;

		// Generate metadata from ID
		const { type, icon, routePrefix } = getEntityTypeFromId(entityId);
		const route = getRouteFromEntityId(entityId);

		const metadata: EntityMetadata = {
			id: entityId,
			name: entityId, // Will be updated when we fetch the actual entity
			type,
			icon,
			route
		};

		// Cache it
		const newRegistry = new Map(this.registry);
		newRegistry.set(entityId, metadata);
		this.registry = newRegistry;

		return metadata;
	}

	/**
	 * Update entity metadata in registry (e.g., when title changes)
	 */
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

	private persistTabState(): void {
		if (typeof window === 'undefined') return;

		const data = {
			version: TAB_STORAGE_VERSION,
			tabs: this.tabs,
			activeTabId: this.activeTabId,
			split: this.split
		};

		try {
			localStorage.setItem(TAB_STORAGE_KEY, JSON.stringify(data));
		} catch (e) {
			console.warn('[WorkspaceStore] Failed to persist tab state:', e);
		}
	}

	private restoreTabState(): void {
		if (typeof window === 'undefined') return;

		// Load fallback preference
		const fallbackStored = localStorage.getItem('virtues-fallback-preference');
		if (fallbackStored && ['empty', 'chat', 'conway', 'dog-jump', 'wiki-today'].includes(fallbackStored)) {
			this.fallbackPreference = fallbackStored as FallbackView;
		}

		try {
			const stored = localStorage.getItem(TAB_STORAGE_KEY);
			if (stored) {
				const data = JSON.parse(stored);

				// Migration check
				if (!data.version || data.version < TAB_STORAGE_VERSION) {
					localStorage.removeItem(TAB_STORAGE_KEY);
				} else {
					// Restore split state if valid
					if (data.split?.enabled && data.split.panes) {
						const totalTabs = (data.split.panes[0]?.tabs?.length || 0) + (data.split.panes[1]?.tabs?.length || 0);
						if (totalTabs > 0) {
							this.split = data.split;
							this.tabs = [];
							this.activeTabId = null;
							return;
						}
					}

					// Regular restore
					if (Array.isArray(data.tabs) && data.tabs.length > 0) {
						this.tabs = data.tabs;
						this.activeTabId = data.activeTabId || data.tabs[0]?.id || null;
						this.split = { enabled: false, panes: null, activePaneId: 'left' };
						return;
					}
				}
			}
		} catch (e) {
			console.warn('[WorkspaceStore] Failed to restore tab state:', e);
		}

		// Default: create initial tab based on preference
		this.tabs = [];
		this.activeTabId = null;
		this.split = { enabled: false, panes: null, activePaneId: 'left' };
		this.openDefaultTab();
	}

	private openDefaultTab(): void {
		const pref = this.fallbackPreference;
		if (pref === 'chat' || pref === 'empty') {
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

	// ============================================================================
	// Tab CRUD
	// ============================================================================

	openTab(input: Omit<Tab, 'id' | 'createdAt'>): string {
		const id = crypto.randomUUID();
		const tab: Tab = { ...input, id, createdAt: Date.now() };

		this.pushToHistory(id);

		if (this.split.enabled && this.split.panes) {
			const paneIndex = this.split.activePaneId === 'left' ? 0 : 1;
			const newPanes = [...this.split.panes] as [PaneState, PaneState];
			newPanes[paneIndex] = {
				...newPanes[paneIndex],
				tabs: [...newPanes[paneIndex].tabs, tab],
				activeTabId: id
			};
			this.split = { ...this.split, panes: newPanes };
		} else {
			this.tabs = [...this.tabs, tab];
			this.activeTabId = id;
		}

		this.persistTabState();
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

		// Find existing tab if not forcing new
		if (!options?.forceNew) {
			let result: { tab: Tab; paneId: 'left' | 'right' | null } | undefined;

			if (parsed.type === 'chat' && parsed.conversationId) {
				result = this.findTab((t) => t.conversationId === parsed.conversationId);
			} else if (parsed.type === 'page-detail' && parsed.pageId) {
				result = this.findTab((t) => t.type === 'page-detail' && t.pageId === parsed.pageId);
			} else if (parsed.type === 'wiki' && parsed.slug) {
				result = this.findTab((t) => t.type === 'wiki' && t.slug === parsed.slug);
			} else if (parsed.type === 'wiki-list' && parsed.wikiCategory) {
				result = this.findTab((t) => t.type === 'wiki-list' && t.wikiCategory === parsed.wikiCategory);
			} else if (parsed.type === 'data-sources' && parsed.sourceId) {
				result = this.findTab((t) => t.type === 'data-sources' && t.sourceId === parsed.sourceId);
			} else if (parsed.type === 'profile' && parsed.profileSection) {
				result = this.findTab((t) => t.type === 'profile' && t.profileSection === parsed.profileSection);
			} else {
				result = this.findTab((t) => t.type === parsed.type && !t.conversationId && !t.slug && !t.sourceId);
			}

			if (result) {
				if (result.paneId) {
					this.setActiveTabInPane(result.tab.id, result.paneId);
				} else {
					this.setActiveTab(result.tab.id);
				}
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
						route,
						icon: parsed.icon,
						conversationId: parsed.conversationId,
						pageId: parsed.pageId,
						slug: parsed.slug,
						sourceId: parsed.sourceId,
						wikiCategory: parsed.wikiCategory,
						profileSection: parsed.profileSection
					});
					return currentTab.id;
				}
			}
		}

		// Create new tab
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
			if (options?.paneId) {
				return this.openTabInPane(tabInput, options.paneId);
			}
			if (options?.preferEmptyPane) {
				if (this.split.panes[0].tabs.length === 0) return this.openTabInPane(tabInput, 'left');
				if (this.split.panes[1].tabs.length === 0) return this.openTabInPane(tabInput, 'right');
			}
			return this.openTabInPane(tabInput, this.split.activePaneId);
		}

		return this.openTab(tabInput);
	}

	openEntityTab(entityId: string, name?: string): string {
		const route = getRouteFromEntityId(entityId);
		return this.openTabFromRoute(route, { label: name || entityId });
	}

	openOrFocusChat(conversationId: string, label?: string): void {
		const existing = this.findTab((t) => t.conversationId === conversationId);
		if (existing) {
			this.setActiveTab(existing.tab.id);
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

	closeTab(tabId: string): void {
		if (this.split.enabled && this.split.panes) {
			const paneId = this.findTabPane(tabId);
			if (paneId) {
				this.closeTabInPane(tabId, paneId);
			}
			return;
		}

		const index = this.tabs.findIndex((t) => t.id === tabId);
		if (index === -1) return;

		if (this.activeTabId === tabId) {
			if (this.tabs.length === 1) {
				this.tabs = [];
				this.activeTabId = null;
				this.openDefaultTab();
				return;
			} else if (index === this.tabs.length - 1) {
				this.activeTabId = this.tabs[index - 1].id;
			} else {
				this.activeTabId = this.tabs[index + 1].id;
			}
		}

		this.tabs = this.tabs.filter((t) => t.id !== tabId);
		this.persistTabState();
	}

	closeOtherTabs(tabId: string, paneId?: 'left' | 'right'): void {
		if (this.split.enabled && this.split.panes && paneId) {
			const paneIndex = paneId === 'left' ? 0 : 1;
			const pane = this.split.panes[paneIndex];
			const tabToKeep = pane.tabs.find((t) => t.id === tabId);
			if (!tabToKeep) return;

			const newPanes = [...this.split.panes] as [PaneState, PaneState];
			newPanes[paneIndex] = { ...newPanes[paneIndex], tabs: [tabToKeep], activeTabId: tabId };
			this.split = { ...this.split, panes: newPanes };
			this.persistTabState();
			return;
		}

		const tabToKeep = this.tabs.find((t) => t.id === tabId);
		if (!tabToKeep) return;

		this.tabs = [tabToKeep];
		this.activeTabId = tabId;
		this.persistTabState();
	}

	closeTabsToRight(tabId: string, paneId?: 'left' | 'right'): void {
		if (this.split.enabled && this.split.panes && paneId) {
			const paneIndex = paneId === 'left' ? 0 : 1;
			const pane = this.split.panes[paneIndex];
			const index = pane.tabs.findIndex((t) => t.id === tabId);
			if (index === -1) return;

			const newTabs = pane.tabs.slice(0, index + 1);
			const newActiveId = newTabs.some((t) => t.id === pane.activeTabId) ? pane.activeTabId : newTabs[newTabs.length - 1]?.id || null;

			const newPanes = [...this.split.panes] as [PaneState, PaneState];
			newPanes[paneIndex] = { ...newPanes[paneIndex], tabs: newTabs, activeTabId: newActiveId };
			this.split = { ...this.split, panes: newPanes };
			this.persistTabState();
			return;
		}

		const index = this.tabs.findIndex((t) => t.id === tabId);
		if (index === -1) return;

		const newTabs = this.tabs.slice(0, index + 1);
		if (!newTabs.some((t) => t.id === this.activeTabId)) {
			this.activeTabId = newTabs[newTabs.length - 1]?.id || null;
		}
		this.tabs = newTabs;
		this.persistTabState();
	}

	closeAllTabs(): void {
		this.tabs = [];
		this.activeTabId = null;
		this.split = { enabled: false, panes: null, activePaneId: 'left' };
		this._history = [];
		this._historyIndex = -1;
		localStorage.removeItem(TAB_STORAGE_KEY);
	}

	setActiveTab(tabId: string): void {
		if (this.split.enabled && this.split.panes) {
			const paneId = this.findTabPane(tabId);
			if (paneId) {
				this.setActiveTabInPane(tabId, paneId);
			}
			return;
		}

		if (this.tabs.some((t) => t.id === tabId)) {
			this.pushToHistory(tabId);
			this.activeTabId = tabId;
			this.persistTabState();
		}
	}

	updateTab(tabId: string, updates: Partial<Omit<Tab, 'id' | 'createdAt'>>): void {
		if (this.split.enabled && this.split.panes) {
			const paneId = this.findTabPane(tabId);
			if (paneId) {
				const paneIndex = paneId === 'left' ? 0 : 1;
				const newPanes = [...this.split.panes] as [PaneState, PaneState];
				newPanes[paneIndex] = {
					...newPanes[paneIndex],
					tabs: newPanes[paneIndex].tabs.map((t) => (t.id === tabId ? { ...t, ...updates } : t))
				};
				this.split = { ...this.split, panes: newPanes };
				this.persistTabState();
			}
			return;
		}

		this.tabs = this.tabs.map((t) => (t.id === tabId ? { ...t, ...updates } : t));
		this.persistTabState();
	}

	togglePin(tabId: string): void {
		const tab = this.getAllTabs().find((t) => t.id === tabId);
		if (!tab) return;

		this.updateTab(tabId, { pinned: !tab.pinned });

		// Re-sort to put pinned first
		if (this.split.enabled && this.split.panes) {
			const paneId = this.findTabPane(tabId);
			if (paneId) {
				const paneIndex = paneId === 'left' ? 0 : 1;
				const sortedTabs = [...this.split.panes[paneIndex].tabs].sort((a, b) => {
					if (a.pinned && !b.pinned) return -1;
					if (!a.pinned && b.pinned) return 1;
					return 0;
				});
				const newPanes = [...this.split.panes] as [PaneState, PaneState];
				newPanes[paneIndex] = { ...newPanes[paneIndex], tabs: sortedTabs };
				this.split = { ...this.split, panes: newPanes };
				this.persistTabState();
			}
		} else {
			this.tabs = [...this.tabs].sort((a, b) => {
				if (a.pinned && !b.pinned) return -1;
				if (!a.pinned && b.pinned) return 1;
				return 0;
			});
			this.persistTabState();
		}
	}

	reorderTabs(fromIndex: number, toIndex: number): void {
		if (fromIndex === toIndex) return;
		if (fromIndex < 0 || fromIndex >= this.tabs.length) return;
		if (toIndex < 0 || toIndex >= this.tabs.length) return;

		const newTabs = [...this.tabs];
		const [moved] = newTabs.splice(fromIndex, 1);
		newTabs.splice(toIndex, 0, moved);
		this.tabs = newTabs;
		this.persistTabState();
	}

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
		this.persistTabState();
	}

	// ============================================================================
	// Tab Query Methods
	// ============================================================================

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

	findTabPane(tabId: string): 'left' | 'right' | null {
		if (!this.split.enabled || !this.split.panes) return null;
		if (this.split.panes[0].tabs.some((t) => t.id === tabId)) return 'left';
		if (this.split.panes[1].tabs.some((t) => t.id === tabId)) return 'right';
		return null;
	}

	getAllTabs(): Tab[] {
		if (this.split.enabled && this.split.panes) {
			return [...this.split.panes[0].tabs, ...this.split.panes[1].tabs];
		}
		return this.tabs;
	}

	getActiveTabsForSidebar(): Tab[] {
		if (this.split.enabled && this.split.panes) {
			const activeTabs: Tab[] = [];
			for (const pane of this.split.panes) {
				const activeTab = pane.tabs.find((t) => t.id === pane.activeTabId);
				if (activeTab) activeTabs.push(activeTab);
			}
			return activeTabs;
		}
		const singleActiveTab = this.tabs.find((t) => t.id === this.activeTabId);
		return singleActiveTab ? [singleActiveTab] : [];
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

		const result = this.findTab((t) => t.id === tabId);
		if (result) {
			if (result.paneId) {
				this.setActiveTabInPane(result.tab.id, result.paneId);
			} else {
				this.activeTabId = result.tab.id;
				this.persistTabState();
			}
		}

		this._isNavigatingHistory = false;
	}

	goForward(): void {
		if (!this.canGoForward()) return;

		this._isNavigatingHistory = true;
		this._historyIndex++;
		const tabId = this._history[this._historyIndex];

		const result = this.findTab((t) => t.id === tabId);
		if (result) {
			if (result.paneId) {
				this.setActiveTabInPane(result.tab.id, result.paneId);
			} else {
				this.activeTabId = result.tab.id;
				this.persistTabState();
			}
		}

		this._isNavigatingHistory = false;
	}

	// ============================================================================
	// Split Screen Methods
	// ============================================================================

	get leftPane(): PaneState | null {
		return this.split.panes?.[0] || null;
	}

	get rightPane(): PaneState | null {
		return this.split.panes?.[1] || null;
	}

	enableSplit(): void {
		if (this.split.enabled) return;

		this.split = {
			enabled: true,
			panes: [
				{ id: 'left', tabs: [...this.tabs], activeTabId: this.activeTabId, width: 50 },
				{ id: 'right', tabs: [], activeTabId: null, width: 50 }
			],
			activePaneId: 'left'
		};

		this.tabs = [];
		this.activeTabId = null;
		this.persistTabState();
	}

	disableSplit(): void {
		if (!this.split.enabled || !this.split.panes) return;

		const allTabs = [...this.split.panes[0].tabs, ...this.split.panes[1].tabs];
		const newActiveId = this.split.panes[0].activeTabId || this.split.panes[1].activeTabId || allTabs[0]?.id || null;

		this.tabs = allTabs;
		this.activeTabId = newActiveId;
		this.split = { enabled: false, panes: null, activePaneId: 'left' };
		this.persistTabState();
	}

	toggleSplit(): void {
		if (this.split.enabled) {
			this.disableSplit();
		} else {
			this.enableSplit();
		}
	}

	setActivePane(paneId: 'left' | 'right'): void {
		if (!this.split.enabled) return;
		this.split = { ...this.split, activePaneId: paneId };
		this.persistTabState();
	}

	openTabInPane(input: Omit<Tab, 'id' | 'createdAt'>, paneId: 'left' | 'right'): string {
		if (!this.split.enabled || !this.split.panes) {
			return this.openTab(input);
		}

		const id = crypto.randomUUID();
		const tab: Tab = { ...input, id, createdAt: Date.now() };

		this.pushToHistory(id);

		const paneIndex = paneId === 'left' ? 0 : 1;
		const newPanes = [...this.split.panes] as [PaneState, PaneState];
		newPanes[paneIndex] = {
			...newPanes[paneIndex],
			tabs: [...newPanes[paneIndex].tabs, tab],
			activeTabId: id
		};

		this.split = { ...this.split, panes: newPanes, activePaneId: paneId };
		this.persistTabState();
		return id;
	}

	closeTabInPane(tabId: string, paneId: 'left' | 'right'): void {
		if (!this.split.enabled || !this.split.panes) return;

		const paneIndex = paneId === 'left' ? 0 : 1;
		const otherPaneIndex = paneId === 'left' ? 1 : 0;
		const pane = this.split.panes[paneIndex];
		const otherPane = this.split.panes[otherPaneIndex];
		const tabIndex = pane.tabs.findIndex((t) => t.id === tabId);

		if (tabIndex === -1) return;

		const newTabs = pane.tabs.filter((t) => t.id !== tabId);

		// If last tab in pane, collapse split
		if (newTabs.length === 0) {
			this.tabs = [...otherPane.tabs];
			this.activeTabId = otherPane.activeTabId || otherPane.tabs[0]?.id || null;
			this.split = { enabled: false, panes: null, activePaneId: 'left' };
			this.persistTabState();
			return;
		}

		let newActiveId = pane.activeTabId;
		if (newActiveId === tabId) {
			newActiveId = tabIndex === newTabs.length ? newTabs[tabIndex - 1]?.id || null : newTabs[tabIndex]?.id || null;
		}

		const newPanes = [...this.split.panes] as [PaneState, PaneState];
		newPanes[paneIndex] = { ...newPanes[paneIndex], tabs: newTabs, activeTabId: newActiveId };
		this.split = { ...this.split, panes: newPanes };
		this.persistTabState();
	}

	setActiveTabInPane(tabId: string, paneId: 'left' | 'right'): void {
		if (!this.split.enabled || !this.split.panes) return;

		const paneIndex = paneId === 'left' ? 0 : 1;
		const pane = this.split.panes[paneIndex];

		if (!pane.tabs.some((t) => t.id === tabId)) return;

		this.pushToHistory(tabId);

		const newPanes = [...this.split.panes] as [PaneState, PaneState];
		newPanes[paneIndex] = { ...newPanes[paneIndex], activeTabId: tabId };
		this.split = { ...this.split, panes: newPanes, activePaneId: paneId };
		this.persistTabState();
	}

	moveTabToPane(tabId: string, targetPaneId: 'left' | 'right'): void {
		if (!this.split.enabled || !this.split.panes) return;

		const leftPane = this.split.panes[0];
		const rightPane = this.split.panes[1];

		const inLeft = leftPane.tabs.some((t) => t.id === tabId);
		const inRight = rightPane.tabs.some((t) => t.id === tabId);

		if (!inLeft && !inRight) return;

		const sourcePaneId = inLeft ? 'left' : 'right';
		if (sourcePaneId === targetPaneId) return;

		const sourcePane = sourcePaneId === 'left' ? leftPane : rightPane;
		const targetPane = targetPaneId === 'left' ? leftPane : rightPane;

		const tab = sourcePane.tabs.find((t) => t.id === tabId);
		if (!tab) return;

		const newSourceTabs = sourcePane.tabs.filter((t) => t.id !== tabId);
		let newSourceActiveId = sourcePane.activeTabId;
		if (newSourceActiveId === tabId) {
			newSourceActiveId = newSourceTabs[0]?.id || null;
		}

		const newTargetTabs = [...targetPane.tabs, tab];

		const newLeftPane: PaneState = sourcePaneId === 'left'
			? { ...leftPane, tabs: newSourceTabs, activeTabId: newSourceActiveId }
			: { ...leftPane, tabs: newTargetTabs, activeTabId: tabId };

		const newRightPane: PaneState = sourcePaneId === 'right'
			? { ...rightPane, tabs: newSourceTabs, activeTabId: newSourceActiveId }
			: { ...rightPane, tabs: newTargetTabs, activeTabId: tabId };

		this.split = { ...this.split, panes: [newLeftPane, newRightPane], activePaneId: targetPaneId };

		if (newSourceTabs.length === 0) {
			this.disableSplit();
		} else {
			this.persistTabState();
		}
	}

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

		this.persistTabState();
	}

	openSessionContext(conversationId: string, currentPaneId: 'left' | 'right' | null): string {
		const targetPaneId = currentPaneId === 'right' ? 'left' : 'right';

		const existing = this.findTab((t) => t.type === 'session-context' && t.linkedConversationId === conversationId);
		if (existing) {
			if (existing.paneId) {
				this.setActiveTabInPane(existing.tab.id, existing.paneId);
			} else {
				this.setActiveTab(existing.tab.id);
			}
			return existing.tab.id;
		}

		if (!this.split.enabled) {
			this.enableSplit();
		}

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
		console.log('[WorkspaceStore Debug]', {
			workspaces: this.workspaces,
			activeWorkspaceId: this.activeWorkspaceId,
			tree: this.tree,
			tabs: this.tabs,
			activeTabId: this.activeTabId,
			split: this.split,
			registry: Object.fromEntries(this.registry)
		});
	}
}

// ============================================================================
// Export singleton
// ============================================================================

export const workspaceStore = new WorkspaceStore();

// Expose to window for debugging
if (typeof window !== 'undefined') {
	(window as unknown as { workspaceStore: WorkspaceStore }).workspaceStore = workspaceStore;
}
