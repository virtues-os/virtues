/**
 * Tab Registry - Declarative tab definitions with dynamic component dispatch.
 * 
 * Replaces the if-else chain in TabContent.svelte with a registry-based approach.
 * Each tab type has its own definition including:
 * - Route matching and parsing
 * - URL serialization/deserialization
 * - Metadata (icon, label)
 * - Component reference
 */

import type { Component } from 'svelte';
import type { TabType, ParsedRoute } from './types';

// Import all view components
import ChatView from '$lib/components/tabs/views/ChatView.svelte';
import HistoryView from '$lib/components/tabs/views/HistoryView.svelte';
import ContextView from '$lib/components/tabs/views/ContextView.svelte';
import WikiView from '$lib/components/tabs/views/WikiView.svelte';
import WikiDetailView from '$lib/components/tabs/views/WikiDetailView.svelte';
import WikiListView from '$lib/components/tabs/views/WikiListView.svelte';
import DataSourcesView from '$lib/components/tabs/views/DataSourcesView.svelte';
import DataSourceDetailView from '$lib/components/tabs/views/DataSourceDetailView.svelte';
import UsageView from '$lib/components/tabs/views/UsageView.svelte';
import JobsView from '$lib/components/tabs/views/JobsView.svelte';
import ProfileView from '$lib/components/tabs/views/ProfileView.svelte';
import EntitiesView from '$lib/components/tabs/views/EntitiesView.svelte';
import DriveView from '$lib/components/tabs/views/DriveView.svelte';
import DeveloperSqlView from '$lib/components/tabs/views/DeveloperSqlView.svelte';
import DeveloperTerminalView from '$lib/components/tabs/views/DeveloperTerminalView.svelte';
import AddSourceView from '$lib/components/tabs/views/AddSourceView.svelte';
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

	// Component reference (synchronous for simplicity)
	// biome-ignore lint/suspicious/noExplicitAny: Component props vary by tab type
	component: Component<any>;

	// Optional: determines if a tab needs a detail variant
	hasDetail?: boolean;
}

