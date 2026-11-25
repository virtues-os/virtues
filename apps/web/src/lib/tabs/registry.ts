/**
 * Tab Registry - Namespace-based tab definitions with URL routing.
 *
 * URL Patterns:
 * - Entity namespaces: /{namespace} (list) or /{namespace}/{namespace}_{id} (detail)
 * - Storage: /drive, /drive/{path}
 * - System: /virtues/{page}
 */

import { eager, type ViewLoader } from './lazy';
import type { TabType, ParsedRoute } from './types';

/** A Browser tab is named for the site it shows. */
export function browserLabel(url: string | null): string {
	if (!url) return 'Browser';
	try {
		return new URL(url).hostname.replace(/^www\./, '') || 'Browser';
	} catch {
		return 'Browser';
	}
}
import { getLocalDateSlug } from '$lib/utils/dateUtils';
import { THREAD_ICON } from '$lib/utils/iconHelpers';
import { SERVER_TASKS_ROUTE, SITES_ROUTE, homeRoute, readAddress, type AddressWord } from '$lib/applets/routes';

// Views are loaders (see ./lazy.ts): each chunk arrives the first time a
// tab of that kind opens. Chat and home stay eager — a session opens on them.
import ChatViewEager from '$lib/components/tabs/views/ChatView.svelte';
const ChatView: ViewLoader = eager(ChatViewEager);
const BrowserView: ViewLoader = () => import('$lib/components/tabs/views/BrowserView.svelte');
const HistoryView: ViewLoader = () => import('$lib/components/tabs/views/HistoryView.svelte');
const WikiView: ViewLoader = () => import('$lib/components/tabs/views/WikiView.svelte');
const WikiDetailView: ViewLoader = () => import('$lib/components/tabs/views/WikiDetailView.svelte');
const SourcesView: ViewLoader = () => import('$lib/components/sources/SourcesView.svelte');
/** A tab's label before its page has loaded and given it a name. */
const ADDRESS_LABEL: Record<AddressWord, string> = { site: 'Site', skill: 'Skill', 'server-task': 'Server task' };

const AppletsView: ViewLoader = () => import('$lib/components/tabs/views/AppletsView.svelte');
const AppletDetailView: ViewLoader = () => import('$lib/components/tabs/views/AppletDetailView.svelte');
const SettingsView: ViewLoader = () => import('$lib/components/tabs/views/SettingsView.svelte');
const StorageView: ViewLoader = () => import('$lib/components/tabs/views/StorageView.svelte');
const DesignView: ViewLoader = () => import('$lib/components/tabs/views/DesignView.svelte');
const InboxView: ViewLoader = () => import('$lib/components/tabs/views/InboxView.svelte');
const AssetView: ViewLoader = () => import('$lib/components/tabs/views/AssetView.svelte');
const PagesView: ViewLoader = () => import('$lib/components/tabs/views/PagesView.svelte');
const PageDetailView: ViewLoader = () => import('$lib/components/tabs/views/PageDetailView.svelte');
const BookmarksView: ViewLoader = () => import('$lib/components/tabs/views/BookmarksView.svelte');
const PasswordsView: ViewLoader = () => import('$lib/components/tabs/views/PasswordsView.svelte');
const PhotosView: ViewLoader = () => import('$lib/components/tabs/views/PhotosView.svelte');
const PhotoDetailView: ViewLoader = () => import('$lib/components/tabs/views/PhotoDetailView.svelte');
const BookmarkDetailView: ViewLoader = () => import('$lib/components/tabs/views/BookmarkDetailView.svelte');
const ThreadsView: ViewLoader = () => import('$lib/components/tabs/views/ThreadsView.svelte');
const ThreadView: ViewLoader = () => import('$lib/components/tabs/views/ThreadView.svelte');
const TaskView: ViewLoader = () => import('$lib/components/tabs/views/TaskView.svelte');
const InsightView: ViewLoader = () => import('$lib/components/tabs/views/InsightView.svelte');
const NarrativeIdentityView: ViewLoader = () => import('$lib/components/tabs/views/NarrativeIdentityView.svelte');
const DataView: ViewLoader = () => import('$lib/components/tabs/views/DataView.svelte');
const QueryView: ViewLoader = () => import('$lib/components/tabs/views/QueryView.svelte');

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
	component: ViewLoader;

	// Optional: detail component for entity namespaces
	// biome-ignore lint/suspicious/noExplicitAny: Component props vary by tab type
	detailComponent?: ViewLoader;
}

