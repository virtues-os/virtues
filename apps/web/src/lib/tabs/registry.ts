/**
 * Tab Registry - Namespace-based tab definitions with URL routing.
 *
 * URL Patterns:
 * - Entity namespaces: /{namespace} (list) or /{namespace}/{namespace}_{id} (detail)
 * - Storage: /drive, /drive/{path}
 * - System: /virtues/{page}
 * - Easter eggs: /life, /jump
 */

import type { Component } from 'svelte';
import type { TabType, ParsedRoute } from './types';

// Import all view components
import ChatView from '$lib/components/tabs/views/ChatView.svelte';
import WikiView from '$lib/components/tabs/views/WikiView.svelte';
import WikiDetailView from '$lib/components/tabs/views/WikiDetailView.svelte';
import WikiListView from '$lib/components/tabs/views/WikiListView.svelte';
import DataSourcesView from '$lib/components/tabs/views/DataSourcesView.svelte';
import DataSourceDetailView from '$lib/components/tabs/views/DataSourceDetailView.svelte';
import UsageView from '$lib/components/tabs/views/UsageView.svelte';
import JobsView from '$lib/components/tabs/views/JobsView.svelte';
import ProfileView from '$lib/components/tabs/views/ProfileView.svelte';
import DriveView from '$lib/components/tabs/views/DriveView.svelte';
import TrashView from '$lib/components/tabs/views/TrashView.svelte';
import DeveloperSqlView from '$lib/components/tabs/views/DeveloperSqlView.svelte';
import DeveloperTerminalView from '$lib/components/tabs/views/DeveloperTerminalView.svelte';
import DeveloperSitemapView from '$lib/components/tabs/views/DeveloperSitemapView.svelte';
import DeveloperLakeView from '$lib/components/tabs/views/DeveloperLakeView.svelte';
// AddSourceView removed - source connection now handled via modals in DataSourcesView
import FeedbackView from '$lib/components/tabs/views/FeedbackView.svelte';
import ConwayView from '$lib/components/tabs/views/ConwayView.svelte';
import DogJumpView from '$lib/components/tabs/views/DogJumpView.svelte';
import PagesView from '$lib/components/tabs/views/PagesView.svelte';
import PageDetailView from '$lib/components/tabs/views/PageDetailView.svelte';

export interface TabDefinition {
	// Route matching
	match: (path: string, params: URLSearchParams) => boolean;
	parse: (path: string, params: URLSearchParams) => ParsedRoute;

	// Serialization (for URL sharing)
	serialize: (id?: string) => string;
	deserialize: (serialized: string) => string; // returns route

	// Metadata
	icon: string;
	defaultLabel: string;

	// Component reference
	// biome-ignore lint/suspicious/noExplicitAny: Component props vary by tab type
	component: Component<any>;

	// Optional: detail component for entity namespaces
	// biome-ignore lint/suspicious/noExplicitAny: Component props vary by tab type
	detailComponent?: Component<any>;
}

