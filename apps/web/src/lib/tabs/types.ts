/**
 * Tab type definitions.
 * 
 * Uses a flat interface with optional properties for backwards compatibility.
 * Type guards are provided for type narrowing when stricter types are needed.
 */

// All supported tab types
export type TabType =
	| 'chat'
	| 'history'
	| 'session-context'
	| 'pages'
	| 'page-detail'
	| 'wiki'
	| 'wiki-list'
	| 'data-sources'
	| 'data-sources-add'
	| 'data-entities'
	| 'data-jobs'
	| 'data-drive'
	| 'usage'
	| 'profile'
	| 'developer-sql'
	| 'developer-terminal'
	| 'feedback'
	| 'conway'
	| 'dog-jump';

/**
 * Tab interface - flat structure with optional type-specific properties.
 * This maintains backwards compatibility with existing views.
 */
export interface Tab {
	id: string;
	type: TabType;
	label: string;
	route: string;
	icon?: string;
	pinned?: boolean;

	// Type-specific data (optional for all)
	conversationId?: string; // For chat tabs
	linkedConversationId?: string; // For session-context tabs (links to chat)
	pageId?: string; // For page-detail tabs
	slug?: string; // For wiki entity tabs
	sourceId?: string; // For data source detail tabs
	wikiCategory?: string; // For wiki-list tabs (people, places, etc.)
	profileSection?: string; // For profile tabs (account, assistant)

	scrollPosition?: number;
	createdAt: number;
}

// Type guard helpers for narrowing
export function isChatTab(tab: Tab): tab is Tab & { type: 'chat' } {
	return tab.type === 'chat';
}

export function isSessionContextTab(tab: Tab): tab is Tab & { type: 'session-context' } {
	return tab.type === 'session-context';
}

export function isPageDetailTab(tab: Tab): tab is Tab & { type: 'page-detail' } {
	return tab.type === 'page-detail';
}

export function isWikiTab(tab: Tab): tab is Tab & { type: 'wiki' } {
	return tab.type === 'wiki';
}

export function isWikiListTab(tab: Tab): tab is Tab & { type: 'wiki-list' } {
	return tab.type === 'wiki-list';
}

export function isDataSourcesTab(tab: Tab): tab is Tab & { type: 'data-sources' } {
	return tab.type === 'data-sources';
}

export function isProfileTab(tab: Tab): tab is Tab & { type: 'profile' } {
	return tab.type === 'profile';
}

// Fallback view preference type
export type FallbackView = 'empty' | 'chat' | 'conway' | 'dog-jump' | 'wiki-today';

// Domain groups for hybrid navigation (same domain = navigate in place)
export type TabDomain = 'chat' | 'pages' | 'wiki' | 'data' | 'settings' | 'developer';

export function getTabDomain(type: TabType): TabDomain {
	switch (type) {
		case 'chat':
		case 'history':
		case 'session-context':
			return 'chat';
		case 'pages':
		case 'page-detail':
			return 'pages';
		case 'wiki':
		case 'wiki-list':
			return 'wiki';
		case 'data-sources':
		case 'data-sources-add':
		case 'data-entities':
		case 'data-jobs':
		case 'data-drive':
		case 'usage':
			return 'data';
		case 'profile':
		case 'feedback':
			return 'settings';
		case 'developer-sql':
		case 'developer-terminal':
			return 'developer';
		default:
			return 'chat';
	}
}

// Split screen state
export interface PaneState {
	id: 'left' | 'right';
	tabs: Tab[];
	activeTabId: string | null;
	width: number; // percentage (e.g., 50)
}

export interface SplitState {
	enabled: boolean;
	panes: [PaneState, PaneState] | null;
	activePaneId: 'left' | 'right';
}

// Route parsing result (used by parseRoute)
export interface ParsedRoute {
	type: TabType;
	label: string;
	icon: string;
	conversationId?: string;
	linkedConversationId?: string;
	pageId?: string;
	slug?: string;
	sourceId?: string;
	wikiCategory?: string;
	profileSection?: string;
}
