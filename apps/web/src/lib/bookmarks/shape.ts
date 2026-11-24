/**
 * How a bookmark is laid out, decided once for the Wall, the List, the
 * bookmark page and Storage's front page.
 *
 * There is no layout per platform. A bookmark is laid out from the fields it
 * has, so a site nobody has seen before still looks right:
 *
 * - The headline is the first of: a title, a post's text, a picture, the
 *   address. A domain is never a headline.
 * - A picture above a title is a cover. A picture under a post's text is an
 *   attachment. A post whose picture is the content (Instagram) shows the
 *   picture before the caption.
 *
 * The plan is agents/plan/bookmarks-plan.md, section 1.
 */

import { backendUrl } from '$lib/config/backend';
import type { BookmarkApi, BookmarkDoor } from './api';

/** The room's filters, in the order the room shows them. */
export const DOOR_FILTERS: readonly { door: BookmarkDoor | null; label: string }[] = [
	{ door: null, label: 'All' },
	{ door: 'instagram', label: 'Instagram' },
	{ door: 'x', label: 'X' },
	{ door: 'browser', label: 'Browser' },
	{ door: 'github', label: 'GitHub' },
	{ door: 'hand', label: 'Saved by hand' },
	{ door: 'iphone', label: 'iPhone' },
];

/** Platforms whose saves are posts: text by an author, maybe a picture. */
const POST_PLATFORMS = new Set(['x', 'instagram']);
/** Posts whose picture is the content, so it comes before the caption. */
const PICTURE_FIRST = new Set(['instagram']);

const PLATFORM_NAMES: Record<string, string> = {
	x: 'X',
	instagram: 'Instagram',
	github: 'GitHub',
	safari: 'Safari',
	chrome: 'Chrome',
	'chrome-beta': 'Chrome Beta',
	'chrome-canary': 'Chrome Canary',
	arc: 'Arc',
	dia: 'Dia',
	brave: 'Brave',
	edge: 'Edge',
	vivaldi: 'Vivaldi',
	opera: 'Opera',
	chromium: 'Chromium',
};

export type BookmarkShape = {
	kind: 'post' | 'page' | 'picture' | 'address';
	/** The title, the post's text or the address. Null only for a picture. */
	headline: string | null;
	/** `@handle` for a post. */
	handle: string | null;
	/** A post author's display name, when known. */
	authorName: string | null;
	/** A post author's picture, a copy on your server. */
	authorPicture: string | null;
	/** The site a page belongs to, for the row above its title. */
	site: string | null;
	/** The site's icon, a copy on your server. */
	icon: string | null;
	/** A page's own summary, under its title. Posts have none. */
	summary: string | null;
	/** The picture's address on your server. */
	picture: string | null;
	/** A video's length, "18:42", drawn on its picture. */
	duration: string | null;
	/** Facts the page offered, in words: "9 min read". Only ones that exist. */
	facts: string[];
	/** The page a post links to. */
	preview: LinkPreview | null;
	/** True when the picture comes before the headline. */
	pictureFirst: boolean;
	/** The person's note. */
	note: string | null;
};

export type LinkPreview = {
	url: string;
	/** The site's name, or its address without "www.". */
	site: string;
	title: string | null;
	picture: string | null;
};

const clean = (s: string | null | undefined) => {
	const t = s?.trim();
	return t ? t : null;
};

/** Internal viewer routes (`/drive/file_…`) are a place to open, not an address to read. */
export const isInternal = (b: Pick<BookmarkApi, 'url'>) => b.url.startsWith('/');

function parse(url: string): URL | null {
	try {
		return new URL(url);
	} catch {
		return null;
	}
}

export function hostOf(url: string): string {
	return parse(url)?.hostname.replace(/^www\./, '') ?? url;
}

/**
 * An http or https address. `utils/webLinks.isWebLink` asks the same and
 * more, but it imports the toast and menu code, which this module, loaded by
 * Storage and search as well, has no use for.
 */
export function isHttp(url: string): boolean {
	const p = parse(url)?.protocol;
	return p === 'http:' || p === 'https:';
}

/** A host on the local network: an IP literal, localhost, or a .local name. */
export function isLocal(url: string): boolean {
	const host = parse(url)?.hostname ?? '';
	return (
		host === 'localhost' ||
		host.endsWith('.local') ||
		host.endsWith('.localhost') ||
		/^\d{1,3}(\.\d{1,3}){3}$/.test(host) ||
		host.startsWith('[')
	);
}

/** The address, readable: host and path, no scheme, no trailing slash. */
export function addressOf(url: string): string {
	const u = parse(url);
	if (!u) return url;
	const path = u.pathname === '/' ? '' : u.pathname.replace(/\/$/, '');
	const port = u.port ? `:${u.port}` : '';
	const out = `${u.hostname.replace(/^www\./, '')}${port}${path}`;
	return out.length > 64 ? `${out.slice(0, 63)}…` : out;
}

export function platformName(platform: string | null): string | null {
	if (!platform) return null;
	return PLATFORM_NAMES[platform] ?? platform.charAt(0).toUpperCase() + platform.slice(1);
}