// Complete tab registry with namespace-based URL patterns
export const tabRegistry: Record<TabType, TabDefinition> = {
	// ========================================================================
	// CHAT NAMESPACE: /, /chat, /chat/chat_{id}
	// ========================================================================
	chat: {
		match: (path) => path === '/' || path === '/chat' || /^\/chat\/chat_[^/]+$/.test(path),
		parse: (path) => {
			// Root or /chat = new chat
			if (path === '/' || path === '/chat') {
				return {
					type: 'chat',
					label: 'New Chat',
					icon: 'ri:chat-1-line',
					normalizedRoute: '/chat',
				};
			}
			// Detail view
			const match = path.match(/^\/chat\/(chat_[^/]+)$/);
			return {
				type: 'chat',
				label: 'Chat',
				icon: 'ri:chat-1-line',
				entityId: match?.[1],
			};
		},
		serialize: (id) => (id ? `chat_${id}` : 'chat'),
		deserialize: (serialized) => {
			if (serialized.startsWith('chat_')) {
				return `/chat/${serialized}`;
			}
			return '/chat';
		},
		icon: 'ri:chat-1-line',
		defaultLabel: 'Chats',
		component: ChatView,
		detailComponent: ChatView,
	},

	// ========================================================================
	// PAGE NAMESPACE: /page, /page/page_{id}
	// ========================================================================
	page: {
		match: (path) => path === '/page' || /^\/page\/page_[^/]+$/.test(path),
		parse: (path) => {
			// List view
			if (path === '/page') {
				return {
					type: 'page',
					label: 'Pages',
					icon: 'ri:file-list-3-line',
				};
			}
			// Detail view
			const match = path.match(/^\/page\/(page_[^/]+)$/);
			return {
				type: 'page',
				label: 'Page',
				icon: 'ri:file-text-line',
				entityId: match?.[1],
			};
		},
		serialize: (id) => (id ? `page_${id}` : 'page'),
		deserialize: (serialized) => {
			if (serialized.startsWith('page_')) {
				return `/page/${serialized}`;
			}
			return '/page';
		},
		icon: 'ri:file-list-3-line',
		defaultLabel: 'Pages',
		component: PagesView,
		detailComponent: PageDetailView,
	},

	// ========================================================================
	// WIKI OVERVIEW: /wiki
	// ========================================================================
	wiki: {
		match: (path) => path === '/wiki',
		parse: () => ({
			type: 'wiki',
			label: 'Wiki',
			icon: 'ri:book-2-line',
		}),
		serialize: () => 'wiki',
		deserialize: () => '/wiki',
		icon: 'ri:book-2-line',
		defaultLabel: 'Wiki',
		component: WikiView,
	},

	// ========================================================================
	// PERSON NAMESPACE: /person, /person/person_{id}
	// ========================================================================
	person: {
		match: (path) => path === '/person' || /^\/person\/person_[^/]+$/.test(path),
		parse: (path) => {
			if (path === '/person') {
				return {
					type: 'person',
					label: 'People',
					icon: 'ri:user-line',
				};
			}
			const match = path.match(/^\/person\/(person_[^/]+)$/);
			return {
				type: 'person',
				label: 'Person',
				icon: 'ri:user-line',
				entityId: match?.[1],
			};
		},
		serialize: (id) => (id ? `person_${id}` : 'person'),
		deserialize: (serialized) => {
			if (serialized.startsWith('person_')) {
				return `/person/${serialized}`;
			}
			return '/person';
		},
		icon: 'ri:user-line',
		defaultLabel: 'People',
		component: WikiListView,
		detailComponent: WikiDetailView,
	},

	// ========================================================================
	// PLACE NAMESPACE: /place, /place/place_{id}
	// ========================================================================
	place: {
		match: (path) => path === '/place' || /^\/place\/place_[^/]+$/.test(path),
		parse: (path) => {
			if (path === '/place') {
				return {
					type: 'place',
					label: 'Places',
					icon: 'ri:map-pin-line',
				};
			}
			const match = path.match(/^\/place\/(place_[^/]+)$/);
			return {
				type: 'place',
				label: 'Place',
				icon: 'ri:map-pin-line',
				entityId: match?.[1],
			};
		},
		serialize: (id) => (id ? `place_${id}` : 'place'),
		deserialize: (serialized) => {
			if (serialized.startsWith('place_')) {
				return `/place/${serialized}`;
			}
			return '/place';
		},
		icon: 'ri:map-pin-line',
		defaultLabel: 'Places',
		component: WikiListView,
		detailComponent: WikiDetailView,
	},

	// ========================================================================
	// ORG NAMESPACE: /org, /org/org_{id}
	// ========================================================================
	org: {
		match: (path) => path === '/org' || /^\/org\/org_[^/]+$/.test(path),
		parse: (path) => {
			if (path === '/org') {
				return {
					type: 'org',
					label: 'Organizations',
					icon: 'ri:building-line',
				};
			}
			const match = path.match(/^\/org\/(org_[^/]+)$/);
			return {
				type: 'org',
				label: 'Organization',
				icon: 'ri:building-line',
				entityId: match?.[1],
			};
		},
		serialize: (id) => (id ? `org_${id}` : 'org'),
		deserialize: (serialized) => {
			if (serialized.startsWith('org_')) {
				return `/org/${serialized}`;
			}
			return '/org';
		},
		icon: 'ri:building-line',
		defaultLabel: 'Organizations',
		component: WikiListView,
		detailComponent: WikiDetailView,
	},

	// ========================================================================
	// DAY NAMESPACE: /day, /day/day_{date}
	// ========================================================================
	day: {
		match: (path) => path === '/day' || /^\/day\/day_\d{4}-\d{2}-\d{2}$/.test(path),
		parse: (path) => {
			if (path === '/day') {
				// Default to today - normalize route to include date
				const today = new Date().toISOString().split('T')[0];
				return {
					type: 'day',
					label: 'Today',
					icon: 'ri:calendar-line',
					entityId: `day_${today}`,
					normalizedRoute: `/day/day_${today}`,
				};
			}
			const match = path.match(/^\/day\/(day_\d{4}-\d{2}-\d{2})$/);
			const dateStr = match?.[1]?.replace('day_', '') || '';
			return {
				type: 'day',
				label: dateStr,
				icon: 'ri:calendar-line',
				entityId: match?.[1],
			};
		},
		serialize: (id) => (id ? `day_${id}` : 'day'),
		deserialize: (serialized) => {
			if (serialized.startsWith('day_')) {
				return `/day/${serialized}`;
			}
			return '/day';
		},
		icon: 'ri:calendar-line',
		defaultLabel: 'Today',
		component: WikiDetailView,
		detailComponent: WikiDetailView,
	},

	// ========================================================================
	// YEAR NAMESPACE: /year, /year/year_{year}
	// ========================================================================
	year: {
		match: (path) => path === '/year' || /^\/year\/year_\d{4}$/.test(path),
		parse: (path) => {
			if (path === '/year') {
				// Default to current year - normalize route to include year
				const currentYear = new Date().getFullYear();
				return {
					type: 'year',
					label: String(currentYear),
					icon: 'ri:calendar-line',
					entityId: `year_${currentYear}`,
					normalizedRoute: `/year/year_${currentYear}`,
				};
			}
			const match = path.match(/^\/year\/(year_\d{4})$/);
			const yearStr = match?.[1]?.replace('year_', '') || '';
			return {
				type: 'year',
				label: yearStr,
				icon: 'ri:calendar-line',
				entityId: match?.[1],
			};
		},
		serialize: (id) => (id ? `year_${id}` : 'year'),
		deserialize: (serialized) => {
			if (serialized.startsWith('year_')) {
				return `/year/${serialized}`;
			}
			return '/year';
		},
		icon: 'ri:calendar-line',
		defaultLabel: 'Year',
		component: WikiDetailView,
		detailComponent: WikiDetailView,
	},

	// ========================================================================
	// SOURCE NAMESPACE: /source, /source/source_{id}
	// Note: /source/add now redirects to /source (modals handle add flow)
	// ========================================================================
	source: {
		match: (path) =>
			path === '/source' || path === '/source/add' || /^\/source\/source_[^/]+$/.test(path),
		parse: (path) => {
			// Redirect /source/add to /source (add flow now uses modals)
			if (path === '/source/add' || path === '/source') {
				return {
					type: 'source',
					label: 'Sources',
					icon: 'ri:database-2-line',
					normalizedRoute: '/source', // Ensure /source/add redirects to /source
				};
			}
			// Detail view
			const match = path.match(/^\/source\/(source_[^/]+)$/);
			return {
				type: 'source',
				label: 'Source',
				icon: 'ri:database-2-line',
				entityId: match?.[1],
			};
		},
		serialize: (id) => {
			if (id) return `source_${id}`;
			return 'source';
		},
		deserialize: (serialized) => {
			if (serialized.startsWith('source_')) {
				return `/source/${serialized}`;
			}
			return '/source';
		},
		icon: 'ri:database-2-line',
		defaultLabel: 'Sources',
		component: DataSourcesView,
		detailComponent: DataSourceDetailView,
	},

	// ========================================================================
	// DRIVE NAMESPACE: /drive, /drive/{path}
	// ========================================================================
	drive: {
		match: (path) => path === '/drive' || path.startsWith('/drive/'),
		parse: (path) => {
			if (path === '/drive') {
				return {
					type: 'drive',
					label: 'Drive',
					icon: 'ri:hard-drive-2-line',
				};
			}
			const storagePath = path.replace('/drive/', '');
			const fileName = storagePath.split('/').pop() || 'File';
			return {
				type: 'drive',
				label: fileName,
				icon: 'ri:file-line',
				storagePath,
			};
		},
		serialize: (id) => (id ? `drive_${encodeURIComponent(id)}` : 'drive'),
		deserialize: (serialized) => {
			if (serialized.startsWith('drive_')) {
				const path = decodeURIComponent(serialized.slice(6));
				return `/drive/${path}`;
			}
			return '/drive';
		},
		icon: 'ri:hard-drive-2-line',
		defaultLabel: 'Drive',
		component: DriveView,
	},

	// ========================================================================
	// TRASH: /trash
	// ========================================================================
	trash: {
		match: (path) => path === '/trash',
		parse: () => ({
			type: 'trash',
			label: 'Trash',
			icon: 'ri:delete-bin-line',
		}),
		serialize: () => 'trash',
		deserialize: () => '/trash',
		icon: 'ri:delete-bin-line',
		defaultLabel: 'Trash',
		component: TrashView,
	},

	// ========================================================================
	// VIRTUES NAMESPACE: /virtues/{page}
	// System pages: account, assistant, usage, jobs, sql, terminal, sitemap, feedback
	// ========================================================================
	virtues: {
		match: (path) => path.startsWith('/virtues/'),
		parse: (path) => {
			const page = path.replace('/virtues/', '');

			const pageConfig: Record<string, { label: string; icon: string }> = {
				account: { label: 'Account', icon: 'ri:user-settings-line' },
				assistant: { label: 'Assistant', icon: 'ri:robot-line' },
				usage: { label: 'Usage', icon: 'ri:bar-chart-line' },
				jobs: { label: 'Jobs', icon: 'ri:refresh-line' },
				lake: { label: 'Lake', icon: 'ri:database-2-line' },
				sql: { label: 'SQL', icon: 'ri:database-2-line' },
				terminal: { label: 'Terminal', icon: 'ri:terminal-box-line' },
				sitemap: { label: 'Sitemap', icon: 'ri:road-map-line' },
				feedback: { label: 'Feedback', icon: 'ri:feedback-line' },
			};

			const config = pageConfig[page] || { label: 'Virtues', icon: 'ri:compass-3-line' };
			return {
				type: 'virtues',
				label: config.label,
				icon: config.icon,
				virtuesPage: page,
			};
		},
		serialize: (id) => (id ? `virtues_${id}` : 'virtues'),
		deserialize: (serialized) => {
			if (serialized.startsWith('virtues_')) {
				return `/virtues/${serialized.slice(8)}`;
			}
			return '/virtues/account';
		},
		icon: 'ri:compass-3-line',
		defaultLabel: 'Virtues',
		component: ProfileView, // Will dispatch to correct component based on virtuesPage
	},

	// ========================================================================
	// EASTER EGGS
	// ========================================================================
	conway: {
		match: (path) => path === '/life',
		parse: () => ({
			type: 'conway',
			label: 'Zen Garden',
			icon: 'ri:seedling-line',
		}),
		serialize: () => 'conway',
		deserialize: () => '/life',
		icon: 'ri:seedling-line',
		defaultLabel: 'Zen Garden',
		component: ConwayView,
	},

	'dog-jump': {
		match: (path) => path === '/jump',
		parse: () => ({
			type: 'dog-jump',
			label: 'Dog Jump',
			icon: 'ri:mickey-line',
		}),
		serialize: () => 'dog-jump',
		deserialize: () => '/jump',
		icon: 'ri:mickey-line',
		defaultLabel: 'Dog Jump',
		component: DogJumpView,
	},
};

