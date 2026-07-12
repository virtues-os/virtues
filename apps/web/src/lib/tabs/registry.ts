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
import { getLocalDateSlug } from '$lib/utils/dateUtils';

// Import all view components
import HomeViewSpread from '$lib/components/tabs/views/HomeViewSpread.svelte';
import ChatView from '$lib/components/tabs/views/ChatView.svelte';
import HistoryView from '$lib/components/tabs/views/HistoryView.svelte';
import WikiView from '$lib/components/tabs/views/WikiView.svelte';
import WikiDetailView from '$lib/components/tabs/views/WikiDetailView.svelte';
import WikiListView from '$lib/components/tabs/views/WikiListView.svelte';
import ConnectionsPanel from '$lib/components/actions/ConnectionsPanel.svelte';
import CredentialDetailView from '$lib/components/tabs/views/CredentialDetailView.svelte';
import UsageTab from '$lib/components/tabs/views/UsageTab.svelte';
import TelemetryTab from '$lib/components/tabs/views/TelemetryTab.svelte';
import ActionsView from '$lib/components/tabs/views/ActionsView.svelte';
import ActionDetailView from '$lib/components/tabs/views/ActionDetailView.svelte';
import DevelopersView from '$lib/components/tabs/views/DevelopersView.svelte';
import ProfileView from '$lib/components/tabs/views/ProfileView.svelte';
import AssistantView from '$lib/components/tabs/views/AssistantView.svelte';
import DriveView from '$lib/components/tabs/views/DriveView.svelte';
import AssetView from '$lib/components/tabs/views/AssetView.svelte';
import TrashView from '$lib/components/tabs/views/TrashView.svelte';
import DeveloperSqlView from '$lib/components/tabs/views/DeveloperSqlView.svelte';
import DeveloperTerminalView from '$lib/components/tabs/views/DeveloperTerminalView.svelte';
import DeveloperSitemapView from '$lib/components/tabs/views/DeveloperSitemapView.svelte';
import DeveloperLakeView from '$lib/components/tabs/views/DeveloperLakeView.svelte';
import BillingView from '$lib/components/tabs/views/BillingView.svelte';
import ChangelogView from '$lib/components/tabs/views/ChangelogView.svelte';
import DevicesView from '$lib/components/tabs/views/DevicesView.svelte';
import ActivityView from '$lib/components/tabs/views/ActivityView.svelte';
import ByoKeyView from '$lib/components/tabs/views/ByoKeyView.svelte';
import SystemInfoView from '$lib/components/tabs/views/SystemInfoView.svelte';
import ThisMacView from '$lib/components/tabs/views/ThisMacView.svelte';
import ConwayView from '$lib/components/tabs/views/ConwayView.svelte';
import DogJumpView from '$lib/components/tabs/views/DogJumpView.svelte';
import PagesView from '$lib/components/tabs/views/PagesView.svelte';
import PageDetailView from '$lib/components/tabs/views/PageDetailView.svelte';
import ThingsView from '$lib/components/tabs/views/ThingsView.svelte';
import ThingDetailView from '$lib/components/tabs/views/ThingDetailView.svelte';
import NotebooksListView from '$lib/components/tabs/views/NotebooksListView.svelte';
import NotebookDetailView from '$lib/components/tabs/views/NotebookDetailView.svelte';
import NarrativeIdentityView from '$lib/components/tabs/views/NarrativeIdentityView.svelte';
import EntitiesView from '$lib/components/tabs/views/EntitiesView.svelte';
import ToolsView from '$lib/components/tabs/views/ToolsView.svelte';
import OntologyIndexView from '$lib/components/tabs/views/OntologyIndexView.svelte';
import OntologyDetailView from '$lib/components/tabs/views/OntologyDetailView.svelte';
import DataView from '$lib/components/tabs/views/DataView.svelte';

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
	// HOME: /home — the default landing / "Return" page (synthesis surface)
	// ========================================================================
	home: {
		match: (path) => path === '/home',
		parse: () => ({
			type: 'home',
			label: 'Home',
			icon: 'ri:sparkling-2-line',
		}),
		serialize: () => 'home',
		deserialize: () => '/home',
		icon: 'ri:sparkling-2-line',
		defaultLabel: 'Home',
		component: HomeViewSpread,
	},

	// ========================================================================
	// CHAT NAMESPACE: /, /chat, /chat/chat_{id}
	// ========================================================================
	chat: {
		match: (path) => path === '/' || path === '/chat' || /^\/chat\/chat_[^/]+$/.test(path),
		parse: (path, params) => {
			// Root or /chat = new chat
			if (path === '/' || path === '/chat') {
				// Preserve the temporary/ghost flag so ChatView can start in ghost mode.
				const temporary = params?.get('temporary') === '1';
				return {
					type: 'chat',
					label: temporary ? 'Temporary Chat' : 'New Chat',
					icon: temporary ? 'ri:ghost-line' : 'ri:chat-1-line',
					normalizedRoute: temporary ? '/chat?temporary=1' : '/chat',
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
	// CHAT HISTORY: /chat-history
	// ========================================================================
	'chat-history': {
		match: (path) => path === '/chat-history',
		parse: () => ({
			type: 'chat-history',
			label: 'All Chats',
			icon: 'ri:chat-history-line',
		}),
		serialize: () => 'chat-history',
		deserialize: () => '/chat-history',
		icon: 'ri:chat-history-line',
		defaultLabel: 'All Chats',
		component: HistoryView,
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
	// ENTITIES: /entities (unified entity list)
	// ========================================================================
	entities: {
		match: (path) => path === '/entities',
		parse: () => ({
			type: 'entities',
			label: 'Entities',
			icon: 'ri:group-line',
		}),
		serialize: () => 'entities',
		deserialize: () => '/entities',
		icon: 'ri:group-line',
		defaultLabel: 'Entities',
		component: EntitiesView,
	},

	// ========================================================================
	// PERSON NAMESPACE: /person, /person/{id}
	// ========================================================================
	person: {
		match: (path) => path === '/person' || /^\/person\/[^/]+$/.test(path),
		parse: (path) => {
			if (path === '/person') {
				return {
					type: 'person',
					label: 'People',
					icon: 'ri:user-line',
				};
			}
			const match = path.match(/^\/person\/([^/]+)$/);
			return {
				type: 'person',
				label: 'Person',
				icon: 'ri:user-line',
				entityId: match?.[1],
			};
		},
		serialize: (id) => id || 'person',
		deserialize: (serialized) => {
			if (serialized && serialized !== 'person') {
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
	// PLACE NAMESPACE: /place, /place/{id}
	// ========================================================================
	place: {
		match: (path) => path === '/place' || /^\/place\/[^/]+$/.test(path),
		parse: (path) => {
			if (path === '/place') {
				return {
					type: 'place',
					label: 'Places',
					icon: 'ri:map-pin-line',
				};
			}
			const match = path.match(/^\/place\/([^/]+)$/);
			return {
				type: 'place',
				label: 'Place',
				icon: 'ri:map-pin-line',
				entityId: match?.[1],
			};
		},
		serialize: (id) => id || 'place',
		deserialize: (serialized) => {
			if (serialized && serialized !== 'place') {
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
	// ORG NAMESPACE: /org, /org/{id}
	// ========================================================================
	org: {
		match: (path) => path === '/org' || /^\/org\/[^/]+$/.test(path),
		parse: (path) => {
			if (path === '/org') {
				return {
					type: 'org',
					label: 'Organizations',
					icon: 'ri:building-line',
				};
			}
			const match = path.match(/^\/org\/([^/]+)$/);
			return {
				type: 'org',
				label: 'Organization',
				icon: 'ri:building-line',
				entityId: match?.[1],
			};
		},
		serialize: (id) => id || 'org',
		deserialize: (serialized) => {
			if (serialized && serialized !== 'org') {
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
	// THING NAMESPACE: /things (list), /thing/thg_{id} (detail)
	//
	// A "thing" is a pure reference entity — a pet, car, book, concept: the
	// catch-all beside person/place/org (organization lives in Notebooks now).
	// Sidebar "Things" links to `/things`. The DB has a `category` column for
	// future use but it is not surfaced in v1 UX.
	// ========================================================================
	thing: {
		match: (path) =>
			path === '/things' ||
			path === '/thing' ||
			/^\/thing\/[^/]+$/.test(path),
		parse: (path) => {
			if (path === '/things' || path === '/thing') {
				return {
					type: 'thing',
					label: 'Things',
					icon: 'ri:lightbulb-line',
					normalizedRoute: '/things',
				};
			}
			const match = path.match(/^\/thing\/([^/]+)$/);
			return {
				type: 'thing',
				label: 'Thing',
				icon: 'ri:lightbulb-line',
				entityId: match?.[1],
			};
		},
		serialize: (id) => id || 'things',
		deserialize: (serialized) => {
			if (serialized && serialized !== 'things' && serialized !== 'thing') {
				return `/thing/${serialized}`;
			}
			return '/things';
		},
		icon: 'ri:lightbulb-line',
		defaultLabel: 'Things',
		component: ThingsView,
		detailComponent: ThingDetailView,
	},

	// ========================================================================
	// NOTEBOOK NAMESPACE: /notebooks (list), /notebook/{id} (detail)
	//
	// A Notebook is the "room" a chat lives in — a workspace lens over the graph:
	// a Library of materials, filed chats, entities, and pages. (id may be a
	// legacy `space_…` or a new `nb_…` — both route the same.)
	// ========================================================================
	notebook: {
		match: (path) =>
			path === '/notebooks' ||
			path === '/notebook' ||
			/^\/notebook\/[^/]+$/.test(path),
		parse: (path) => {
			if (path === '/notebooks' || path === '/notebook') {
				return {
					type: 'notebook',
					label: 'Notebooks',
					icon: 'ri:booklet-line',
					normalizedRoute: '/notebooks',
				};
			}
			const match = path.match(/^\/notebook\/([^/]+)$/);
			return {
				type: 'notebook',
				label: 'Notebook',
				icon: 'ri:booklet-line',
				entityId: match?.[1],
			};
		},
		serialize: (id) => id || 'notebooks',
		deserialize: (serialized) => {
			if (serialized && serialized !== 'notebooks' && serialized !== 'notebook') {
				return `/notebook/${serialized}`;
			}
			return '/notebooks';
		},
		icon: 'ri:booklet-line',
		defaultLabel: 'Notebooks',
		component: NotebooksListView,
		detailComponent: NotebookDetailView,
	},

	// ========================================================================
	// DAY NAMESPACE: /day, /day/day_{date}
	// ========================================================================
	day: {
		match: (path) => path === '/day' || /^\/day\/day_\d{4}-\d{2}-\d{2}$/.test(path),
		parse: (path) => {
			if (path === '/day') {
				// Default to today - normalize route to include date
				const today = getLocalDateSlug();
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
	// NARRATIVE IDENTITY: /narrative-identity
	// ========================================================================
	'narrative-identity': {
		match: (path) =>
			path === '/narrative-identity' ||
			/^\/narrative-identity\/(past|present|future)$/.test(path),
		parse: () => ({
			type: 'narrative-identity',
			label: 'Narrative Identity',
			icon: 'ri:quill-pen-line',
		}),
		serialize: () => 'narrative-identity',
		deserialize: () => '/narrative-identity',
		icon: 'ri:quill-pen-line',
		defaultLabel: 'Narrative Identity',
		component: NarrativeIdentityView,
	},

	// ========================================================================
	// SOURCE NAMESPACE: /sources, /sources/<credential_id>
	//   - `/sources` (and `/source` legacy alias) → list of credentials
	//   - `/sources/<id>` → CredentialDetailView for one credential
	// "Source" in the URL is user-facing vocabulary; under the hood each row
	// is a credential (one connection to a provider).
	// ========================================================================
	source: {
		match: (path) =>
			path === '/sources' || path === '/source' || /^\/sources\/[^/]+$/.test(path),
		parse: (path) => {
			if (path === '/sources' || path === '/source') {
				return {
					type: 'source',
					label: 'Sources',
					icon: 'ri:database-2-line',
					normalizedRoute: '/sources',
				};
			}
			const m = path.match(/^\/sources\/([^/]+)$/);
			return {
				type: 'source',
				label: 'Source',
				icon: 'ri:database-2-line',
				entityId: m?.[1],
			};
		},
		serialize: (id) => (id ? id : 'sources'),
		deserialize: (serialized) => (serialized === 'sources' ? '/sources' : `/sources/${serialized}`),
		icon: 'ri:database-2-line',
		defaultLabel: 'Sources',
		component: ConnectionsPanel,
		detailComponent: CredentialDetailView,
	},

	// ========================================================================
	// TOOLS: /tools
	// ========================================================================
	tools: {
		match: (path) => path === '/tools',
		parse: () => ({
			type: 'tools',
			label: 'Tools',
			icon: 'ri:tools-line',
		}),
		serialize: () => 'tools',
		deserialize: () => '/tools',
		icon: 'ri:tools-line',
		defaultLabel: 'Tools',
		component: ToolsView,
	},

	// ========================================================================
	// ACTIONS: /actions, /actions/{actions|templates|history}
	// ========================================================================
	actions: {
		match: (path) =>
			path === '/actions' || /^\/actions\/(actions|templates|history)$/.test(path),
		parse: () => ({
			type: 'actions',
			label: 'Actions',
			icon: 'ri:flashlight-line',
		}),
		serialize: () => 'actions',
		deserialize: () => '/actions',
		icon: 'ri:flashlight-line',
		defaultLabel: 'Actions',
		component: ActionsView,
	},

	// ========================================================================
	// ACTION DETAIL: /action/action_{id}
	// Singular namespace — no list view; actions list lives under `actions`.
	// ========================================================================
	action: {
		match: (path) => /^\/action\/action_[^/]+$/.test(path),
		parse: (path) => {
			const match = path.match(/^\/action\/(action_[^/]+)$/);
			return {
				type: 'action',
				label: 'Action',
				icon: 'ri:flashlight-line',
				entityId: match?.[1],
			};
		},
		serialize: (id) => (id ? `action_${id}` : 'action'),
		deserialize: (serialized) => {
			if (serialized.startsWith('action_')) return `/action/${serialized}`;
			return '/actions';
		},
		icon: 'ri:flashlight-line',
		defaultLabel: 'Action',
		component: ActionDetailView,
		detailComponent: ActionDetailView,
	},

	// ========================================================================
	// DEVELOPERS: /developers
	// Tab group containing SQL, Terminal, and Lake sub-views (selected via #hash).
	// ========================================================================
	developers: {
		match: (path) =>
			path === '/developers' || /^\/developers\/(sql|terminal|lake)$/.test(path),
		parse: () => ({
			type: 'developers',
			label: 'Developers',
			icon: 'ri:code-s-slash-line',
		}),
		serialize: () => 'developers',
		deserialize: () => '/developers',
		icon: 'ri:code-s-slash-line',
		defaultLabel: 'Developers',
		component: DevelopersView,
	},

	// ========================================================================
	// ONTOLOGY NAMESPACE: /ontologies, /ontologies/{name}
	// ========================================================================
	ontology: {
		match: (path) => path === '/ontologies' || /^\/ontologies\/[a-z_]+$/.test(path),
		parse: (path) => {
			if (path === '/ontologies') {
				return {
					type: 'ontology',
					label: 'Ontologies',
					icon: 'ri:table-line',
				};
			}
			const match = path.match(/^\/ontologies\/([a-z_]+)$/);
			const name = match?.[1] || '';
			const displayName = name
				.replace(/_/g, ' ')
				.replace(/\b\w/g, (c) => c.toUpperCase());
			return {
				type: 'ontology',
				label: displayName,
				icon: 'ri:table-line',
				entityId: name,
			};
		},
		serialize: (id) => (id ? `ontology_${id}` : 'ontologies'),
		deserialize: (serialized) => {
			if (serialized.startsWith('ontology_')) return `/ontologies/${serialized.slice(9)}`;
			return '/ontologies';
		},
		icon: 'ri:table-line',
		defaultLabel: 'Ontologies',
		component: OntologyIndexView,
		detailComponent: OntologyDetailView,
	},
	record: {
		// /record/<ontology>/<id> — a single raw life-graph record. The ontology
		// is a lowercase_underscore name; the id is everything after it.
		match: (path) => /^\/record\/[a-z0-9_]+\/.+$/.test(path),
		parse: (path) => {
			const m = path.match(/^\/record\/([a-z0-9_]+)\/(.+)$/);
			const ontology = m?.[1] ?? '';
			const recordId = m?.[2] ?? '';
			const label = ontology.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
			return {
				type: 'record',
				label: label || 'Record',
				icon: 'ri:database-2-line',
				// entityId carries both segments so this reads as a detail view;
				// DataView re-parses the full route anyway.
				entityId: recordId ? `${ontology}/${recordId}` : undefined,
			};
		},
		serialize: (id) => (id ? `record_${id}` : 'record'),
		deserialize: (serialized) =>
			serialized.startsWith('record_') ? `/record/${serialized.slice(7)}` : '/record',
		icon: 'ri:database-2-line',
		defaultLabel: 'Record',
		component: DataView,
		detailComponent: DataView,
	},

	// ========================================================================
	// ASSET: /drive/file_{id} — single-file viewer (open density for a file ref)
	// Matched before `drive` so an id-addressed file opens the viewer, while
	// path-addressed routes (/drive/Documents/…) still open the browser.
	// ========================================================================
	asset: {
		match: (path) => /^\/drive\/file_[^/]+$/.test(path),
		parse: (path) => {
			const match = path.match(/^\/drive\/(file_[^/]+)$/);
			return {
				type: 'asset',
				label: 'File',
				icon: 'ri:file-line',
				entityId: match?.[1],
			};
		},
		serialize: (id) => (id ? id : 'asset'),
		deserialize: (serialized) =>
			serialized.startsWith('file_') ? `/drive/${serialized}` : '/drive',
		icon: 'ri:file-line',
		defaultLabel: 'File',
		component: AssetView,
		detailComponent: AssetView,
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
	// System pages: account, assistant, usage, jobs, sql, terminal, sitemap
	// ========================================================================
	virtues: {
		match: (path) => path.startsWith('/virtues/'),
		parse: (path) => {
			const page = path.replace('/virtues/', '');

			const pageConfig: Record<string, { label: string; icon: string }> = {
				account: { label: 'Account', icon: 'ri:user-settings-line' },
				devices: { label: 'Devices', icon: 'ri:device-line' },
				activity: { label: 'Activity', icon: 'ri:history-line' },
				assistant: { label: 'Assistant', icon: 'ri:robot-line' },
				billing: { label: 'Billing', icon: 'ri:bank-card-line' },
				'byo-key': { label: 'AI Provider Key', icon: 'ri:key-line' },
				changelog: { label: "What's New", icon: 'ri:megaphone-line' },
				usage: { label: 'Usage', icon: 'ri:bar-chart-line' },
					telemetry: { label: 'Telemetry', icon: 'ri:pulse-line' },
				lake: { label: 'Lake', icon: 'ri:database-2-line' },
				sql: { label: 'SQL', icon: 'ri:database-2-line' },
				terminal: { label: 'Terminal', icon: 'ri:terminal-box-line' },
				sitemap: { label: 'Sitemap', icon: 'ri:road-map-line' },
				system: { label: 'System', icon: 'ri:computer-line' },
				'this-mac': { label: 'This Mac', icon: 'ri:macbook-line' },
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
		devices: DevicesView,
		activity: ActivityView,
		assistant: AssistantView,
		'byo-key': ByoKeyView,
		billing: BillingView,
		changelog: ChangelogView,
		usage: UsageTab,
		telemetry: TelemetryTab,
		lake: DeveloperLakeView,
		sql: DeveloperSqlView,
		terminal: DeveloperTerminalView,
		sitemap: DeveloperSitemapView,
		system: SystemInfoView,
		'this-mac': ThisMacView,
	};
	return componentMap[page] || ProfileView;
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
		// Landing surface (exact /home; no overlap with '/')
		'home',
		// Specific patterns first
		'source', // Source list and detail views
		'tools', // Tools management page
		'actions', // Actions list page (must come before singular 'action')
		'action', // Action detail page
		'developers', // Developers tab group (SQL/Terminal/Lake)
		'ontology', // Ontology data browsing
		'record', // /record/<ontology>/<id> — single raw record
		'virtues', // Has /virtues/* pattern
		'asset', // /drive/file_{id} — must precede 'drive' (which matches all /drive/*)
		'drive', // Has /drive/* pattern
		'trash', // Drive trash
		'chat-history', // Chat history list (before 'chat')
		// Entity namespaces
		'chat', // Also matches /
		'page',
		'wiki', // Wiki overview page
		'entities', // Unified entity list
		'person',
		'place',
		'org',
		'thing',
		'notebook',
		'day',
		'year',
		'narrative-identity',
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

