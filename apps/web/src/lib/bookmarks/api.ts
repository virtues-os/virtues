/**
 * Bookmarks API — saved web content (`data_content_bookmark`).
 *
 * Distinct from sidebar pins (`app_pins`): a pin is a route you parked on the
 * rail, a bookmark is a page from the world you kept.
 */

type FetchFn = typeof fetch;

/**
 * The sentence a person reads when a call fails.
 *
 * A status code is not copy. Every one of these used to reach the screen
 * verbatim - "Failed to load bookmark: 404" is what a broken route looked
 * like to the person who hit it, and a bad save rendered the server's raw
 * JSON. The code goes to the console, where it is useful; the screen gets
 * what happened and what to do about it.
 */
async function failure(res: Response, fallback: string): Promise<Error> {
	const detail = await res
		.clone()
		.text()
		.catch(() => "");
	console.warn(`bookmarks: ${res.status} on ${res.url}`, detail);
	return new Error(fallback);
}


export interface BookmarkApi {
	id: string;
	url: string;
	title: string | null;
	description: string | null;
	/** User-authored marginalia. Never machine-written. */
	note: string | null;
	source_platform: string | null;
	bookmark_type: string | null;
	author: string | null;
	tags: string[] | null;
	/** The picture, always a copy on your server, or null. */
	picture_url: string | null;
	/** How it arrived. */
	door: BookmarkDoor;
	/** What the card shows beyond the row's columns. Absent facts are left out. */
	details: BookmarkDetails;
	/** The source's own time: when a post was written, or a page bookmarked. */
	timestamp: string;
	/** When it was saved, as well as your server can tell. The room groups by this. */
	saved_at: string;
	/** From the extraction record — "article", "reference", "repository". */
	medium: string | null;
}

/**
 * `held` and `queued` are both pending on the box; they are separate here
 * because only one of them is waiting on something that exists. `queued` is a
 * page or an image the sweep will read. `held` is a file no pass reads yet —
 * video, audio, or a file that left Drive. (It meant every image until the
 * image pass arrived; the box derives it from the sweep's own claim rule.)
 */
export type BookmarkState =
	| "enriched"
	| "queued"
	| "held"
	| "failed"
	| "skipped";

/** A post's link: the short form in its text, where it leads, and how X shows it. */
export interface PostLink {
	short: string;
	expanded: string;
	display: string;
}

/** Every picture here is a copy on your server. */
export interface BookmarkDetails {
	author_name?: string;
	author_picture_url?: string;
	site_name?: string;
	icon_url?: string;
	duration_secs?: number;
	reading_minutes?: number;
	links?: PostLink[];
	/** The short link X appends for the post's own picture. */
	media_short_url?: string;
	/** The page a post links to. */
	link_preview?: { url?: string; title?: string; site_name?: string; picture_url?: string };
}

/** The door a save came through, as the room's filters name it. */
export type BookmarkDoor = "instagram" | "x" | "github" | "iphone" | "hand" | "browser";

export interface BookmarkPageApi {
	items: BookmarkApi[];
	total: number;
}

export async function getBookmarksPage(
	opts: {
		offset: number;
		limit: number;
		search?: string;
		dir?: "asc" | "desc";
		door?: BookmarkDoor;
	},
	fetchFn: FetchFn = fetch
): Promise<BookmarkPageApi> {
	const params = new URLSearchParams();
	params.set("offset", String(opts.offset));
	params.set("limit", String(opts.limit));
	if (opts.search) params.set("search", opts.search);
	if (opts.dir) params.set("dir", opts.dir);
	if (opts.door) params.set("door", opts.door);

	const res = await fetchFn(`/api/bookmarks?${params}`, { cache: "no-store" });
	if (!res.ok) {
		throw await failure(res, "Your server couldn't list your bookmarks. Try again.");
	}
	return res.json();
}

/** The extraction record, as the enrichment pass writes it. */
export interface ExtractionRecord {
	description?: string | null;
	medium?: string | null;
	subject?: string[];
	entities?: string[];
	style?: string | null;
	likely_queries?: string[];
}

export interface BookmarkDetailApi extends BookmarkApi {
	/** How much the box knows about this one yet. */
	state: BookmarkState;
	/** Set when the source no longer has it. The row and its note survive. */
	deleted_at_source: string | null;
	enrichment_model: string | null;
	enriched_at: string | null;
	extraction: ExtractionRecord | null;
	/** The record's entities, each with its Wiki page when one has exactly that name. */
	mentions: { name: string; route: string | null }[];
}

export async function getBookmark(
	id: string,
	fetchFn: FetchFn = fetch
): Promise<BookmarkDetailApi> {
	const res = await fetchFn(`/api/bookmarks/${encodeURIComponent(id)}`, {
		cache: "no-store",
	});
	if (!res.ok) {
		throw await failure(
			res,
			res.status === 404
				? "That bookmark isn't on your server. Go back to Bookmarks to see what is."
				: "Your server couldn't open that bookmark. Try again."
		);
	}
	return res.json();
}

/**
 * Write the note. Blank clears it — writing nothing is a legitimate edit.
 *
 * The note is part of the bookmark's embed text, so an edit changes what the
 * row is findable by; the indexer picks that up on its next pass.
 */
export async function updateBookmarkNote(
	id: string,
	note: string | null,
	fetchFn: FetchFn = fetch
): Promise<BookmarkDetailApi> {
	const res = await fetchFn(`/api/bookmarks/${encodeURIComponent(id)}/note`, {
		method: "PATCH",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({ note }),
	});
	if (!res.ok) {
		throw await failure(
			res,
			res.status === 404
				? "That bookmark isn't on your server, so the note has nowhere to go."
				: "Your server couldn't save that note. Try again."
		);
	}
	return res.json();
}

/** Save a URL. Idempotent on the canonicalized URL — re-saving updates. */
export async function saveBookmark(
	body: { url: string; note?: string; tags?: string[] },
	fetchFn: FetchFn = fetch
): Promise<BookmarkApi> {
	const res = await fetchFn("/api/bookmarks", {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(body),
	});
	if (!res.ok) {
		throw await failure(
			res,
			res.status === 400
				? "That isn't an address your server can save. Enter one starting with http:// or https://."
				: "Your server couldn't save that link. Try again."
		);
	}
	return res.json();
}