/**
 * Get the appropriate component for a tab type and whether it's a detail view.
 */
// biome-ignore lint/suspicious/noExplicitAny: Component props vary by tab type
export function getComponent(type: TabType, hasEntityId: boolean): Component<any> {
	const def = tabRegistry[type];
	if (hasEntityId && def.detailComponent) {
		return def.detailComponent;
	}
	return def.component;
}

/**
 * Get the component for virtues pages (system pages).
 */
// biome-ignore lint/suspicious/noExplicitAny: Component props vary by page
export function getVirtuesComponent(page: string): Component<any> {
	const componentMap: Record<string, Component<any>> = {
		account: ProfileView,
		assistant: ProfileView,
		usage: UsageView,
		jobs: JobsView,
		lake: DeveloperLakeView,
		sql: DeveloperSqlView,
		terminal: DeveloperTerminalView,
		sitemap: DeveloperSitemapView,
		feedback: FeedbackView,
	};
	return componentMap[page] || ProfileView;
}

/**
 * Get the component for source pages.
 * Note: Add flow is now handled via modals in DataSourcesView.
 */
// biome-ignore lint/suspicious/noExplicitAny: Component props vary
export function getSourceComponent(hasEntityId: boolean): Component<any> {
	if (hasEntityId) return DataSourceDetailView;
	return DataSourcesView;
}

