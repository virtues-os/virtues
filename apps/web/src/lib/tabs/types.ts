/**
 * Tab type definitions for namespace-based URL routing.
 *
 * URL structure:
 * - Entity namespaces: /{namespace} (list) or /{namespace}/{namespace}_{id} (detail)
 * - Storage namespaces: /drive, /lake (with subpaths)
 * - System namespace: /virtues/{page}
 */

// All supported tab types - consolidated namespace-based types
export type TabType =
	// Entity namespaces (SQLite backend)
	| 'chat' // Chat conversations: /, /chat, /chat/chat_{id}
	| 'chat-history' // Chat history list: /chat-history
	| 'page' // User documents: /page, /page/page_{id}
	| 'wiki' // Wiki overview: /wiki
	| 'person' // Wiki people: /person, /person/person_{id}
	| 'place' // Wiki places: /place, /place/place_{id}
	| 'org' // Wiki organizations: /org, /org/org_{id}
	| 'day' // Wiki days: /day, /day/day_{date}
	| 'year' // Wiki years: /year, /year/{year}
	| 'narrative-identity' // Wiki narrative identity: /narrative-identity
	| 'source' // Data sources: /source, /source/source_{id}
	// Storage namespaces
	| 'drive' // Personal files: /drive, /drive/{path}
	| 'trash' // Drive trash: /trash
	// View namespace
	| 'view' // Folder/view pages: /view/view_{id}
	// System namespace
	| 'virtues' // System pages: /virtues/{account|assistant|usage|jobs|sql|terminal|sitemap|feedback}
	// Easter eggs
	| 'conway'
	| 'dog-jump';

/**
 * Tab interface - flat structure with optional type-specific properties.
 *
 * Note: Entity IDs (e.g., 'chat_abc123') are derived from `route` using
 * `routeToEntityId(tab.route)`. The route is the source of truth.
 */
export interface Tab {
	id: string;
	type: TabType;
	label: string;
	route: string; // URL-native: '/chat/chat_abc123', '/page/page_xyz'
	icon?: string;
	pinned?: boolean;

	// Storage path (for drive/lake namespaces)
	storagePath?: string; // e.g., 'photos/2024/vacation.jpg'

	// System page (for virtues namespace)
	virtuesPage?: string; // e.g., 'account', 'usage', 'sql'

	scrollPosition?: number;
	createdAt: number;
}

// Type guard helpers for narrowing
export function isChatTab(tab: Tab): tab is Tab & { type: 'chat' } {
	return tab.type === 'chat';
}

export function isPageTab(tab: Tab): tab is Tab & { type: 'page' } {
	return tab.type === 'page';
}

export function isPersonTab(tab: Tab): tab is Tab & { type: 'person' } {
	return tab.type === 'person';
}

export function isPlaceTab(tab: Tab): tab is Tab & { type: 'place' } {
	return tab.type === 'place';
}

export function isOrgTab(tab: Tab): tab is Tab & { type: 'org' } {
	return tab.type === 'org';
}

export function isDayTab(tab: Tab): tab is Tab & { type: 'day' } {
	return tab.type === 'day';
}

export function isYearTab(tab: Tab): tab is Tab & { type: 'year' } {
	return tab.type === 'year';
}

export function isSourceTab(tab: Tab): tab is Tab & { type: 'source' } {
	return tab.type === 'source';
}

export function isDriveTab(tab: Tab): tab is Tab & { type: 'drive' } {
	return tab.type === 'drive';
}

export function isTrashTab(tab: Tab): tab is Tab & { type: 'trash' } {
	return tab.type === 'trash';
}

export function isVirtuesTab(tab: Tab): tab is Tab & { type: 'virtues' } {
	return tab.type === 'virtues';
}

// Pane state - unified model where every tab lives in a pane
export interface PaneState {
	id: string; // 'left', 'right', or could be UUID for future N-pane
	tabs: Tab[];
	activeTabId: string | null;
	width: number; // percentage (e.g., 50 or 100)
}

// Route parsing result (used by parseRoute)
export interface ParsedRoute {
	type: TabType;
	label: string;
	icon: string;
	entityId?: string;
	storagePath?: string;
	virtuesPage?: string;
	/** If set, use this route instead of the original (e.g., /day → /day/day_2026-01-25) */
	normalizedRoute?: string;
}

// Re-export URL utilities for backward compatibility
// These are now defined in $lib/utils/urlUtils.ts
export {
	parseEntityId,
	entityIdToRoute,
	routeToEntityId
} from '$lib/utils/urlUtils';