// Complete tab registry with namespace-based URL patterns
/**
 * Every path the wiki room answers to — the ONE list.
 *
 * This regex used to be written out twice, here and in WikiView's own section
 * parser. Adding Lifeline and History to one copy and not the other made both
 * rooms unreachable: the section rendered fine, the sidebar linked to it, the
 * typechecker was happy, and no tab would open because the router did not
 * recognise the path. Two lists that must agree is a bug waiting for the next
 * section.
 */
export const WIKI_SECTION_RE =
	/^\/wiki\/(days|years|entities|identity|chapters|things|stories|lifeline|history|people|places|orgs|unlinked)$/;

/**
 * Words that were sections of the Sources room and now only forward
 * (SourcesView's FORWARDS): Catalog and Activity to `/sources`, Streams and
 * Query to Settings › Your data. They stay reserved so `/sources/<x>` cannot
 * read an old link as a source id. `/sources` itself is not in this list.
 */
export const SOURCES_SECTIONS = ['catalog', 'activity', 'streams', 'query'] as const;
export type SourcesSection = (typeof SOURCES_SECTIONS)[number];

export const tabRegistry: Record<TabType, TabDefinition> = {
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
					label: temporary ? 'Temporary chat' : 'New chat',
					icon: temporary ? 'mynaui:incognito' : 'atlas:chats',
					normalizedRoute: temporary ? '/chat?temporary=1' : '/chat',
				};
			}
			// Detail view
			const match = path.match(/^\/chat\/(chat_[^/]+)$/);
			return {
				type: 'chat',
				label: 'Chat',
				icon: 'atlas:chats',
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
		icon: 'atlas:chats',
		defaultLabel: 'Chats',
		component: ChatView,
		detailComponent: ChatView,
	},

	// ========================================================================
	// CHAT HISTORY: /chat-history
	// ========================================================================
	// The in-app browser (Mac). One native view; the tab is where it shows.
	browser: {
		match: (path) => path === '/browser',
		parse: (_path, params) => ({
			type: 'browser',
			label: browserLabel(params.get('url')),
			icon: 'mynaui:globe',
		}),
		serialize: () => 'browser',
		deserialize: () => '/browser',
		icon: 'mynaui:globe',
		defaultLabel: 'Browser',
		component: BrowserView,
	},

	'chat-history': {
		match: (path) => path === '/chat-history',
		parse: () => ({
			type: 'chat-history',
			label: 'All Chats',
			icon: 'mynaui:chat-messages',
		}),
		serialize: () => 'chat-history',
		deserialize: () => '/chat-history',
		icon: 'mynaui:chat-messages',
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
					icon: 'atlas:pages',
				};
			}
			// Detail view
			const match = path.match(/^\/page\/(page_[^/]+)$/);
			return {
				type: 'page',
				label: 'Page',
				icon: 'atlas:pages',
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
		icon: 'atlas:pages',
		defaultLabel: 'Pages',
		component: PagesView,
		detailComponent: PageDetailView,
	},

	// ========================================================================
	// WIKI: /wiki, /wiki/{days|entities|identity}
	// Legacy paths still match so old pins/deep-links land in the wiki:
	// /entities, and /wiki/{people|places|orgs|unlinked} (folded into the
	// unified entities section).
	// ========================================================================
	wiki: {
		match: (path) =>
			path === '/wiki' ||
			WIKI_SECTION_RE.test(path) ||
			path === '/entities',
		parse: () => ({
			type: 'wiki',
			label: 'Wiki',
			icon: 'mynaui:book',
		}),
		serialize: () => 'wiki',
		deserialize: () => '/wiki',
		icon: 'mynaui:book',
		defaultLabel: 'Wiki',
		component: WikiView,
	},

	// ========================================================================
	// PERSON NAMESPACE: /person/{id}
	//
	// The list is the wiki's index, `/wiki/people`, so the bare path opens
	// it with this kind filtered.
	// ========================================================================
	person: {
		match: (path) => path === '/person' || /^\/person\/[^/]+$/.test(path),
		parse: (path) => {
			if (path === '/person') {
				return {
					type: 'wiki',
					label: 'Wiki',
					icon: 'mynaui:book',
					normalizedRoute: '/wiki/people',
				};
			}
			const match = path.match(/^\/person\/([^/]+)$/);
			return {
				type: 'person',
				label: 'Person',
				icon: 'mynaui:user',
				entityId: match?.[1],
			};
		},
		serialize: (id) => id || 'person',
		deserialize: (serialized) => {
			if (serialized && serialized !== 'person') {
				return `/person/${serialized}`;
			}
			return '/wiki/people';
		},
		icon: 'mynaui:user',
		defaultLabel: 'People',
		component: WikiView,
		detailComponent: WikiDetailView,
	},

	// ========================================================================
	// PLACE NAMESPACE: /place/{id}
	//
	// The list is the wiki's index, `/wiki/places`, so the bare path opens
	// it with this kind filtered.
	// ========================================================================
	place: {
		match: (path) => path === '/place' || /^\/place\/[^/]+$/.test(path),
		parse: (path) => {
			if (path === '/place') {
				return {
					type: 'wiki',
					label: 'Wiki',
					icon: 'mynaui:book',
					normalizedRoute: '/wiki/places',
				};
			}
			const match = path.match(/^\/place\/([^/]+)$/);
			return {
				type: 'place',
				label: 'Place',
				icon: 'mynaui:map-pin',
				entityId: match?.[1],
			};
		},
		serialize: (id) => id || 'place',
		deserialize: (serialized) => {
			if (serialized && serialized !== 'place') {
				return `/place/${serialized}`;
			}
			return '/wiki/places';
		},
		icon: 'mynaui:map-pin',
		defaultLabel: 'Places',
		component: WikiView,
		detailComponent: WikiDetailView,
	},

	// ========================================================================
	// ORG NAMESPACE: /org/{id}
	//
	// The list is the wiki's index, `/wiki/orgs`, so the bare path opens
	// it with this kind filtered.
	// ========================================================================
	org: {
		match: (path) => path === '/org' || /^\/org\/[^/]+$/.test(path),
		parse: (path) => {
			if (path === '/org') {
				return {
					type: 'wiki',
					label: 'Wiki',
					icon: 'mynaui:book',
					normalizedRoute: '/wiki/orgs',
				};
			}
			const match = path.match(/^\/org\/([^/]+)$/);
			return {
				type: 'org',
				label: 'Organization',
				icon: 'mynaui:building',
				entityId: match?.[1],
			};
		},
		serialize: (id) => id || 'org',
		deserialize: (serialized) => {
			if (serialized && serialized !== 'org') {
				return `/org/${serialized}`;
			}
			return '/wiki/orgs';
		},
		icon: 'mynaui:building',
		defaultLabel: 'Organizations',
		component: WikiView,
		detailComponent: WikiDetailView,
	},

	// ========================================================================
	// THING NAMESPACE: /thing/{id}
	//
	// The list is the wiki's Things room, `/wiki/things`, so the bare path
	// opens that room.
	// ========================================================================
	thing: {
		match: (path) => path === '/thing' || /^\/thing\/[^/]+$/.test(path),
		parse: (path) => {
			if (path === '/thing') {
				return {
					type: 'wiki',
					label: 'Wiki',
					icon: 'mynaui:book',
					normalizedRoute: '/wiki/things',
				};
			}
			const match = path.match(/^\/thing\/([^/]+)$/);
			return {
				type: 'thing',
				label: 'Thing',
				icon: 'atlas:things',
				entityId: match?.[1],
			};
		},
		serialize: (id) => id || 'thing',
		deserialize: (serialized) => {
			if (serialized && serialized !== 'thing') {
				return `/thing/${serialized}`;
			}
			return '/wiki/things';
		},
		icon: 'atlas:things',
		defaultLabel: 'Things',
		component: WikiView,
		detailComponent: WikiDetailView,
	},

	// ========================================================================
	// BOOKMARK NAMESPACE: /bookmarks
	//
	// Saved web content — browser bookmarks, GitHub stars, hand-saved links.
	//
	// /bookmark/{id} is the detail. It used to be "a bookmark's detail IS the
	// page it points at" — true when a row was just a URL, wrong once a row
	// carries a note, tags, an extraction record and a read-state. The generic
	// /record/… view still renders the raw row for anyone who wants it; this
	// one leads with the note, because that is the only text on the page a
	// person wrote.
	// ========================================================================
	bookmarks: {
		match: (path) => path === '/bookmarks',
		parse: () => ({
			type: 'bookmarks',
			label: 'Bookmarks',
			icon: 'mynaui:bookmark',
			normalizedRoute: '/bookmarks',
		}),
		serialize: () => 'bookmarks',
		deserialize: () => '/bookmarks',
		icon: 'mynaui:bookmark',
		defaultLabel: 'Bookmarks',
		component: BookmarksView,
	},

	// PASSWORDS: /passwords — the logins saved on the box, in Storage beside
	// Bookmarks. Its own namespace, not under /storage, because nothing here
	// is a file and Storage's search and indexing must never reach it.
	passwords: {
		match: (path) => path === '/passwords',
		parse: () => ({
			type: 'passwords',
			label: 'Passwords',
			icon: 'mynaui:lock-password',
			normalizedRoute: '/passwords',
		}),
		serialize: () => 'passwords',
		deserialize: () => '/passwords',
		icon: 'mynaui:lock-password',
		defaultLabel: 'Passwords',
		component: PasswordsView,
	},

	// BOOKMARK DETAIL: /bookmark/{id} — singular, matching /thread/{id}.
	bookmark: {
		match: (path) => /^\/bookmark\/.+$/.test(path),
		parse: (path) => ({
			type: 'bookmark',
			label: 'Bookmark',
			icon: 'mynaui:bookmark',
			entityId: path.match(/^\/bookmark\/(.+)$/)?.[1],
		}),
		serialize: (id) => (id ? `bookmark_${id}` : 'bookmark'),
		deserialize: (serialized) =>
			serialized.startsWith('bookmark_') ? `/bookmark/${serialized.slice(9)}` : '/bookmarks',
		icon: 'mynaui:bookmark',
		defaultLabel: 'Bookmark',
		component: BookmarkDetailView,
		detailComponent: BookmarkDetailView,
	},

	// PHOTO FAVORITES: /photos, and one photo at /photo/{id}.
	photos: {
		match: (path) => path === '/photos',
		parse: () => ({
			type: 'photos',
			label: 'Photo favorites',
			icon: 'mynaui:image',
			normalizedRoute: '/photos',
		}),
		serialize: () => 'photos',
		deserialize: () => '/photos',
		icon: 'mynaui:image',
		defaultLabel: 'Photo favorites',
		component: PhotosView,
	},
	photo: {
		match: (path) => /^\/photo\/.+$/.test(path),
		parse: (path) => ({
			type: 'photo',
			label: 'Photo',
			icon: 'mynaui:image',
			entityId: path.match(/^\/photo\/(.+)$/)?.[1],
		}),
		serialize: (id) => (id ? `photo_${id}` : 'photo'),
		deserialize: (serialized) =>
			serialized.startsWith('photo_') ? `/photo/${serialized.slice(6)}` : '/photos',
		icon: 'mynaui:image',
		defaultLabel: 'Photo',
		component: PhotoDetailView,
		detailComponent: PhotoDetailView,
	},

	// ========================================================================
	// THREAD NAMESPACE: /threads (the door), /thread/{id} (a thread's page)
	//
	// A thread is something happening that you follow. It holds tasks, filed
	// chats, pages, files and records.
	// TODO(2026-11-17): migrate projectStore, projectActions, ProjectGlyph and
	// the Project* types in api/client.ts to thread names once the files that
	// import them hold no other session's uncommitted work.
	//
	// `/threads` is the list of every thread. Tasks and insights live in
	// the Inbox, which also answers the old `/tasks` addresses.
	// ========================================================================
	thread: {
		match: (path) =>
			path === '/threads' ||
			path === '/thread' ||
			/^\/thread\/[^/]+$/.test(path),
		parse: (path) => {
			if (!/^\/thread\/[^/]+$/.test(path)) {
				return { type: 'thread', label: 'Threads', icon: THREAD_ICON, normalizedRoute: '/threads' };
			}
			const match = path.match(/^\/thread\/([^/]+)$/);
			return {
				type: 'thread',
				label: 'Thread',
				icon: THREAD_ICON,
				entityId: match?.[1],
			};
		},
		// Prefixed like `bookmark_`, not bare like `person_…`: a bare `nb_…` id
		// has no tab type in front of it, so `deserializeTab` read it as type
		// "nb" and every thread tab restored as Home.
		serialize: (id) => (id ? `thread_${id}` : 'threads'),
		deserialize: (serialized) =>
			serialized.startsWith('thread_') ? `/thread/${serialized.slice(7)}` : '/threads',
		icon: THREAD_ICON,
		defaultLabel: 'Threads',
		component: ThreadsView,
		detailComponent: ThreadView,
	},

	// ========================================================================
	// INSIGHT: /insight/{key}, one note's page. It sits under the Threads
	// door. A note's key holds colons, so the route carries it encoded.
	// ========================================================================
	insight: {
		match: (path) => /^\/insight\/[^/]+$/.test(path),
		parse: (path) => ({
			type: 'insight',
			label: 'Insight',
			icon: 'mynaui:sparkles',
			entityId: path.match(/^\/insight\/([^/]+)$/)?.[1],
		}),
		serialize: (id) => (id ? `insight_${id}` : 'insight'),
		deserialize: (serialized) =>
			serialized.startsWith('insight_') ? `/insight/${serialized.slice(8)}` : '/inbox',
		icon: 'mynaui:sparkles',
		defaultLabel: 'Insight',
		component: InsightView,
		detailComponent: InsightView,
	},

	// ========================================================================
	// TASK: /task/{id}, one task's page. It sits under the Threads door.
	// ========================================================================
	task: {
		match: (path) => /^\/task\/[^/]+$/.test(path),
		parse: (path) => ({
			type: 'task',
			label: 'Task',
			icon: 'atlas:tasks',
			entityId: path.match(/^\/task\/([^/]+)$/)?.[1],
		}),
		// Task ids carry their own `task_` prefix, so the serialized form is
		// `task_task_…`, and the type prefix comes off whole.
		serialize: (id) => (id ? `task_${id}` : 'task'),
		deserialize: (serialized) =>
			serialized.startsWith('task_') && serialized.length > 5 ? `/task/${serialized.slice(5)}` : '/inbox',
		icon: 'atlas:tasks',
		defaultLabel: 'Task',
		component: TaskView,
		detailComponent: TaskView,
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
					icon: 'mynaui:calendar',
					entityId: `day_${today}`,
					normalizedRoute: `/day/day_${today}`,
				};
			}
			const match = path.match(/^\/day\/(day_\d{4}-\d{2}-\d{2})$/);
			const dateStr = match?.[1]?.replace('day_', '') || '';
			return {
				type: 'day',
				label: dateStr,
				icon: 'mynaui:calendar',
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
		icon: 'mynaui:calendar',
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
					icon: 'mynaui:calendar',
					entityId: `year_${currentYear}`,
					normalizedRoute: `/year/year_${currentYear}`,
				};
			}
			const match = path.match(/^\/year\/(year_\d{4})$/);
			const yearStr = match?.[1]?.replace('year_', '') || '';
			return {
				type: 'year',
				label: yearStr,
				icon: 'mynaui:calendar',
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
		icon: 'mynaui:calendar',
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
			icon: 'mynaui:pen',
		}),
		serialize: () => 'narrative-identity',
		deserialize: () => '/narrative-identity',
		icon: 'mynaui:pen',
		defaultLabel: 'Narrative Identity',
		component: NarrativeIdentityView,
	},

	// ========================================================================
	// SOURCE NAMESPACE: /sources, /sources/<source_id>, /sources/<credential_id>
	//   - `/sources` (and `/source` legacy alias) → the one Sources page
	//   - `/sources/<word in SOURCES_SECTIONS>` → forwards (see SourcesView)
	//   - `/sources/<id>` → one source, or one connection for `cred_…`
	// "Source" in the URL is user-facing vocabulary; under the hood a connection
	// is a credential (OAuth/API-key) or a paired device (iOS/Mac).
	//
	// Sections are matched before ids — see SOURCES_SECTIONS for why the two
	// must not be decided in two places.
	// ========================================================================
	source: {
		match: (path) =>
			path === '/sources' || path === '/source' || /^\/sources\/[^/]+$/.test(path),
		parse: (path, params) => {
			if (path === '/sources' || path === '/source') {
				// The OAuth callback lands here as `?connected=<id>` or
				// `?source=<id>&error=<reason>`. Keep those on the tab route so
				// the Sources page can say how the connect went; it strips them once
				// read, so a refresh does not repeat the notice.
				const kept = new URLSearchParams();
				for (const k of ['connected', 'source', 'error']) {
					const v = params?.get(k);
					if (v) kept.set(k, v);
				}
				const query = kept.size > 0 ? `?${kept}` : '';
				return {
					type: 'source',
					label: 'Sources',
					icon: 'mynaui:database',
					normalizedRoute: `/sources${query}`,
				};
			}
			const m = path.match(/^\/sources\/([^/]+)$/);
			const seg = m?.[1] ?? '';
			if ((SOURCES_SECTIONS as readonly string[]).includes(seg)) {
				return {
					type: 'source',
					label: `Sources · ${seg[0].toUpperCase()}${seg.slice(1)}`,
					icon: 'mynaui:database',
				};
			}
			return {
				type: 'source',
				label: 'Source',
				icon: 'mynaui:database',
				entityId: seg,
			};
		},
		serialize: (id) => (id ? id : 'sources'),
		deserialize: (serialized) => (serialized === 'sources' ? '/sources' : `/sources/${serialized}`),
		icon: 'mynaui:database',
		defaultLabel: 'Sources',
		// No `detailComponent`. TabContent decides list-vs-detail with a generic
		// `/^\/[a-z]+\/([^/]+)$/` on the route, which cannot tell `/sources/catalog`
		// (a section) from `/sources/cred_abc` (a credential) — it called both
		// details and rendered "Credential not found" on the catalog. SourcesView
		// owns the whole namespace and dispatches the detail itself, against the
		// same SOURCES_SECTIONS list the matcher above uses.
		component: SourcesView,
	},

	// ========================================================================
	// SITES: /sites, the door for what you set up. The addresses it had
	// before (/applets, /actions/*) still resolve.
	// ========================================================================
	applets: {
		match: (path) =>
			path === SITES_ROUTE ||
			path === '/applets' ||
			path === '/actions' || /^\/actions\/(actions|templates|history)$/.test(path),
		parse: () => ({
			type: 'applets',
			label: 'Sites',
			icon: 'atlas:applets',
			normalizedRoute: SITES_ROUTE,
		}),
		serialize: () => 'applets',
		deserialize: () => SITES_ROUTE,
		icon: 'atlas:applets',
		defaultLabel: 'Sites',
		component: AppletsView,
	},

	// ========================================================================
	// A tab saved while a site's view had a tab type of its own. No route
	// resolves to it now; the entry keeps such a tab opening, on the page
	// that draws the view.
	// ========================================================================
	'applet-view': {
		match: () => false,
		parse: () => ({ type: 'applet-view', label: 'Site', icon: 'atlas:applets' }),
		serialize: () => 'applet-view',
		deserialize: () => SITES_ROUTE,
		icon: 'atlas:applets',
		defaultLabel: 'Site',
		component: AppletDetailView,
	},

	// ========================================================================
	// A SITE, A SKILL OR A SERVER TASK: /site/{name}, /skill/{name} and
	// /server-task/{name}, each with /options for its other side
	// (`applets/routes.ts`). The addresses used before (/applet/applet_{id},
	// with /view or ?panel=, and /job/{name}) resolve to these.
	// ========================================================================
	applet: {
		match: (path) => readAddress(path) !== null,
		parse: (path, params) => {
			const query = params?.toString();
			const address = readAddress(query ? `${path}?${query}` : path);
			return {
				type: 'applet',
				label: address ? ADDRESS_LABEL[address.word] : 'Site',
				icon: 'atlas:applets',
				entityId: address?.id,
				normalizedRoute: address?.route,
			};
		},
		serialize: (id) => (id ? `applet_${id}` : 'applet'),
		deserialize: (serialized) => {
			const id = serialized.replace(/^applet_(?=applet_)/, '');
			return id.startsWith('applet_') ? homeRoute(id) : SITES_ROUTE;
		},
		icon: 'atlas:applets',
		defaultLabel: 'Site',
		component: AppletDetailView,
		detailComponent: AppletDetailView,
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
				icon: 'mynaui:database',
				// entityId carries both segments so this reads as a detail view;
				// DataView re-parses the full route anyway.
				entityId: recordId ? `${ontology}/${recordId}` : undefined,
			};
		},
		serialize: (id) => (id ? `record_${id}` : 'record'),
		deserialize: (serialized) =>
			serialized.startsWith('record_') ? `/record/${serialized.slice(7)}` : '/record',
		icon: 'mynaui:database',
		defaultLabel: 'Record',
		component: DataView,
		detailComponent: DataView,
	},

	query: {
		// /chat/<chat>/tool/<call> — the rows one query returned, which a chat
		// answer's figure links to. Matched before `chat`, whose own routes
		// stop at the chat id.
		match: (path) => /^\/chat\/[^/]+\/tool\/[^/#]+(#\d)?$/.test(path),
		parse: (path) => ({
			type: 'query',
			label: 'Query',
			icon: 'mynaui:table',
			entityId: path.slice('/chat/'.length),
		}),
		serialize: (id) => (id ? `query_${id}` : 'query'),
		deserialize: (serialized) =>
			serialized.startsWith('query_') ? `/chat/${serialized.slice(6)}` : '/chat',
		icon: 'mynaui:table',
		defaultLabel: 'Query',
		component: QueryView,
		detailComponent: QueryView,
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
				icon: 'mynaui:file',
				entityId: match?.[1],
			};
		},
		serialize: (id) => (id ? `asset_${id}` : 'asset'),
		deserialize: (serialized) => {
			// serialized arrives as `asset_file_{id}` (from KNOWN_TYPES dispatch) or a
			// bare `file_{id}` legacy token; strip the type prefix and rebuild the route.
			const fileId = serialized.startsWith('asset_') ? serialized.slice(6) : serialized;
			return fileId.startsWith('file_') ? `/drive/${fileId}` : '/drive';
		},
		icon: 'mynaui:file',
		defaultLabel: 'File',
		component: AssetView,
		detailComponent: AssetView,
	},

	// ========================================================================
	// STORAGE NAMESPACE: /storage, /storage/{drive,streams,media,trash},
	//                    /storage/drive/{path}
	//
	// One surface for the four kinds of bytes the box holds — files you filed,
	// the raw evidence your devices sent, the assets the app made, and what you
	// deleted. They used to be scattered (/drive, /trash, /developers/lake) and
	// the lake was a stub rendering zeros.
	//
	// The base is /storage rather than /drive on purpose: Drive's sub-paths are
	// USER FOLDER NAMES, so a tab at /drive/streams would be ambiguous with a
	// folder someone actually named "streams" — and that folder would silently
	// become unreachable. Under /storage, drive paths live at /storage/drive/…
	// and can't collide with a tab.
	//
	// /drive and /trash still resolve (below) so existing links and bookmarks
	// keep working.
	// ========================================================================
	storage: {
		match: (path) => path === '/storage' || path.startsWith('/storage/'),
		parse: (path) => {
			const sub = path.match(/^\/storage\/(streams|media|trash)$/)?.[1];
			if (sub === 'streams') {
				return { type: 'storage', label: 'Streams', icon: 'mynaui:database' };
			}
			if (sub === 'media') {
				return { type: 'storage', label: 'App Media', icon: 'mynaui:image' };
			}
			if (sub === 'trash') {
				return { type: 'storage', label: 'Recently deleted', icon: 'mynaui:trash' };
			}
			// The front page: what Storage holds, newest first.
			if (path === '/storage') {
				return { type: 'storage', label: 'Storage', icon: 'mynaui:archive' };
			}
			// Files, possibly deep inside a folder: /storage/drive/Documents/2026
			const storagePath = path.replace(/^\/storage\/drive\/?/, '');
			if (!storagePath) {
				return { type: 'storage', label: 'Files', icon: 'mynaui:hard-drive' };
			}
			return {
				type: 'storage',
				label: storagePath.split('/').pop() || 'File',
				icon: 'mynaui:file',
				storagePath,
			};
		},
		serialize: (id) => (id ? `storage_${encodeURIComponent(id)}` : 'storage'),
		deserialize: (serialized) => {
			if (serialized.startsWith('storage_')) {
				return `/storage/${decodeURIComponent(serialized.slice(8))}`;
			}
			return '/storage';
		},
		icon: 'mynaui:archive',
		defaultLabel: 'Storage',
		component: StorageView,
	},

	// ========================================================================
	// LEGACY: /drive, /drive/{path}, /trash — kept so old links resolve.
	// Both now open the Storage surface on the right tab.
	// ========================================================================
	drive: {
		match: (path) => path === '/drive' || path.startsWith('/drive/'),
		parse: (path) => {
			const storagePath = path.replace(/^\/drive\/?/, '');
			if (!storagePath) {
				return { type: 'drive', label: 'Files', icon: 'mynaui:hard-drive' };
			}
			return {
				type: 'drive',
				label: storagePath.split('/').pop() || 'File',
				icon: 'mynaui:file',
				storagePath,
			};
		},
		serialize: (id) => (id ? `drive_${encodeURIComponent(id)}` : 'drive'),
		deserialize: (serialized) => {
			if (serialized.startsWith('drive_')) {
				return `/storage/drive/${decodeURIComponent(serialized.slice(6))}`;
			}
			return '/storage/drive';
		},
		icon: 'mynaui:hard-drive',
		defaultLabel: 'Drive',
		component: StorageView,
	},

	trash: {
		match: (path) => path === '/trash',
		parse: () => ({
			type: 'trash',
			label: 'Recently deleted',
			icon: 'mynaui:trash',
		}),
		serialize: () => 'trash',
		deserialize: () => '/storage/trash',
		icon: 'mynaui:trash',
		defaultLabel: 'Recently deleted',
		component: StorageView,
	},

	// ========================================================================
	// INBOX: /inbox. Tasks, insights and what your server needs, with the
	// chosen one whole beside the list. The page opens on its list; any
	// other view is in the query (`?view=radar`, and the others InboxView
	// names). The sidebar's Inbox row opens it. The day on a map is its Map
	// view, so the address the Timeline page had (`/timeline`, and
	// `/timeline/YYYY-MM-DD` from an old link or pin) opens that view.
	// ========================================================================
	inbox: {
		match: (path) =>
			path === '/inbox' ||
			/^\/tasks(\/(assistant|all))?$/.test(path) ||
			path === '/timeline' || /^\/timeline\/\d{4}-\d{2}-\d{2}$/.test(path),
		parse: (path, params) => {
			const view = path.startsWith('/timeline') ? 'map' : params?.get('view');
			const query = view && ['radar', 'cards', 'graph', 'map', 'week'].includes(view) ? `?view=${view}` : '';
			return { type: 'inbox', label: 'Inbox', icon: 'mynaui:inbox', normalizedRoute: `/inbox${query}` };
		},
		serialize: () => 'inbox',
		deserialize: () => '/inbox',
		icon: 'mynaui:inbox',
		defaultLabel: 'Inbox',
		component: InboxView,
	},

	// ========================================================================
	// DESIGN: /design. The design system, agents/build/design.md rendered
	// with live specimens. No door opens it; it is reached by its address.
	// ========================================================================
	design: {
		match: (path) => path === '/design',
		parse: () => ({ type: 'design', label: 'Design system', icon: 'mynaui:swatches' }),
		serialize: () => 'design',
		deserialize: () => '/design',
		icon: 'mynaui:swatches',
		defaultLabel: 'Design system',
		component: DesignView,
	},

	// ========================================================================
	// SETTINGS NAMESPACE: /virtues[/{section}[/{sub}]]
	// One room (SettingsView) — You, Assistant, Connections, Billing, Box,
	// Developer — as a two-level route-driven sub-nav. Legacy flat pages
	// (/virtues/account, /virtues/system/*, /virtues/telemetry, ...) resolve
	// here and self-heal to their new home inside the room shell.
	// ========================================================================
	virtues: {
		match: (path) => path === '/virtues' || path.startsWith('/virtues/'),
		parse: (path) => {
			// The Server tasks page was at /virtues/applets, then /virtues/jobs.
			const moved = path === '/virtues/applets' || path === '/virtues/jobs';
			const page = path === '/virtues' ? 'you' : moved ? 'server-tasks' : path.replace('/virtues/', '');
			return {
				type: 'virtues',
				label: 'Settings',
				icon: 'mynaui:cog',
				virtuesPage: page,
				...(moved ? { normalizedRoute: SERVER_TASKS_ROUTE } : {}),
			};
		},
		serialize: (id) => (id ? `virtues_${id}` : 'virtues'),
		deserialize: (serialized) => {
			if (serialized.startsWith('virtues_')) {
				return `/virtues/${serialized.slice(8)}`;
			}
			return '/virtues/you';
		},
		icon: 'mynaui:cog',
		defaultLabel: 'Settings',
		component: SettingsView,
	},
};