/** The picture's address, made reachable from wherever the app runs. */
export function pictureSrc(b: Pick<BookmarkApi, 'picture_url'>): string | null {
	return b.picture_url ? backendUrl(b.picture_url) : null;
}

/** "18:42" or "1:02:03". */
export function formatDuration(secs: number): string {
	const h = Math.floor(secs / 3600);
	const m = Math.floor((secs % 3600) / 60);
	const sec = String(secs % 60).padStart(2, '0');
	return h ? `${h}:${String(m).padStart(2, '0')}:${sec}` : `${m}:${sec}`;
}

/**
 * A post's text as a person reads it: each short link replaced by the
 * readable address it leads to, and the short link X appends for the post's
 * own picture removed, since the picture is shown.
 */
export function postText(b: BookmarkApi): string | null {
	let text = b.description ?? '';
	for (const link of b.details?.links ?? []) {
		if (link.short) text = text.split(link.short).join(link.display || link.expanded);
	}
	const media = b.details?.media_short_url;
	if (media) text = text.split(media).join('');
	return clean(text);
}

/**
 * A post author's display name. X sync keeps it; for posts saved before it
 * did, the page title the enrichment pass kept ("Nick (@nick) on X") names
 * the author, and that is read instead.
 */
function authorNameOf(b: BookmarkApi): string | null {
	const kept = clean(b.details?.author_name);
	if (kept) return kept;
	const handle = clean(b.author)?.replace(/^@/, '');
	const title = clean(b.title);
	if (!handle || !title) return null;
	const suffix = ` (@${handle}) on X`;
	return title.endsWith(suffix) ? clean(title.slice(0, -suffix.length)) : null;
}

const route = (path: string | undefined) => (path ? backendUrl(path) : null);

function previewOf(b: BookmarkApi): LinkPreview | null {
	const p = b.details?.link_preview;
	if (!p?.url) return null;
	return { url: p.url, site: clean(p.site_name) ?? hostOf(p.url), title: clean(p.title), picture: route(p.picture_url) };
}

function factsOf(b: BookmarkApi, duration: string | null, picture: string | null): string[] {
	const facts: string[] = [];
	if (duration && !picture) facts.push(duration);
	if (b.details?.reading_minutes) facts.push(`${b.details.reading_minutes} min read`);
	return facts;
}

const EMPTY = {
	handle: null,
	authorName: null,
	authorPicture: null,
	site: null,
	icon: null,
	summary: null,
	picture: null,
	duration: null,
	facts: [] as string[],
	preview: null,
	pictureFirst: true,
};

export function shapeOf(b: BookmarkApi): BookmarkShape {
	const title = clean(b.title);
	const text = clean(b.description);
	const picture = pictureSrc(b);
	const note = clean(b.note);
	const platform = b.source_platform ?? '';
	const handle = clean(b.author) ? `@${clean(b.author)!.replace(/^@/, '')}` : null;
	const duration = b.details?.duration_secs ? formatDuration(b.details.duration_secs) : null;

	// A post leads with its text even when it has a title: the enrichment
	// pass fills `title` from the page's own title tag, which for a post is
	// "Name (@handle) on X", a label for the page rather than the post.
	if (POST_PLATFORMS.has(platform) && (text || picture)) {
		return {
			...EMPTY,
			kind: 'post',
			headline: postText(b),
			handle,
			authorName: authorNameOf(b),
			authorPicture: route(b.details?.author_picture_url),
			picture,
			preview: previewOf(b),
			pictureFirst: PICTURE_FIRST.has(platform),
			note,
		};
	}
	if (title) {
		return {
			...EMPTY,
			kind: 'page',
			headline: title,
			site: siteOf(b),
			icon: route(b.details?.icon_url),
			summary: text,
			picture,
			duration: picture ? duration : null,
			facts: factsOf(b, duration, picture),
			note,
		};
	}
	if (picture) {
		return { ...EMPTY, kind: 'picture', headline: null, picture, note };
	}
	return {
		...EMPTY,
		kind: 'address',
		headline: isInternal(b) ? 'Saved file' : addressOf(b.url),
		site: isLocal(b.url) ? 'Local network' : null,
		note,
	};
}

/** The row above a page's title: the name the site gives itself, or its address. */
function siteOf(b: BookmarkApi): string | null {
	if (isInternal(b)) return null;
	if (b.door === 'github') return 'GitHub';
	if (isLocal(b.url)) return 'Local network';
	return clean(b.details?.site_name) ?? hostOf(b.url);
}

/** How it arrived, for a card's footer: "Chrome", "iPhone", "Saved by hand". */
export function arrivedBy(b: BookmarkApi): string {
	switch (b.door) {
		case 'iphone':
			return 'iPhone';
		case 'hand':
			return 'Saved by hand';
		case 'browser':
			return platformName(b.source_platform) ?? 'Browser';
		default:
			return platformName(b.source_platform) ?? 'Saved';
	}
}

const sameDay = (a: Date, b: Date) =>
	a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();

