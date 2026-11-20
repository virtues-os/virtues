/**
 * What search shows for what you typed. The view (`SearchModal`) draws the
 * groups this returns and runs the row you pick. Everything about which
 * rows appear, and in what order, is decided here.
 *
 * Empty, it shows where you are and what you were last in: the project the
 * open chat or page is filed in, then Recent. Commands are left out until
 * you type something that names one, so the first thing under the field is
 * your own work.
 *
 * Typed, the order is:
 *   - an address gets Open, first;
 *   - a day ("yesterday", "tuesday", "oct 5") gets that day, next;
 *   - a question gets Ask, next;
 *   - a query that starts a command's name ("new", "go") gets Commands;
 *   - chats, pages, threads and applets whose names match;
 *   - people and places whose names match;
 *   - settings whose names match (lib/settings), a few;
 *   - Commands and Ask, if they did not come first;
 *   - matches inside your records, last, because they arrive from the box
 *     after the rest and rows appearing above the selection would move it.
 *
 * A leading `#chats`, `#pages`, `#threads`, `#applets`, `#settings` or
 * `#commands` shows that kind only. `#projects` still works.
 */
import type { LocalSearchHit } from '$lib/api/client';
import { matchRank } from '$lib/search/match';
import type { ContextMenuItem } from '$lib/stores/contextMenu.svelte';
import { chatSessions } from '$lib/stores/chatSessions.svelte';
import { pagesStore } from '$lib/stores/pages.svelte';
import { projectStore } from '$lib/stores/project.svelte';
import { visits } from '$lib/stores/visits.svelte';
import { recentRows, type RecentKind, type RecentRow } from '$lib/sidebar/recents.svelte';
import { parseRef } from '$lib/utils/refRoutes';
import { isUntitled } from '$lib/refs/identity.svelte';
import { namedDays, type NamedDay } from '$lib/search/dates';
import { matchEntities, type EntityRow } from '$lib/search/entities.svelte';
import { SETTINGS, titleOf, type Setting } from '$lib/settings';

export interface Command {
	id: string;
	label: string;
	icon: string;
	shortcut?: string;
	run: () => void | Promise<void>;
	/** The command changes what search shows (the theme list) instead of closing it. */
	keepOpen?: boolean;
}

export type SearchRow =
	| { kind: 'open'; url: string }
	| { kind: 'ask'; query: string }
	| { kind: 'command'; command: Command }
	| { kind: 'record'; record: RecentRow }
	| { kind: 'day'; day: NamedDay }
	| { kind: 'entity'; entity: EntityRow }
	| { kind: 'setting'; setting: Setting }
	| { kind: 'content'; hit: LocalSearchHit };

export interface SearchGroup {
	/** Shown above the rows; null for a group that needs no name. */
	label: string | null;
	rows: SearchRow[];
}

export type Scope = RecentKind | 'command' | 'setting';

const SCOPES: Record<string, Scope> = {
	chat: 'chat', chats: 'chat',
	page: 'page', pages: 'page',
	project: 'thread', projects: 'thread',
	thread: 'thread', threads: 'thread',
	applet: 'applet', applets: 'applet',
	command: 'command', commands: 'command',
	setting: 'setting', settings: 'setting',
	action: 'command', actions: 'command',
};

/** Empty, the whole sheet stays short enough to read at a glance. */
const EMPTY_ROWS = 7;
const HERE_ROWS = 3;
const MATCH_ROWS = 6;
const SCOPED_ROWS = 25;
const COMMAND_ROWS = 4;
const ENTITY_ROWS = 3;
const SETTING_ROWS = 3;

/** The `#scope` a query starts with, and the text left to match. An unknown
 *  `#foo` is searched as written. */