/**
 * Get the appropriate component for a tab type and whether it's a detail view.
 */
export function getComponent(type: TabType, hasEntityId: boolean): ViewLoader {
	const def = tabRegistry[type];
	if (hasEntityId && def.detailComponent) {
		return def.detailComponent;
	}
	return def.component;
}

/**
 * Get the component for /virtues/* pages. There is now a single Settings room;
 * it dispatches to the right section from the route and self-heals legacy
 * flat paths on mount.
 */
export function getVirtuesComponent(_page: string): ViewLoader {
	return SettingsView;
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
		'browser', // /browser?url=…
		'source', // Source list and detail views
		'applets', // The Sites door
		'applet', // A site's, a skill's or a server task's page
		'record', // /record/<ontology>/<id> — single raw record
		'query', // /chat/<chat>/tool/<call> — must precede 'chat'
		'virtues', // Has /virtues/* pattern
		'storage', // /storage — Drive surface (unified bytes view)
		'asset', // /drive/file_{id} — must precede 'drive' (which matches all /drive/*)
		'drive', // Has /drive/* pattern
		'trash', // Drive trash
		'design', // The design system: /design
		'inbox', // /inbox, and the Timeline page's old /timeline
		'chat-history', // Chat history list (before 'chat')
		// Entity namespaces
		'chat', // Also matches /
		'page',
		'wiki', // Wiki room (overview + entity sections; also legacy /entities)
		'person',
		'place',
		'org',
		'thing',
		'thread',
		'task',
		'insight',
		// Detail before the room: /bookmark/{id} and /bookmarks are distinct
		// paths, but keeping the pair adjacent is how the next person notices
		// that BOTH have to be listed here. An entry in `tabRegistry` alone is
		// unreachable — this array is what routing actually walks.
		'bookmark',
		'bookmarks',
		'photo',
		'photos',
		'passwords',
		'day',
		'year',
		'narrative-identity',
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
		label: 'New chat',
		icon: 'atlas:chats',
	};
}