/**
 * Parse a route string into tab metadata using the registry.
 */
export function parseRoute(route: string): ParsedRoute {
	const url = new URL(route, 'http://localhost');
	const path = url.pathname;
	const params = url.searchParams;

	// Try to match against registry in priority order
	// Note: Order matters for overlapping patterns
	const orderedTypes: TabType[] = [
		// Specific patterns first
		'source', // Source list and detail views
		'virtues', // Has /virtues/* pattern
		'drive', // Has /drive/* pattern
		'trash', // Drive trash
		// Entity namespaces
		'chat', // Also matches /
		'page',
		'wiki', // Wiki overview page
		'person',
		'place',
		'org',
		'day',
		'year',
		// Easter eggs last
		'conway',
		'dog-jump',
	];

	for (const type of orderedTypes) {
		const def = tabRegistry[type];
		if (def.match(path, params)) {
			return def.parse(path, params);
		}
	}

	// Fallback to chat
	return {
		type: 'chat',
		label: 'New Chat',
		icon: 'ri:chat-1-line',
	};
}

/**
 * Check if a route matches a namespace list view (no entity ID).
 */
export function isListView(route: string): boolean {
	const parsed = parseRoute(route);
	return !parsed.entityId && !parsed.storagePath && !parsed.virtuesPage;
}

/**
 * Check if a route matches a namespace detail view (has entity ID).
 */
export function isDetailView(route: string): boolean {
	const parsed = parseRoute(route);
	return !!parsed.entityId;
}

// Legacy support: Get detail component for wiki types
// biome-ignore lint/suspicious/noExplicitAny: Component props vary by tab type
export function getDetailComponent(type: TabType): Component<any> | null {
	const def = tabRegistry[type];
	return def.detailComponent || null;
}
