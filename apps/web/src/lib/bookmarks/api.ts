/**
 * Bookmarks API — saved web content (`data_content_bookmark`).
 *
 * Distinct from sidebar pins (`app_pins`): a pin is a route you parked on the
 * rail, a bookmark is a page from the world you kept.
 */

type FetchFn = typeof fetch;

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
	thumbnail_url: string | null;
	timestamp: string;
}

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
	},
	fetchFn: FetchFn = fetch
): Promise<BookmarkPageApi> {
	const params = new URLSearchParams();
	params.set("offset", String(opts.offset));
	params.set("limit", String(opts.limit));
	if (opts.search) params.set("search", opts.search);
	if (opts.dir) params.set("dir", opts.dir);

	const res = await fetchFn(`/api/bookmarks?${params}`, { cache: "no-store" });
	if (!res.ok) {
		throw new Error(`Failed to load bookmarks: ${res.status}`);
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
		const detail = await res.text().catch(() => "");
		throw new Error(detail || `Failed to save bookmark: ${res.status}`);
	}
	return res.json();
}
