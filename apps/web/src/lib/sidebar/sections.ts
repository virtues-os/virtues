/**
 * System Sidebar Sections
 *
 * Single source of truth for the sidebar's system navigation.
 * These are rendered directly from constants — no database backing.
 * Only user-created folders and items live in the DB.
 *
 * The sidebar is a "contents page", not a mode-switching rail: a stable
 * directory of top-level destinations. Deep navigation lives inside each
 * room (and is curated via Pins / Notebooks), never in this panel.
 *
 * Destinations split into three gap-separated clusters (no text headers):
 *   - Rhythm    — Home, Today.
 *   - Create    — Chats, Pages, Notebooks (where you work).
 *   - Substrate — Wiki (the life-graph: entities + time + narrative), Drive,
 *                 Actions (the layers underneath).
 */

export interface SystemSectionItem {
	id: string;
	label: string;
	icon: string;
	href: string;
}

export interface SystemSection {
	id: string;
	name: string;
	icon: string;
	type: 'smart' | 'static' | 'link';
	/** Link sections: direct route */
	href?: string;
	/** Smart sections: namespace for fetching recent items */
	namespace?: string;
	/** Smart sections: max items to show */
	limit?: number;
	/**
	 * The `+` on the row: what this collection creates.
	 *
	 * Declared here rather than per-component so that "a collection row offers
	 * to add to it" is a property of the section, not a favour done to some
	 * rows. Pages had one and Chats didn't, which read as a missing button on
	 * Chats — but the real bug was that nothing made it a rule.
	 */
	quickAdd?: 'chat' | 'page' | 'notebook';
	/**
	 * Open on first render.
	 *
	 * Only Notebooks sets this. The sidebar's problem was never that it lacked
	 * rows — it was that nothing in it belonged to the user, so eight fixed
	 * rooms sat above a column of dead space. A collection that fills that space
	 * only fills it if it's open, and asking someone to expand it every session
	 * to see their own work is asking them to do the layout's job.
	 */
	defaultExpanded?: boolean;
	/** Smart sections: "View All" route */
	moreRoute?: string;
	/** Static sections: fixed child items */
	items?: SystemSectionItem[];
}

/** A named cluster of destinations. `label: null` renders no header (the "now" zone). */
export interface SectionGroup {
	id: string;
	label: string | null;
	items: SystemSection[];
}

export const HOME_ROUTE = '/home';

// Home is a nav row again.
//
// It was removed when the ∴ mark took the job, on the argument that the mark
// needed one. That argument only holds if the mark belongs in the sidebar at
// all — and a wordmark that behaves like a button is not a pattern used
// anywhere else in software, so people didn't read it as the way home. The
// mark is gone and the destination is back where it can be read.
const HOME: SystemSection = {
	id: 'sys_home_row',
	name: 'Home',
	icon: 'ri:home-4-line',
	type: 'link',
	href: HOME_ROUTE,
};

// Today is gone, and not merely because it was usually empty.
//
// `/home` IS the live view of today — the "day before synthesis" essay, raw
// streams and a now-marker. `/day` is the *composed* view of a day, which only
// exists after the nightly run. So a "Today" row pointed at the one day that is
// by definition not yet written: structurally guaranteed to be the worst day
// page in the archive, sitting one row below the page that already showed you
// today properly.
//
// Past days are still worth reaching. They are reachable through Home's
// now-marker, the wiki, and search; if that proves too thin, the honest
// replacement is a "Days" index, not Today.

// Narrative folded into Wiki (the life-graph's throughline). Still reachable at
// /narrative-identity via Wiki + search; no longer a top-level sidebar entry.

// The three collections are `smart`: the row navigates to the index, the
// chevron opens the list in place, the + creates one. Click and expand are
// SEPARATE hit targets — collapsing them into one control is where this
// pattern usually goes wrong.
//
// This is what fills the sidebar. It used to be eight fixed rooms and a void:
// nothing in the panel was the user's, which is most of why it read as any
// product's menu rather than as theirs.
//
// Not a re-run of Recents. Recents was bad because it listed destinations
// ALREADY in the nav — clicking "Pages" logged a visit to /pages, so the
// sidebar was the largest contributor to its own history. Your actual chats
// and notebooks cannot be reached any other way. Same shape, opposite value.
const CHATS: SystemSection = {
	id: 'sys_chat',
	name: 'Chats',
	icon: 'ri:chat-1-line',
	type: 'smart',
	href: '/chat-history',
	namespace: 'chat',
	// Unbounded and volatile, so capped. Notebooks are not.
	limit: 8,
	moreRoute: '/chat-history',
	quickAdd: 'chat',
};

const PAGES: SystemSection = {
	id: 'sys_pages',
	name: 'Pages',
	icon: 'ri:file-text-line',
	type: 'smart',
	href: '/page',
	namespace: 'page',
	limit: 8,
	moreRoute: '/page',
	quickAdd: 'page',
};

const WIKI: SystemSection = {
	id: 'sys_wiki',
	name: 'Wiki',
	icon: 'ri:book-open-line',
	type: 'link',
	href: '/wiki',
};

// Uncapped, deliberately. Notebooks are curated containers — there are rarely
// more than a dozen and every one of them is something the owner made. This is
// the list that earns the column's height.
//
// One level only: notebooks expand to notebooks, never to their contents. A
// two-level tree in a 208px column stops being readable, and a notebook's own
// page is where its contents belong.
const NOTEBOOKS: SystemSection = {
	id: 'sys_notebooks',
	name: 'Notebooks',
	icon: 'ri:booklet-line',
	type: 'smart',
	href: '/notebooks',
	namespace: 'notebook',
	defaultExpanded: true,
	moreRoute: '/notebooks',
	quickAdd: 'notebook',
};

const DRIVE: SystemSection = {
	id: 'sys_drive',
	name: 'Drive',
	icon: 'ri:cloud-line',
	type: 'link',
	href: '/storage',
};

const ACTIONS: SystemSection = {
	id: 'sys_actions',
	name: 'Applets',
	icon: 'ri:flashlight-line',
	type: 'link',
	href: '/applets',
};

export const SECTION_GROUPS: SectionGroup[] = [
	// Rhythm — Create — Substrate, each separated by a blank gap (no headers).
	{ id: 'grp_rhythm', label: null, items: [HOME] },
	// Notebooks first: curated containers above loose material, which is the
	// order a contents page would use.
	{ id: 'grp_create', label: null, items: [NOTEBOOKS, CHATS, PAGES] },
	{ id: 'grp_substrate', label: null, items: [WIKI, DRIVE, ACTIONS] },
];

/** Map old DB view IDs → new constant IDs (for localStorage migration) */
export const LEGACY_ID_MAP: Record<string, string> = {
	'view_sys_sec_chats': 'sys_chat',
	'view_sys_sec_pages': 'sys_pages',
	'view_sys_sec_wiki': 'sys_wiki',
	'view_sys_sec_files': 'sys_drive',
	'view_sys_sec_data': '',
	'view_sys_sec_developer': '',
	'sys_chats': 'sys_chat',
	'sys_files': 'sys_drive',
	'sys_projects': 'sys_notebooks',
	'sys_things': 'sys_notebooks',
	'sys_spaces': 'sys_notebooks',
	'sys_you': '',
};