/** "Today", "Yesterday", "Nov 21", or "Nov 21, 2019" in another year. */
export function dayLabel(iso: string, now = new Date()): string {
	const d = new Date(iso);
	if (Number.isNaN(d.getTime())) return '';
	if (sameDay(d, now)) return 'Today';
	const yesterday = new Date(now);
	yesterday.setDate(now.getDate() - 1);
	if (sameDay(d, yesterday)) return 'Yesterday';
	return d.toLocaleDateString('en-US', {
		month: 'short',
		day: 'numeric',
		...(d.getFullYear() === now.getFullYear() ? {} : { year: 'numeric' }),
	});
}

/** A card's footer: how it arrived and when. */
export function footerOf(b: BookmarkApi, now = new Date()): string {
	return `${arrivedBy(b)} · ${dayLabel(b.saved_at, now)}`;
}

/** The bookmark page's crumb: its headline, in words. */
export function crumbOf(b: BookmarkApi): string {
	const s = shapeOf(b);
	if (s.kind === 'page') return s.headline!;
	if (s.kind === 'post') return s.authorName || s.handle ? `Post by ${s.authorName ?? s.handle}` : 'Post';
	if (s.kind === 'picture') return b.bookmark_type === 'screenshot' ? 'Saved screenshot' : 'Saved picture';
	return s.headline!;
}

/**
 * The open button's words, or null when there is nowhere to open.
 *
 * Only a web address (http or https) is opened. A browser can hold a
 * bookmarklet, a `javascript:` address, and opening one would run its script
 * inside the app.
 */
export function openLabel(b: BookmarkApi): string | null {
	if (!isHttp(b.url)) return null;
	if (POST_PLATFORMS.has(b.source_platform ?? '') || b.door === 'github') {
		return `Open in ${platformName(b.door === 'github' ? 'github' : b.source_platform)}`;
	}
	const site = clean(b.details?.site_name);
	return site ? `Open in ${site}` : `Open ${addressOf(b.url).split('/')[0]}`;
}

/** Monday 00:00 of the week holding `d`, local time. */
function weekStart(d: Date): Date {
	const s = new Date(d.getFullYear(), d.getMonth(), d.getDate());
	s.setDate(s.getDate() - ((s.getDay() + 6) % 7));
	return s;
}

/**
 * The time group a save belongs to: This week, Last week, Earlier in
 * [this month], then month names this year, then years. Weeks start on
 * Monday.
 */
export function groupOf(iso: string, now = new Date()): string {
	const d = new Date(iso);
	const thisWeek = weekStart(now);
	if (d >= thisWeek) return 'This week';
	const lastWeek = new Date(thisWeek);
	lastWeek.setDate(thisWeek.getDate() - 7);
	if (d >= lastWeek) return 'Last week';
	const month = d.toLocaleDateString('en-US', { month: 'long' });
	if (d.getFullYear() === now.getFullYear()) {
		return d.getMonth() === now.getMonth() ? `Earlier in ${month}` : month;
	}
	return String(d.getFullYear());
}

/** Consecutive saves in the same time group. The input is newest first. */
export function groupBySave<T extends Pick<BookmarkApi, 'saved_at'>>(
	items: T[],
	now = new Date()
): { label: string; items: T[] }[] {
	const groups: { label: string; items: T[] }[] = [];
	for (const item of items) {
		const label = groupOf(item.saved_at, now);
		const last = groups[groups.length - 1];
		if (last?.label === label) last.items.push(item);
		else groups.push({ label, items: [item] });
	}
	return groups;
}

/**
 * A rough height for a card, in px, for placing it in a column before it is
 * drawn. Only the order of magnitude matters: it decides which column is
 * shortest.
 */
export function estimateHeight(b: BookmarkApi, columnWidth: number): number {
	const s = shapeOf(b);
	const charsPerLine = Math.max(20, columnWidth / 8);
	let h = 32 + 20; // padding and footer
	if (s.handle) h += 28;
	if (s.preview) h += s.preview.picture ? 80 : 56;
	if (s.facts.length) h += 20;
	if (s.site) h += 28;
	if (s.picture) h += s.kind === 'post' ? columnWidth : columnWidth * 0.55;
	if (s.headline) h += Math.min(3, Math.ceil(s.headline.length / charsPerLine)) * 24 + 12;
	if (s.summary) h += Math.min(3, Math.ceil(s.summary.length / charsPerLine)) * 20 + 12;
	if (s.note) h += Math.ceil(s.note.length / charsPerLine) * 20 + 28;
	return h;
}

/**
 * Place cards in columns, each in the shortest column so far, in order. Plain
 * CSS columns fill top to bottom, which scrambles date order across a row;
 * this keeps it left to right.
 */
export function intoColumns<T>(items: T[], columns: number, height: (item: T) => number): T[][] {
	const cols: T[][] = Array.from({ length: Math.max(1, columns) }, () => []);
	const heights = cols.map(() => 0);
	for (const item of items) {
		const i = heights.indexOf(Math.min(...heights));
		cols[i].push(item);
		heights[i] += height(item);
	}
	return cols;
}