// Complete tab registry
export const tabRegistry: Record<TabType, TabDefinition> = {
	'chat': {
		match: (path) => path === '/',
		parse: (_path, params) => ({
			type: 'chat',
			label: params.get('conversationId') ? 'Chat' : 'New Chat',
			icon: 'ri:chat-1-line',
			conversationId: params.get('conversationId') || undefined,
		}),
		serialize: (id) => id ? `chat_${id}` : 'chat',
		deserialize: (serialized) => {
			const id = serialized.startsWith('chat_') ? serialized.slice(5) : undefined;
			return id ? `/?conversationId=${id}` : '/';
		},
		icon: 'ri:chat-1-line',
		defaultLabel: 'New Chat',
		component: ChatView,
	},

	'history': {
		match: (path) => path === '/history',
		parse: () => ({
			type: 'history',
			label: 'History',
			icon: 'ri:history-line',
		}),
		serialize: () => 'history',
		deserialize: () => '/history',
		icon: 'ri:history-line',
		defaultLabel: 'History',
		component: HistoryView,
	},

	'session-context': {
		match: (path) => /^\/context\/[^/]+$/.test(path),
		parse: (path) => {
			const match = path.match(/^\/context\/([^/]+)$/);
			return {
				type: 'session-context',
				label: 'Context',
				icon: 'ri:information-line',
				linkedConversationId: match?.[1] || '',
			};
		},
		serialize: (id) => id ? `session-context_${id}` : 'session-context',
		deserialize: (serialized) => {
			const id = serialized.startsWith('session-context_') ? serialized.slice(16) : 'unknown';
			return `/context/${id}`;
		},
		icon: 'ri:information-line',
		defaultLabel: 'Context',
		component: ContextView,
	},

	'pages': {
		match: (path) => path === '/pages',
		parse: () => ({
			type: 'pages',
			label: 'Pages',
			icon: 'ri:file-list-3-line',
		}),
		serialize: () => 'pages',
		deserialize: () => '/pages',
		icon: 'ri:file-list-3-line',
		defaultLabel: 'Pages',
		component: PagesView,
	},

	'page-detail': {
		match: (path) => /^\/pages\/[^/]+$/.test(path),
		parse: (path) => {
			const match = path.match(/^\/pages\/([^/]+)$/);
			return {
				type: 'page-detail',
				label: 'Page',
				icon: 'ri:file-text-line',
				pageId: match?.[1] || '',
			};
		},
		serialize: (id) => id ? `page-detail_${id}` : 'page-detail',
		deserialize: (serialized) => {
			const id = serialized.startsWith('page-detail_') ? serialized.slice(12) : '';
			return id ? `/pages/${id}` : '/pages';
		},
		icon: 'ri:file-text-line',
		defaultLabel: 'Page',
		component: PageDetailView,
	},

	'wiki': {
		match: (path) => path === '/wiki' || /^\/wiki\/[^/]+$/.test(path),
		parse: (path) => {
			const match = path.match(/^\/wiki\/([^/]+)$/);
			const slug = match?.[1];
			// Don't match wiki list categories
			if (slug && ['people', 'places', 'orgs', 'things'].includes(slug)) {
				return { type: 'wiki', label: 'Wiki', icon: 'ri:book-2-line' };
			}
			return {
				type: 'wiki',
				label: 'Wiki',
				icon: 'ri:book-2-line',
				slug: slug || undefined,
			};
		},
		serialize: (id) => id ? `wiki_${id}` : 'wiki',
		deserialize: (serialized) => {
			const id = serialized.startsWith('wiki_') ? serialized.slice(5) : undefined;
			return id ? `/wiki/${id}` : '/wiki';
		},
		icon: 'ri:book-2-line',
		defaultLabel: 'Wiki',
		// Component varies based on whether there's a slug
		component: WikiView,
		hasDetail: true,
	},

	'wiki-list': {
		match: (path) => /^\/wiki\/(people|places|orgs|things)$/.test(path),
		parse: (path) => {
			const match = path.match(/^\/wiki\/(people|places|orgs|things)$/);
			const category = match?.[1] || 'people';
			const labels: Record<string, string> = {
				people: 'People',
				places: 'Places',
				orgs: 'Orgs',
				things: 'Things',
			};
			const icons: Record<string, string> = {
				people: 'ri:user-line',
				places: 'ri:map-pin-line',
				orgs: 'ri:building-line',
				things: 'ri:box-3-line',
			};
			return {
				type: 'wiki-list',
				label: labels[category] || 'Wiki',
				icon: icons[category] || 'ri:list-unordered',
				wikiCategory: category,
			};
		},
		serialize: (id) => id ? `wiki-list_${id}` : 'wiki-list',
		deserialize: (serialized) => {
			const id = serialized.startsWith('wiki-list_') ? serialized.slice(10) : 'people';
			return `/wiki/${id}`;
		},
		icon: 'ri:list-unordered',
		defaultLabel: 'Wiki List',
		component: WikiListView,
	},

	'data-sources': {
		match: (path) => path === '/data/sources' || (/^\/data\/sources\/[^/]+$/.test(path) && path !== '/data/sources/add'),
		parse: (path) => {
			const match = path.match(/^\/data\/sources\/([^/]+)$/);
			const sourceId = match?.[1];
			if (sourceId === 'add') {
				return { type: 'data-sources', label: 'Sources', icon: 'ri:database-2-line' };
			}
			return {
				type: 'data-sources',
				label: sourceId ? 'Source' : 'Sources',
				icon: 'ri:database-2-line',
				sourceId: sourceId || undefined,
			};
		},
		serialize: (id) => id ? `data-sources_${id}` : 'data-sources',
		deserialize: (serialized) => {
			const id = serialized.startsWith('data-sources_') ? serialized.slice(13) : undefined;
			return id ? `/data/sources/${id}` : '/data/sources';
		},
		icon: 'ri:database-2-line',
		defaultLabel: 'Sources',
		// Component varies based on whether there's a sourceId
		component: DataSourcesView,
		hasDetail: true,
	},

	'data-sources-add': {
		match: (path) => path === '/data/sources/add',
		parse: () => ({
			type: 'data-sources-add',
			label: 'Add Source',
			icon: 'ri:add-circle-line',
		}),
		serialize: () => 'data-sources-add',
		deserialize: () => '/data/sources/add',
		icon: 'ri:add-circle-line',
		defaultLabel: 'Add Source',
		component: AddSourceView,
	},

	'data-entities': {
		match: (path) => path === '/data/entities',
		parse: () => ({
			type: 'data-entities',
			label: 'Entities',
			icon: 'ri:node-tree',
		}),
		serialize: () => 'data-entities',
		deserialize: () => '/data/entities',
		icon: 'ri:node-tree',
		defaultLabel: 'Entities',
		component: EntitiesView,
	},

	'data-jobs': {
		match: (path) => path === '/data/jobs',
		parse: () => ({
			type: 'data-jobs',
			label: 'Jobs',
			icon: 'ri:refresh-line',
		}),
		serialize: () => 'data-jobs',
		deserialize: () => '/data/jobs',
		icon: 'ri:refresh-line',
		defaultLabel: 'Jobs',
		component: JobsView,
	},

	'data-drive': {
		match: (path) => path === '/data/drive',
		parse: () => ({
			type: 'data-drive',
			label: 'Drive',
			icon: 'ri:folder-line',
		}),
		serialize: () => 'data-drive',
		deserialize: () => '/data/drive',
		icon: 'ri:folder-line',
		defaultLabel: 'Drive',
		component: DriveView,
	},

	'usage': {
		match: (path) => path === '/usage',
		parse: () => ({
			type: 'usage',
			label: 'Usage',
			icon: 'ri:bar-chart-line',
		}),
		serialize: () => 'usage',
		deserialize: () => '/usage',
		icon: 'ri:bar-chart-line',
		defaultLabel: 'Usage',
		component: UsageView,
	},

	'profile': {
		match: (path) => path.startsWith('/profile'),
		parse: (path) => {
			const sections: Record<string, { label: string; icon: string }> = {
				account: { label: 'Account', icon: 'ri:user-settings-line' },
				assistant: { label: 'Assistant', icon: 'ri:robot-line' },
			};
			const section = path.split('/')[2] || 'account';
			const info = sections[section] || sections.account;
			return {
				type: 'profile',
				label: info.label,
				icon: info.icon,
				profileSection: section,
			};
		},
		serialize: (id) => id ? `profile_${id}` : 'profile',
		deserialize: (serialized) => {
			const id = serialized.startsWith('profile_') ? serialized.slice(8) : 'account';
			return `/profile/${id}`;
		},
		icon: 'ri:user-settings-line',
		defaultLabel: 'Profile',
		component: ProfileView,
	},

	'developer-sql': {
		match: (path) => path === '/developer/sql-viewer',
		parse: () => ({
			type: 'developer-sql',
			label: 'SQL Viewer',
			icon: 'ri:database-2-line',
		}),
		serialize: () => 'developer-sql',
		deserialize: () => '/developer/sql-viewer',
		icon: 'ri:database-2-line',
		defaultLabel: 'SQL Viewer',
		component: DeveloperSqlView,
	},

	'developer-terminal': {
		match: (path) => path === '/developer/terminal',
		parse: () => ({
			type: 'developer-terminal',
			label: 'Terminal',
			icon: 'ri:terminal-box-line',
		}),
		serialize: () => 'developer-terminal',
		deserialize: () => '/developer/terminal',
		icon: 'ri:terminal-box-line',
		defaultLabel: 'Terminal',
		component: DeveloperTerminalView,
	},

	'feedback': {
		match: (path) => path === '/feedback',
		parse: () => ({
			type: 'feedback',
			label: 'Feedback',
			icon: 'ri:feedback-line',
		}),
		serialize: () => 'feedback',
		deserialize: () => '/feedback',
		icon: 'ri:feedback-line',
		defaultLabel: 'Feedback',
		component: FeedbackView,
	},

	'conway': {
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

// Helper to get detail component for types that have one
// biome-ignore lint/suspicious/noExplicitAny: Component props vary by tab type
export function getDetailComponent(type: TabType): Component<any> | null {
	if (type === 'wiki') return WikiDetailView;
	if (type === 'data-sources') return DataSourceDetailView;
	return null;
}

/**
 * Parse a route string into tab metadata using the registry.
 * This replaces the old parseRoute function in windowTabs.svelte.ts.
 */
export function parseRoute(route: string): ParsedRoute {
	const url = new URL(route, 'http://localhost');
	const path = url.pathname;
	const params = url.searchParams;

	// Try to match against registry in priority order
	// Note: Order matters for overlapping patterns
	const orderedTypes: TabType[] = [
		// Specific routes first
		'data-sources-add',
		'session-context',
		'page-detail',
		'wiki-list',
		'developer-sql',
		'developer-terminal',
		// Then generic routes
		'chat',
		'history',
		'pages',
		'wiki',
		'data-sources',
		'data-entities',
		'data-jobs',
		'data-drive',
		'usage',
		'profile',
		'feedback',
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
		label: route === '/' ? 'New Chat' : 'Chat',
		icon: 'ri:chat-1-line',
	};
}