export function parseQuery(raw: string): { scope: Scope | null; text: string } {
	const trimmed = raw.trim();
	const token = trimmed.match(/^#(\w+)/)?.[1]?.toLowerCase();
	const scope = token ? (SCOPES[token] ?? null) : null;
	return { scope, text: scope ? trimmed.replace(/^#\w+\s*/, '') : trimmed };
}

/** "virtues.com" or "https://…", as the URL to open. */
export function addressOf(text: string): string | null {
	if (!/^(https?:\/\/)?([a-z0-9-]+\.)+[a-z]{2,}(:\d+)?(\/\S*)?$/i.test(text)) return null;
	return /^https?:\/\//i.test(text) ? text : `https://${text}`;
}

const QUESTION_WORDS = new Set([
	'what', 'when', 'where', 'who', 'whom', 'whose', 'why', 'which', 'how',
	'did', 'do', 'does', 'is', 'are', 'was', 'were', 'can', 'could', 'should', 'would', 'will',
	'summarize', 'summarise', 'tell', 'explain', 'compare', 'find', 'show', 'list',
]);

/** Text that reads as something to ask rather than a name to find. */
export function isQuestion(text: string): boolean {
	if (text.endsWith('?')) return true;
	const words = text.toLowerCase().split(/\s+/).filter(Boolean);
	return words.length >= 2 && (QUESTION_WORDS.has(words[0]) || words.length >= 5);
}

/** Frecency for the kinds the box keeps it for. */
function frecency(row: RecentRow): number {
	return row.kind === 'applet' ? 0 : visits.score(row.kind, row.url.slice(row.url.lastIndexOf('/') + 1));
}

/** Records by match quality, then how often and lately you open them, then
 *  how recently they changed. A stand-in title ("New chat") is not a name,
 *  so untitled records never match. */
function matching(rows: RecentRow[], text: string, limit: number): RecentRow[] {
	return rows
		.filter((row) => !row.untitled)
		.map((row) => ({ row, rank: matchRank(row.title, text), f: frecency(row) }))
		.filter((x) => x.rank > 0)
		.sort((a, b) => b.rank - a.rank || b.f - a.f || b.row.ts - a.row.ts)
		.slice(0, limit)
		.map((x) => x.row);
}

/** Settings by match quality, then the order Settings lists them. Unscoped,
 *  only names that start a word with what was typed, so a loose match does
 *  not crowd the records. */
function settingsMatching(text: string, assistantName: string | null, minRank: number, limit: number): Setting[] {
	return SETTINGS.map((setting, order) => {
		const names = [titleOf(setting.title, assistantName), ...setting.words];
		return { setting, order, rank: Math.max(...names.map((n) => matchRank(n, text))) };
	})
		.filter((x) => x.rank >= minRank)
		.sort((a, b) => b.rank - a.rank || a.order - b.order)
		.slice(0, limit)
		.map((x) => x.setting);
}

function commandsMatching(commands: Command[], text: string, limit: number): { rows: Command[]; leading: boolean } {
	const ranked = commands
		.map((command) => ({ command, rank: matchRank(command.label, text) }))
		.filter((x) => x.rank >= 2)
		.sort((a, b) => b.rank - a.rank);
	return { rows: ranked.slice(0, limit).map((x) => x.command), leading: text.length >= 2 && ranked[0]?.rank === 3 };
}

/** The project the record at `route` is filed in, if any. */
function projectAt(route: string): string | null {
	const ref = parseRef(route);
	if (!ref) return null;
	if (ref.kind === 'thread') return ref.id;
	if (ref.kind === 'chat') return chatSessions.projectOf(ref.id);
	if (ref.kind === 'page') return pagesStore.pages.find((p) => p.id === ref.id)?.threads?.[0]?.id ?? null;
	return null;
}

export function projectOfRow(row: RecentRow): string | null {
	if (row.kind === 'chat') return row.session.thread_id ?? null;
	if (row.kind === 'page') return row.page.threads?.[0]?.id ?? null;
	return null;
}

/** Empty: the open thing's project and its latest other members, then Recent. */
function emptyGroups(here: string | null): SearchGroup[] {
	const current = here?.split(/[?#]/)[0] ?? null;
	const rows = recentRows().filter((r) => r.url !== current);
	const groups: SearchGroup[] = [];
	const shown = new Set<string>();

	const projectId = current ? projectAt(current) : null;
	const project = projectId ? projectStore.projects.find((p) => p.id === projectId) : null;
	if (project && !project.archived_at) {
		const members = rows.filter((r) => projectOfRow(r) === project.id).slice(0, HERE_ROWS);
		const own = recentRows({ pinned: true }).find((r) => r.url === `/thread/${project.id}`);
		const hereRows = [...(own && own.url !== current ? [own] : []), ...members];
		if (hereRows.length) {
			groups.push({ label: `In ${project.name}`, rows: hereRows.map((record) => ({ kind: 'record', record })) });
			for (const r of hereRows) shown.add(r.url);
		}
	}

	const room = EMPTY_ROWS - shown.size;
	const recent = rows.filter((r) => !shown.has(r.url)).slice(0, room);
	if (recent.length) groups.push({ label: 'Recent', rows: recent.map((record) => ({ kind: 'record', record })) });
	return groups;
}

export function searchGroups(input: {
	query: string;
	/** The route of the tab search was opened from. */
	here: string | null;
	commands: Command[];
	content: LocalSearchHit[];
	/** Fills `{name}` in settings' titles; "your assistant" when unknown. */
	assistantName?: string | null;
}): SearchGroup[] {
	const { scope, text } = parseQuery(input.query);
	const name = input.assistantName ?? null;

	if (scope === 'setting') {
		// Every setting: the list is short enough to read whole.
		const rows = text ? settingsMatching(text, name, 1, SETTINGS.length) : [...SETTINGS];
		return [{ label: 'Settings', rows: rows.map((setting) => ({ kind: 'setting', setting })) }];
	}

	if (scope === 'command') {
		const rows = text ? commandsMatching(input.commands, text, SCOPED_ROWS).rows : input.commands;
		return [{ label: 'Commands', rows: rows.map((command) => ({ kind: 'command', command })) }];
	}
	if (scope) {
		const all = recentRows({ pinned: true }).filter((r) => r.kind === scope);
		const rows = text ? matching(all, text, SCOPED_ROWS) : all.slice(0, SCOPED_ROWS);
		return [{ label: null, rows: rows.map((record) => ({ kind: 'record', record })) }];
	}
	if (!text) return emptyGroups(input.here);

	const groups: SearchGroup[] = [];
	const url = addressOf(text);
	const question = !url && isQuestion(text);
	const ask: SearchGroup = { label: null, rows: [{ kind: 'ask', query: text }] };

	if (url) groups.push({ label: null, rows: [{ kind: 'open', url }] });
	const days = url ? [] : namedDays(text);
	if (days.length) groups.push({ label: null, rows: days.map((day) => ({ kind: 'day', day })) });
	if (question) groups.push(ask);

	const commands = commandsMatching(input.commands, text, COMMAND_ROWS);
	const commandGroup: SearchGroup = {
		label: 'Commands',
		rows: commands.rows.map((command) => ({ kind: 'command', command })),
	};
	if (commands.leading) groups.push(commandGroup);

	const records = matching(recentRows({ pinned: true }), text, MATCH_ROWS);
	if (records.length) groups.push({ label: null, rows: records.map((record) => ({ kind: 'record', record })) });

	const people = matchEntities(text, ENTITY_ROWS);
	if (people.length) groups.push({ label: null, rows: people.map((entity) => ({ kind: 'entity', entity })) });

	const settings = settingsMatching(text, name, 2, SETTING_ROWS);
	if (settings.length) groups.push({ label: 'Settings', rows: settings.map((setting) => ({ kind: 'setting', setting })) });

	if (!commands.leading && commandGroup.rows.length) groups.push(commandGroup);
	if (!question) groups.push(ask);

	// A chat or page already listed by name is not listed again for its text.
	const listed = new Set(records.map((r) => r.url));
	// A hit with neither a title nor matched text gives nothing to choose by.
	const content = input.content.filter(
		(hit) => (!isUntitled(hit.title) || previewOf(hit)) && !listed.has(contentTarget(hit).route),
	);
	if (content.length) {
		groups.push({ label: 'In your records', rows: content.map((hit) => ({ kind: 'content', hit })) });
	}
	return groups.filter((g) => g.rows.length);
}

/** Records that have a view of their own, by table, and the noun for an untitled one. */
const ROOMS: Record<string, { route: string; noun: string; glyph: string }> = {
	app_chat: { route: '/chat', noun: 'Chat', glyph: 'atlas:chats' },
	app_page: { route: '/page', noun: 'Page', glyph: 'atlas:pages' },
	content_bookmark: { route: '/bookmark', noun: 'Bookmark', glyph: 'atlas:bookmarks' },
};

/** The glyph for a content hit: its view's, else the one for records. */
export function contentGlyph(hit: LocalSearchHit): string {
	return ROOMS[hit.ontology]?.glyph ?? 'atlas:sources';
}

const RECORD_NOUN: Record<RecentKind, string> = { chat: 'Chat', page: 'Page', thread: 'Thread', applet: 'Site' };

/** `content_bookmark` → "Bookmark": the kind as a noun, without the table's prefix. */
function contentNoun(hit: LocalSearchHit): string {
	const noun = ROOMS[hit.ontology]?.noun ?? hit.ontology.replace(/^[a-z]+_/, '').replaceAll('_', ' ');
	return noun.charAt(0).toUpperCase() + noun.slice(1);
}

/** What a row is, as the noun drawn at its right edge. Null for rows that say it in their text. */
export function rowNoun(row: SearchRow): string | null {
	switch (row.kind) {
		case 'record':
			return RECORD_NOUN[row.record.kind];
		case 'day':
			return 'Day';
		case 'entity':
			return row.entity.kind === 'person' ? 'Person' : 'Place';
		case 'content':
			return contentNoun(row.hit);
		case 'setting':
			return 'Setting';
		default:
			return null;
	}
}

/** Where a row opens, for rows that open a view. */
export function rowRoute(row: SearchRow): string | null {
	switch (row.kind) {
		case 'record':
			return row.record.url;
		case 'day':
			return `/day/day_${row.day.date}`;
		case 'entity':
			return row.entity.url;
		case 'content':
			return contentTarget(row.hit).route;
		default:
			return null;
	}
}

/**
 * Where a content hit opens, and what to call its tab. A record with a view
 * of its own (a chat, a page, a bookmark) opens there, not on the raw
 * record view, and an untitled one is named for its kind rather than its
 * table.
 */
export function contentTarget(hit: LocalSearchHit): { route: string; label: string } {
	const room = ROOMS[hit.ontology];
	if (room) return { route: `${room.route}/${hit.record_id}`, label: hit.title || room.noun };
	return { route: `/record/${hit.ontology}/${hit.record_id}`, label: hit.title || hit.ontology };
}

/** The hit's matched text, unless it only repeats the title. */
export function previewOf(hit: LocalSearchHit): string | null {
	const text = hit.preview?.trim();
	return text && text !== hit.title?.trim() ? text : null;
}

/**
 * The + menu's rows as commands, with each Go to entry flattened into its
 * own row ("Go to Pages"). Search is left out, since you are in it.
 */
export function commandsFromMenu(items: ContextMenuItem[]): Command[] {
	const out: Command[] = [];
	for (const item of items) {
		if (item.id === 'search') continue;
		if (item.submenu) {
			for (const sub of item.submenu) {
				if (sub.action) out.push({ id: sub.id, label: `${item.label} ${sub.label}`, icon: sub.icon ?? 'atlas:search', run: sub.action });
			}
		} else if (item.action) {
			out.push({ id: item.id, label: item.label, icon: item.icon ?? 'atlas:plus', shortcut: item.shortcut, run: item.action });
		}
	}
	return out;
}
