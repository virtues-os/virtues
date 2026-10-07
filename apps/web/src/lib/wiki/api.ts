/**
 * Wiki API Client
 *
 * Fetches wiki pages from the backend API.
 * Backend wiki pages are views of entities/narratives.
 */

import { apiGet, apiSend, deadline, request } from '$lib/api/client';
import type { NoteAnchor } from './dayNotes';

// ============================================================================
// API Response Types (match Rust backend types)
// ============================================================================

export interface WikiPersonApi {
	id: string;
	name: string;
	content: string | null;
	article: string | null;
	article_updated_at: string | null;
	picture: string | null;
	cover_image: string | null;
	emails: string[];
	phones: string[];
	birthday: string | null; // ISO date string
	/** When they died, if they have; precision the person gave (year/month/day). */
	died_on: string | null;
	died_precision: string | null;
	/** The authored line about what this person means — one sentence, verbatim. */
	bond: string | null;
	instagram: string | null;
	facebook: string | null;
	linkedin: string | null;
	x: string | null;
	relationship_category: string | null;
	nickname: string | null;
	notes: string | null;
	/** Surfaces this entity also answers to (migration 0037). */
	aliases: string[];
	/** Is the record keeping this article up to date? Off unless asked. */
	article_maintained?: boolean;
	created_at: string;
	updated_at: string;
}

export interface WikiPlaceApi {
	id: string;
	name: string;
	content: string | null;
	article: string | null;
	article_updated_at: string | null;
	cover_image: string | null;
	category: string | null;
	address: string | null;
	latitude: number | null;
	longitude: number | null;
	/** Visits, COUNTED from wiki_refs. The columns of these names had no
	 *  writer, so this section reported "Total visits: 0" for somewhere you go
	 *  weekly; these now carry the real count and its two dates. */
	seen_count: number | null;
	first_seen: string | null;
	last_seen: string | null;
	/** The phone keeps no audio while you are inside this place. */
	is_audio_muted?: boolean;
	/** Write-only: the update takes it, the page response does not carry it. */
	radius_m?: number;
	created_at: string;
	updated_at: string;
	/** Is the record keeping this article up to date? Off unless asked. */
	article_maintained?: boolean;
}

export interface WikiOrganizationApi {
	id: string;
	name: string;
	content: string | null;
	article: string | null;
	article_updated_at: string | null;
	cover_image: string | null;
	organization_type: string | null;
	relationship_type: string | null;
	role_title: string | null;
	started_at: string | null;
	ended_at: string | null;
	created_at: string;
	updated_at: string;
	/** Is the record keeping this article up to date? Off unless asked. */
	article_maintained?: boolean;
	aliases?: string[];
}


export interface WikiDayApi {
	id: string;
	date: string; // ISO date string
	start_timezone: string | null;
	/** The day's prose, from wiki_day_prose. The article page is its only home (0106). */
	article?: string | null;
	new_entity_count: number;
	new_topic_count: number;
	sleep_cycles: Array<{
		start_time: string;
		end_time: string;
		dominant_stage: string;
		avg_hr: number | null;
		autonomic_z: number | null;
	}>;
	created_at: string;
	updated_at: string;
}

// ============================================================================
// List Item Types
// ============================================================================

export interface WikiPersonListItem {
	id: string;
	name: string;
	picture: string | null;
	relationship_category: string | null;
	/** Records mentioning this entity. The index's sort key — see wiki.rs. */
	ref_count: number;
}

export interface WikiPlaceListItem {
	id: string;
	name: string;
	category: string | null;
	address: string | null;
	/** Records mentioning this entity. The index's sort key — see wiki.rs. */
	ref_count: number;
}

export interface WikiOrganizationListItem {
	id: string;
	name: string;
	organization_type: string | null;
	relationship_type: string | null;
	/** Records mentioning this entity. The index's sort key — see wiki.rs. */
	ref_count: number;
}


// ============================================================================
// API Functions
// ============================================================================

type FetchFn = typeof fetch;

// --- Person ---

export async function getPersonById(
	id: string,
	fetchFn: FetchFn = fetch
): Promise<WikiPersonApi | null> {
	const res = await fetchFn(`/api/wiki/person/${encodeURIComponent(id)}`);
	if (!res.ok) return null;
	return res.json();
}

/**
 * File a person as an organization instead.
 *
 * Returns the new org route — the person route stops resolving the moment this
 * succeeds, so callers must navigate or reload rather than keep the old id.
 */
export async function reclassifyPersonAsOrg(
	id: string,
	fetchFn: FetchFn = fetch
): Promise<{ id: string; route: string }> {
	const res = await fetchFn(`/api/entities/people/${id}/reclassify-as-org`, { method: 'POST' });
	if (!res.ok) throw new Error(`Failed to reclassify: ${res.statusText}`);
	return res.json();
}

/**
 * Whether merging two buckets adds their values.
 *
 * `total` grows with the zoom level and stands on zero; `rate` is an average
 * and floats between its own floor and ceiling. The distinction decides both
 * the arithmetic and the drawing.
 */
export type MeasureKind = 'total' | 'rate';

/** One entry in a lane's measure menu. */
export interface MeasureInfo {
	id: string;
	label: string;
	unit: string;
	kind: MeasureKind;
}

/** One lane of the lifeline: a registry domain and its per-bucket series. */
export interface LifelineLane {
	id: string;
	sources: string[];
	density: number[];
	peak: number;
	/** Smallest non-empty bucket — the baseline a `rate` is drawn against. */
	floor: number;
	/** When this lane started collecting. Before it, the lane wasn't watching. */
	first_seen: string | null;
	/** Which measure produced `density`; `records` when none was chosen. */
	measure: string;
	measure_label: string;
	unit: string;
	kind: MeasureKind;
	available: MeasureInfo[];
}

export interface LifelineData {
	from: string;
	to: string;
	buckets: number;
	lanes: LifelineLane[];
}

/** Per-lane series over a window. The server buckets; the client only draws. */
export async function getLifeline(
	buckets: number,
	from?: string,
	to?: string,
	expand?: string[],
	measures?: Record<string, string>,
	fetchFn: FetchFn = fetch
): Promise<LifelineData | null> {
	const p = new URLSearchParams({ buckets: String(buckets) });
	// Omitting the window asks the server for the whole record. A lifeline
	// defaulted to the last year is not a lifeline — this corpus starts in 2017.
	if (from) p.set('from', from);
	if (to) p.set('to', to);
	if (expand?.length) p.set('expand', expand.join(','));
	const pairs = Object.entries(measures ?? {}).map(([lane, id]) => `${lane}:${id}`);
	if (pairs.length) p.set('measures', pairs.join(','));
	const res = await fetchFn(`/api/wiki/lifeline?${p}`);
	if (!res.ok) return null;
	return res.json();
}

/** Create a person by hand — the record only ever discovered them before. */
export async function createPerson(name: string, fetchFn: FetchFn = fetch): Promise<{ id: string; route: string }> {
	const res = await fetchFn('/api/entities/people', {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ name })
	});
	if (!res.ok) throw new Error('Could not create that person');
	return res.json();
}

/** Delete an entity, and everything that pointed at it. */
export async function deleteEntity(
	entityType: 'person' | 'place' | 'org',
	id: string,
	fetchFn: FetchFn = fetch
): Promise<void> {
	const path = entityType === 'person' ? 'people' : entityType === 'org' ? 'orgs' : 'places';
	const res = await fetchFn(`/api/entities/${path}/${id}`, { method: 'DELETE' });
	if (!res.ok) {
		const body = await res.json().catch(() => null);
		throw new Error(body?.error ?? body?.message ?? 'Could not delete that');
	}
}

/** A note in the margin of a subject. */
export interface WikiNote {
	id: number;
	subject_type: string;
	subject_id: string;
	kind: string;
	body: string;
	author: string;
	source_refs: unknown;
	created_at: string;
	resolution: string | null;
	/** The passage the note sits beside, when it was written on one. */
	anchor?: NoteAnchor | null;
}

/** Open notes on a subject. */
export async function listNotes(
	subjectType: string,
	subjectId: string,
	fetchFn: FetchFn = fetch
): Promise<WikiNote[]> {
	const res = await fetchFn(`/api/wiki/notes/${subjectType}/${subjectId}`);
	if (!res.ok) return [];
	return res.json();
}

/** Leave a note yourself. Human notes need no citation — you were there. */
export async function createNote(
	subjectType: string,
	subjectId: string,
	body: string,
	opts: { kind?: string; anchor?: NoteAnchor } = {},
	fetchFn: FetchFn = fetch
): Promise<WikiNote> {
	const res = await fetchFn(`/api/wiki/notes/${subjectType}/${subjectId}`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ body, kind: opts.kind ?? 'memo', anchor: opts.anchor ?? null })
	});
	if (!res.ok) throw new Error('Could not save that note');
	return res.json();
}

/** Close a note. Accepting hands the editing back to you. */
export async function resolveNote(
	id: number,
	resolution: 'accepted' | 'dismissed',
	fetchFn: FetchFn = fetch
): Promise<void> {
	const res = await fetchFn(`/api/wiki/notes/${id}/resolve`, {
		method: 'PUT',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ resolution })
	});
	if (!res.ok) throw new Error('Could not close that note');
}

/** One heart-rate sample for the day's Autonomic chart. */
export interface DayHeartRateSample {
	timestamp: string;
	bpm: number;
}

/**
 * The day's raw HR samples, oldest first. Sparse days are normal.
 *
 * `tz` anchors the local-day window. Without it the server falls back to the
 * day's recorded zone, which is right for a past day and wrong for today when
 * the box and the browser disagree about where midnight is.
 */
export async function getDayHeartRate(
	date: string,
	tz?: string,
	fetchFn: FetchFn = fetch
): Promise<DayHeartRateSample[]> {
	const q = tz ? `?tz=${encodeURIComponent(tz)}` : '';
	const res = await fetchFn(`/api/wiki/day/${encodeURIComponent(date)}/heart-rate${q}`);
	if (!res.ok) return [];
	return res.json();
}

/** The article join row for a subject. `page_id` is what the editor opens. */
export interface WikiArticleApi {
	id: string;
	subject_type: string;
	subject_id: string;
	page_id: string;
	/** auto | never — see `Maintenance`. */
	maintenance: Maintenance;
}

/** A subject's article row, or null when no article exists yet. */
export async function getArticle(
	subjectType: string,
	subjectId: string,
	fetchFn: FetchFn = fetch
): Promise<WikiArticleApi | null> {
	const res = await fetchFn(
		`/api/wiki/articles/${subjectType}/${encodeURIComponent(subjectId)}`
	);
	if (!res.ok) return null;
	return res.json();
}

/** Open notes across the whole record — the Overview's what-changed count. */
export async function countOpenNotes(fetchFn: FetchFn = fetch): Promise<number> {
	const res = await fetchFn('/api/wiki/notes-open-count');
	if (!res.ok) return 0;
	const j = await res.json();
	return typeof j.open === 'number' ? j.open : 0;
}

/**
 * One edit to some article, for the History room. `version_number` holds the
 * page as the edit left it; `before_version` holds it as it was before, which
 * is what putting this edit back restores. Null on a page's first version.
 *
 * Two entries for one page can share `version_number`: an old chat edit's
 * last version both ends one entry and starts the one read against the live
 * page. They never share the pair, so `editKey` is an entry's identity.
 */
export interface HistoryEntry {
	subject_type: string;
	subject_id: string;
	route: string;
	title: string;
	author: string;
	at: string;
	version_number: number;
	before_version: number | null;
}

/** One line of a diff. `kind` is 'add' | 'del' | 'ctx'. */
export interface DiffLine {
	kind: string;
	text: string;
}

/** One edit to one article, with what changed. Versions read as `HistoryEntry`'s do. */
export interface ArticleRevision {
	version_number: number;
	before_version: number | null;
	author: string;
	at: string;
	diff: DiffLine[];
	is_current: boolean;
}

/** One edit's identity within its page: the versions it went from and to. */
export function editKey(e: { before_version: number | null; version_number: number }): string {
	return `${e.before_version ?? "first"}->${e.version_number}`;
}

/** Every recent edit to any article, newest first. */
export async function listHistory(
	limit = 50,
	fetchFn: FetchFn = fetch
): Promise<HistoryEntry[]> {
	const res = await fetchFn(`/api/wiki/history?limit=${limit}`);
	if (!res.ok) return [];
	return res.json();
}

/** One article's edit history, with diffs. */
export async function getArticleHistory(
	subjectType: string,
	subjectId: string,
	fetchFn: FetchFn = fetch
): Promise<ArticleRevision[]> {
	const res = await fetchFn(`/api/wiki/articles/${subjectType}/${subjectId}/history`);
	if (!res.ok) return [];
	return res.json();
}

/** One piece of prose that mentions a subject. */
export interface SubjectBacklink {
	page_id: string;
	title: string;
	/** The subject's route if it is an article, else the page route. */
	route: string;
	is_article: boolean;
}

/** Everything whose prose links to this subject. Derived at read time. */
export async function getSubjectBacklinks(
	subjectType: string,
	subjectId: string,
	fetchFn: FetchFn = fetch
): Promise<SubjectBacklink[]> {
	const res = await fetchFn(`/api/wiki/subjects/${subjectType}/${subjectId}/backlinks`);
	if (!res.ok) return [];
	return res.json();
}

/**
 * Write a subject's first article, now.
 *
 * Synchronous by design — one model call the user is waiting on. Returns the
 * created article; the caller should re-fetch the entity to pick up the prose.
 */
export async function writeArticle(
	subjectType: string,
	subjectId: string,
	fetchFn: FetchFn = fetch
): Promise<{ id: string; page_id: string }> {
	const res = await fetchFn(`/api/wiki/articles/${subjectType}/${subjectId}`, { method: 'POST' });
	if (!res.ok) {
		const body = await res.json().catch(() => null);
		throw new Error(body?.error ?? body?.message ?? `Could not write the article`);
	}
	return res.json();
}

/** Turn maintenance on or off. Off means the AI never touches this article. */
/** The owner's own page: their name, their document, and the apparatus. */
export interface MeApi {
	person_id: string | null;
	name: string | null;
	birth_date: string | null;
	article: string | null;
	article_updated_at: string | null;
	page_id: string | null;
	chapters: ChapterApi[];
	years: number[];
}

export async function getMe(fetchFn: FetchFn = fetch): Promise<MeApi | null> {
	const res = await fetchFn('/api/wiki/me');
	if (!res.ok) return null;
	return res.json();
}

/** A subject the person named because it mattered. Not a span. */
export interface StoryApi {
	id: string;
	title: string;
	summary: string | null;
	started_at: string | null;
	ended_at: string | null;
	started_precision: string | null;
	ended_precision: string | null;
	has_article: boolean;
}

export async function listStories(fetchFn: FetchFn = fetch): Promise<StoryApi[]> {
	const res = await fetchFn('/api/wiki/stories');
	if (!res.ok) return [];
	return res.json();
}

/** Only the person starts one — the editor may not create a subject. */
export async function createStory(title: string, fetchFn: FetchFn = fetch): Promise<StoryApi> {
	const res = await fetchFn('/api/wiki/stories', {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ title })
	});
	if (!res.ok) throw new Error('Could not start that');
	return res.json();
}

export async function updateStory(
	id: string,
	fields: Partial<Pick<StoryApi, 'title' | 'summary' | 'started_at' | 'ended_at'>>,
	fetchFn: FetchFn = fetch
): Promise<void> {
	const res = await fetchFn(`/api/wiki/story/${id}`, {
		method: 'PUT',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(fields)
	});
	if (!res.ok) throw new Error('Could not save that');
}

export async function deleteStory(id: string, fetchFn: FetchFn = fetch): Promise<void> {
	const res = await fetchFn(`/api/wiki/story/${id}`, { method: 'DELETE' });
	if (!res.ok) throw new Error('Could not remove that');
}

/** Give a story a page, seeded with their words for the editor to fill. */
export async function startStoryArticle(id: string, fetchFn: FetchFn = fetch): Promise<void> {
	const res = await fetchFn(`/api/wiki/story/${id}/article`, { method: 'POST' });
	if (!res.ok) throw new Error('Could not start that page');
}

/** Correct a chapter — a boundary, a name, or the sentence about why it ended. */
export async function updateChapter(
	id: string,
	fields: { title?: string; started_at?: string; ended_at?: string; changepoint?: string; summary?: string },
	fetchFn: FetchFn = fetch
): Promise<void> {
	const res = await fetchFn(`/api/wiki/chapter/${id}`, {
		method: 'PUT',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(fields)
	});
	if (!res.ok) {
		const body = await res.json().catch(() => null);
		throw new Error(body?.error ?? 'Could not save that');
	}
}

/** Unname a chapter. The years it covered stay, as an unnamed stretch. */
export async function deleteChapter(id: string, fetchFn: FetchFn = fetch): Promise<void> {
	const res = await fetchFn(`/api/wiki/chapter/${id}`, { method: 'DELETE' });
	if (!res.ok) throw new Error('Could not remove that');
}

/** One day of a year, with the line the year reads. */
export interface YearDayApi {
	date: string;
	narrated: boolean;
	event_count: number;
	lede: string | null;
}

/** A year: a subject with a page, not a folder of days. */
export interface YearApi {
	id: string;
	year: number;
	title: string | null;
	summary: string | null;
	days_recorded: number;
	days_narrated: number;
	has_article: boolean;
	chapters: string[];
	days: YearDayApi[];
	article: string | null;
	/** Decided by the box, because what the page may OFFER depends on it. */
	state: 'before_record' | 'thin' | 'dense';
}

export type YearSummaryApi = Omit<YearApi, 'days' | 'article' | 'state'>;

/** Every year of the life, newest first. Reading this writes nothing. */
export async function listYears(fetchFn: FetchFn = fetch): Promise<YearSummaryApi[]> {
	const res = await fetchFn('/api/wiki/years');
	if (!res.ok) return [];
	return res.json();
}

export async function getYear(year: number, fetchFn: FetchFn = fetch): Promise<YearApi | null> {
	const res = await fetchFn(`/api/wiki/year/${year}`);
	if (!res.ok) return null;
	return res.json();
}

/** The two things only the person can say about a year. */
export async function updateYear(
	year: number,
	fields: { title?: string; summary?: string },
	fetchFn: FetchFn = fetch
): Promise<void> {
	const res = await fetchFn(`/api/wiki/year/${year}`, {
		method: 'PUT',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(fields)
	});
	if (!res.ok) throw new Error('Could not save that');
}

/** Write the year's first article. Refused for a year with no narrated day. */
export async function writeYearArticle(year: number, fetchFn: FetchFn = fetch): Promise<string> {
	const res = await fetchFn(`/api/wiki/year/${year}/article`, { method: 'POST' });
	if (!res.ok) throw new Error('Could not write that article');
	return res.json();
}

/**
 * How a put-back went. `changed` is false when the page already said that
 * version. `saved` is false when the page says it now but your server
 * couldn't save it yet; it saves it again on its own. `message` is your
 * server's own line about it, which History shows as it is.
 */
export interface RevertOutcome {
	changed: boolean;
	saved: boolean;
	message: string;
}

/**
 * Put an article back to a named version. Adds a version; never rewinds.
 * An older server sends only `message`, and that reads as changed and saved.
 */
export async function revertArticle(
	subjectType: string,
	subjectId: string,
	versionNumber: number,
	fetchFn: FetchFn = fetch
): Promise<RevertOutcome> {
	const res = await fetchFn(`/api/wiki/articles/${subjectType}/${subjectId}/revert`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ version_number: versionNumber })
	});
	if (!res.ok) throw new Error('Could not revert that');
	const body = await res.json().catch(() => null);
	return {
		changed: body?.changed !== false,
		saved: body?.saved !== false,
		message: typeof body?.message === 'string' ? body.message : ''
	};
}

/** How an article is maintained: always, auto, or never. */
/**
 * How the record keeps an article: `auto` (the default — revised when the
 * evidence beneath it moves, at most monthly, never within six hours of your
 * own edit) or `never`.
 *
 * There was an `always` until 2026-09-22. It behaved identically to `auto`,
 * because the only reader tests `<> 'never'` — see migration 0034.
 */
export type Maintenance = 'auto' | 'never';

export async function setArticleMaintenance(
	subjectType: string,
	subjectId: string,
	maintenance: Maintenance,
	fetchFn: FetchFn = fetch
): Promise<void> {
	const res = await fetchFn(`/api/wiki/articles/${subjectType}/${subjectId}/maintenance`, {
		method: 'PUT',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ maintenance })
	});
	if (!res.ok) throw new Error('Could not change that');
}

export async function listPeople(fetchFn: FetchFn = fetch): Promise<WikiPersonListItem[]> {
	const res = await fetchFn("/api/wiki/people");
	if (!res.ok) return [];
	return res.json();
}

export async function updatePerson(
	id: string,
	data: Partial<WikiPersonApi>,
	fetchFn: FetchFn = fetch
): Promise<WikiPersonApi | null> {
	const res = await fetchFn(`/api/wiki/person/${id}`, {
		method: "PUT",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(data),
	});
	if (!res.ok) return null;
	return res.json();
}

// --- Place ---

export async function getPlaceById(
	id: string,
	fetchFn: FetchFn = fetch
): Promise<WikiPlaceApi | null> {
	const res = await fetchFn(`/api/wiki/place/${encodeURIComponent(id)}`);
	if (!res.ok) return null;
	return res.json();
}

export async function listPlaces(fetchFn: FetchFn = fetch): Promise<WikiPlaceListItem[]> {
	const res = await fetchFn("/api/wiki/places");
	if (!res.ok) return [];
	return res.json();
}

export async function updatePlace(
	id: string,
	data: Partial<WikiPlaceApi>,
	fetchFn: FetchFn = fetch
): Promise<WikiPlaceApi | null> {
	const res = await fetchFn(`/api/wiki/place/${id}`, {
		method: "PUT",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(data),
	});
	if (!res.ok) return null;
	return res.json();
}

/** A named place close enough to be the same one, offered as "Same as…". */
export interface NearbyPlace {
	id: string;
	name: string;
	meters: number;
}

/** The named places within 150 m of a place, nearest first. */
export async function getNearbyPlaces(id: string, fetchFn: FetchFn = fetch): Promise<NearbyPlace[]> {
	const res = await fetchFn(`/api/entities/places/${encodeURIComponent(id)}/nearby`);
	if (!res.ok) return [];
	return res.json();
}

/** Fold place `id` into `into`: its visits, pins and notes move, and `id` is
 *  deleted. Returns false when the server refused. */
export async function mergePlace(id: string, into: string, fetchFn: FetchFn = fetch): Promise<boolean> {
	const res = await fetchFn(`/api/entities/places/${encodeURIComponent(id)}/merge`, {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({ into }),
	});
	return res.ok;
}

// --- Organization ---

export async function getOrganizationById(
	id: string,
	fetchFn: FetchFn = fetch
): Promise<WikiOrganizationApi | null> {
	const res = await fetchFn(`/api/wiki/organization/${encodeURIComponent(id)}`);
	if (!res.ok) return null;
	return res.json();
}

export async function listOrganizations(
	fetchFn: FetchFn = fetch
): Promise<WikiOrganizationListItem[]> {
	const res = await fetchFn("/api/wiki/organizations");
	if (!res.ok) return [];
	return res.json();
}

export async function updateOrganization(
	id: string,
	data: Partial<WikiOrganizationApi>,
	fetchFn: FetchFn = fetch
): Promise<WikiOrganizationApi | null> {
	const res = await fetchFn(`/api/wiki/organization/${id}`, {
		method: "PUT",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(data),
	});
	if (!res.ok) return null;
	return res.json();
}

// --- Telos ---



// --- Act ---



// --- Chapter ---



// --- Day ---

export async function getDayByDate(
	date: string,
	fetchFn: FetchFn = fetch
): Promise<WikiDayApi | null> {
	const res = await fetchFn(`/api/wiki/day/${encodeURIComponent(date)}`);
	if (!res.ok) return null;
	return res.json();
}

/**
 * Where a day's Rewrite this page stands. `state` is your server's memory of
 * the day's latest rewrite since it started (`idle` when it has none);
 * `has_page` and `has_your_edits` are read from the page on every ask.
 */
export interface DayRewriteStatus {
	state: 'idle' | 'running' | 'done' | 'failed';
	/**
	 * Why a rewrite failed: not_enough, edited_while_writing, thin_draft,
	 * not_saved, billing, failed, interrupted, not_over, no_page or
	 * needs_consent.
	 */
	code?: string | null;
	message?: string | null;
	started_at?: string | null;
	finished_at?: string | null;
	/** The version that holds the page as it was before the rewrite. */
	before_version?: number | null;
	/** The version the rewrite wrote. */
	after_version?: number | null;
	has_page: boolean;
	/**
	 * The page may hold your own words: you edited it, put a version back, or
	 * turned its upkeep off. A rewrite replaces them only when you say so.
	 */
	has_your_edits: boolean;
}

/**
 * How long a status ask may take. A connection that died while a phone slept
 * can leave a request unsettled for good, and the day page keeps one ask out
 * at a time, so without a deadline it would never ask again.
 */
export const DAY_REWRITE_ASK_MS = 15_000;

/** Where a day's rewrite stands. Rejects, like any failed ask, once `DAY_REWRITE_ASK_MS` passes. */
export function getDayRewrite(date: string): Promise<DayRewriteStatus> {
	return request<DayRewriteStatus>(`/wiki/day/${encodeURIComponent(date)}/rewrite`, {
		signal: deadline(DAY_REWRITE_ASK_MS)
	});
}

/**
 * Ask your server to write a past day's page again. It answers at once and
 * writes in the background; `getDayRewrite` says how it went. A refusal
 * throws an `ApiError` whose message is the code: not_over (422), no_page
 * (404), or one of three 409s: needs_consent, rewrite_in_progress (a rewrite
 * of the day is running), or busy (another writer holds the day).
 * `replaceEdits` is your consent to replace changes you made to the page.
 */
export function rewriteDay(
	date: string,
	{ replaceEdits }: { replaceEdits: boolean }
): Promise<{ state: 'running'; started_at: string }> {
	return apiSend('POST', `/wiki/day/${encodeURIComponent(date)}/rewrite`, {
		replace_edits: replaceEdits
	});
}


export async function listDays(
	startDate?: string,
	endDate?: string,
	fetchFn: FetchFn = fetch
): Promise<WikiDayApi[]> {
	const params = new URLSearchParams();
	if (startDate) params.set("start_date", startDate);
	if (endDate) params.set("end_date", endDate);
	const query = params.toString() ? `?${params}` : "";
	const res = await fetchFn(`/api/wiki/days${query}`);
	if (!res.ok) return [];
	return res.json();
}

/** One heatmap cell: how much recorded life a day holds. */
export interface DayActivityApi {
	date: string;
	event_count: number;
	narrated: boolean;
}

export async function listDayActivity(
	startDate: string,
	endDate: string,
	fetchFn: FetchFn = fetch
): Promise<DayActivityApi[]> {
	const res = await fetchFn(
		`/api/wiki/activity?start_date=${startDate}&end_date=${endDate}`
	);
	if (!res.ok) return [];
	return res.json();
}

/** One raw record linked to an entity via refs — the entity page's evidence feed. */
export interface EntityRecordApi {
	source_type: string;
	id: string;
	timestamp: string;
	label: string;
	preview: string | null;
	role: string | null;
	continuous: boolean;
}

export interface EntityRecordsPageApi {
	items: EntityRecordApi[];
	total: number;
}

/** Per-raw-source_type counts across ALL of an entity's records. */
export interface EntityRecordFacetApi {
	source_type: string;
	count: number;
	continuous: boolean;
}

export async function getEntityRecordsPage(
	entityId: string,
	opts: {
		offset: number;
		limit: number;
		search?: string;
		/** Raw source_types to include; empty = all. */
		types?: string[];
		dir?: "asc" | "desc";
	},
	fetchFn: FetchFn = fetch
): Promise<EntityRecordsPageApi> {
	const params = new URLSearchParams();
	params.set("offset", String(opts.offset));
	params.set("limit", String(opts.limit));
	if (opts.search) params.set("search", opts.search);
	if (opts.types?.length) params.set("types", opts.types.join(","));
	if (opts.dir) params.set("dir", opts.dir);
	// no-store: this endpoint predates some deployed boxes, whose SPA fallback
	// used to answer unknown /api paths with cacheable HTML — never let a
	// poisoned cache entry shadow the real data.
	const res = await fetchFn(
		`/api/wiki/entity/${encodeURIComponent(entityId)}/records?${params}`,
		{ cache: "no-store" }
	);
	if (!res.ok) {
		// A server error is not an empty history — let the caller show it.
		throw new Error(`Failed to load entity records (${res.status})`);
	}
	return res.json();
}

export async function getEntityRecordFacets(
	entityId: string,
	fetchFn: FetchFn = fetch
): Promise<EntityRecordFacetApi[]> {
	const res = await fetchFn(
		`/api/wiki/entity/${encodeURIComponent(entityId)}/records/facets`,
		{ cache: "no-store" }
	);
	if (!res.ok) return [];
	return res.json();
}

/** A past year's entry sharing today's month and day. */
export interface OnThisDayApi {
	date: string;
	/** The day article's opening paragraph. */
	lede: string | null;
	narrated: boolean;
	event_count: number;
}

export async function listOnThisDay(
	date?: string,
	fetchFn: FetchFn = fetch
): Promise<OnThisDayApi[]> {
	const query = date ? `?date=${encodeURIComponent(date)}` : "";
	const res = await fetchFn(`/api/wiki/on-this-day${query}`);
	if (!res.ok) return [];
	return res.json();
}

// --- Narrative identity ---

/** The "In your own words" document, read from its page. Read-only here —
 *  editing happens on the page itself (`page_id`); the writable abridged copy
 *  this used to front was retired 2026-09-01. */
export interface NarrativeIdentityApi {
	content: string;
	updated_at: string;
	/** The document's page; empty until the interview has been written up. */
	page_id: string;
}

export async function getNarrativeIdentity(
	fetchFn: FetchFn = fetch
): Promise<NarrativeIdentityApi | null> {
	const res = await fetchFn(`/api/wiki/narrative-identity`);
	if (!res.ok) return null;
	return res.json();
}

/** One chapter of the life — authored in the interview, never inferred. */
export interface ChapterApi {
	id: string;
	kind: "chapter" | "unknown";
	title: string | null;
	started_at: string;
	ended_at: string | null;
	is_current: boolean;
	changepoint: string | null;
	summary: string | null;
}

export async function getChapters(fetchFn: FetchFn = fetch): Promise<ChapterApi[]> {
	const res = await fetchFn(`/api/wiki/chapters`);
	if (!res.ok) return [];
	const body = await res.json();
	return body.chapters ?? [];
}

// ============================================================================
// Temporal Event Types
// ============================================================================

export interface TemporalEventApi {
	id: string;
	day_id: string;
	start_time: string;
	end_time: string;
	auto_label: string | null;
	auto_location: string | null;
	user_label: string | null;
	user_location: string | null;
	user_notes: string | null;
	source_ontologies: string[] | null;
	is_unknown: boolean | null;
	is_transit: boolean | null;
	is_user_added: boolean | null;
	is_user_edited: boolean | null;
	// Dayline fields
	novelty_z: number | null;
	topics: string[] | null;
	event_summary: string | null;
	agent_action: string | null;
	is_sleep: boolean | null;
	user_hidden: boolean | null;
	// Autonomic scoring
	avg_hr: number | null;
	autonomic_z: number | null;
	hr_z: number | null;
	// Entity/topic novelty
	entities: string[] | null;
	/** `{entity_id: name}` — resolved server-side, because nothing on this
	 *  side can turn `person_a1b2c3d4` into a person. */
	entity_names: Record<string, string> | null;
	topic_novelty: Record<string, number> | null;
	entity_novelty: Record<string, number> | null;
	entity_timestamps: Record<string, string> | null;
	created_at: string;
	updated_at: string;
}

export interface CreateTemporalEventRequest {
	day_id: string;
	start_time: string;
	end_time: string;
	auto_label?: string;
	auto_location?: string;
	user_label?: string;
	user_location?: string;
	user_notes?: string;
	source_ontologies?: unknown;
	is_unknown?: boolean;
	is_transit?: boolean;
	is_user_added?: boolean;
	is_user_edited?: boolean;
}

export interface UpdateTemporalEventRequest {
	start_time?: string;
	end_time?: string;
	user_label?: string;
	user_location?: string;
	user_notes?: string;
	is_user_edited?: boolean;
}

// ============================================================================
// Temporal Event API Functions
// ============================================================================

/**
 * Get events for a specific day by date.
 * @param date - The date in YYYY-MM-DD format
 */
export async function getDayEvents(
	date: string,
	fetchFn: FetchFn = fetch
): Promise<TemporalEventApi[]> {
	const res = await fetchFn(`/api/wiki/day/${encodeURIComponent(date)}/events`);
	if (!res.ok) return [];
	return res.json();
}

/**
 * Create a temporal event.
 */
export async function createTemporalEvent(
	data: CreateTemporalEventRequest,
	fetchFn: FetchFn = fetch
): Promise<TemporalEventApi | null> {
	const res = await fetchFn("/api/wiki/events", {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(data),
	});
	if (!res.ok) return null;
	return res.json();
}

/**
 * Update a temporal event.
 */
export async function updateTemporalEvent(
	eventId: string,
	data: UpdateTemporalEventRequest,
	fetchFn: FetchFn = fetch
): Promise<TemporalEventApi | null> {
	const res = await fetchFn(`/api/wiki/events/${eventId}`, {
		method: "PUT",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(data),
	});
	if (!res.ok) return null;
	return res.json();
}

/**
 * Delete a temporal event.
 */
export async function deleteTemporalEvent(
	eventId: string,
	fetchFn: FetchFn = fetch
): Promise<boolean> {
	const res = await fetchFn(`/api/wiki/events/${eventId}`, {
		method: "DELETE",
	});
	return res.ok;
}

// ============================================================================
// Day Sources Types (Ontology records for a day)
// ============================================================================

export interface DaySourceApi {
	source_type: string;
	id: string;
	timestamp: string;
	label: string;
	preview: string | null;
	/** High-frequency measurement streams (heart rate, steps, HRV). Hidden by
	 *  default on the day page behind a filter, since a day holds thousands. */
	continuous: boolean;
}

/**
 * Get all ontology data sources for a specific date.
 * Returns calendar events, emails, location visits, workouts, etc.
 * @param date - The date in YYYY-MM-DD format
 */
export async function getDaySources(
	date: string,
	fetchFn: FetchFn = fetch
): Promise<DaySourceApi[]> {
	// Pass the viewing device's IANA zone so an in-progress "today" is anchored to
	// where the owner currently is (see agents/record/timezone-model.md). Harmless for past
	// days — the server prefers the day's locked start_timezone.
	const tz = Intl.DateTimeFormat().resolvedOptions().timeZone;
	const qs = tz ? `?tz=${encodeURIComponent(tz)}` : "";
	const res = await fetchFn(`/api/wiki/day/${encodeURIComponent(date)}/sources${qs}`);
	if (!res.ok) return [];
	return res.json();
}

// ============================================================================
// Timeline Day View (chunked location/transit/missing-data stream)
// ============================================================================

export interface TimelineDayLocationChunk {
	type: "location";
	start_time: string;
	end_time: string;
	place_name: string | null;
	latitude: number;
	longitude: number;
	place_id: string | null;
	duration_minutes: number | null;
	place_category: string | null;
}

export type TimelineDayChunk =
	| TimelineDayLocationChunk
	| { type: "transit" }
	| { type: "missing_data" };

export interface TimelineDayPoint {
	latitude: number;
	longitude: number;
	timestamp: string;
	/** The phone's error radius, metres. Past ~100 m it is a cell tower's guess, not GPS. */
	horizontal_accuracy: number | null;
	/** The phone's reported speed, m/s; null when it reported none. */
	speed: number | null;
}

export interface TimelineDayView {
	date: string;
	chunks: TimelineDayChunk[];
	points: TimelineDayPoint[];
	/** The last raw GPS point before the day starts; null when there is none. */
	last_point_before: TimelineDayPoint | null;
}

// ============================================================================
// Day Chats (in-app Virtues + external AI conversations)
// ============================================================================

export interface DayChatApi {
	id: string;
	source: "virtues" | "external";
	provider: string | null;
	title: string;
	message_count: number;
	started_at: string;
}

/**
 * Get all AI chats (in-app Virtues + external imported) that started on a day.
 * In-app chats are navigable; external chats are display-only.
 * @param date - The date in YYYY-MM-DD format
 */
/** The dateline above a day's title. Deterministic facts only; absent is null. */
export interface DayFactsApi {
	temperature_high_c: number | null;
	temperature_low_c: number | null;
	/** Minutes the microphone was recording, silence included. */
	recorded_minutes: number;
	/** Recorded stretches, merged, as [start, end] ISO instants. */
	coverage: [string, string][];
	/** How long the day was in minutes; a server older than this field leaves it out. */
	day_minutes?: number;
}

export async function getDayFacts(date: string, fetchFn: FetchFn = fetch): Promise<DayFactsApi | null> {
	const res = await fetchFn(`/api/wiki/day/${encodeURIComponent(date)}/facts`);
	if (!res.ok) return null;
	return res.json();
}

/** A person as of one day: the facts behind a day page's gloss card. */
export interface PersonGlossApi {
	id: string;
	date: string;
	first_message_on: string | null;
	/** Whether that first message was in a group thread. */
	first_message_in_group: boolean | null;
	last_message_before_on: string | null;
	/** The days just before `date` that `days_in_window` counts over. */
	window_days: number;
	days_in_window: number;
	direct_messages_in_window: number;
	group_messages_in_window: number;
}

export function getPersonGloss(id: string, date: string): Promise<PersonGlossApi> {
	return apiGet<PersonGlossApi>(`/wiki/person/${encodeURIComponent(id)}/gloss`, { date });
}

/** One of your numbers on a day page: the day's value and the days before it. */
export interface DayMeasureApi {
	/** `lane:id`, the form a pin is stored in. */
	key: string;
	label: string;
	unit: string;
	kind: 'total' | 'rate';
	/** Null when nothing was collected that day, which is not zero. */
	value: number | null;
	/** The 30 days before, oldest first; null the same way. */
	before: (number | null)[];
}

export interface MeasureListingApi {
	key: string;
	lane: string;
	label: string;
	unit: string;
	kind: 'total' | 'rate';
}

export interface DayMeasuresApi {
	date: string;
	measures: DayMeasureApi[];
	available: MeasureListingApi[];
}

export function getDayMeasures(date: string, keys: string[]): Promise<DayMeasuresApi> {
	return apiGet<DayMeasuresApi>(`/wiki/day/${encodeURIComponent(date)}/measures`, { keys: keys.join(',') });
}

/** A day whose page reads like this one's. */
export interface SimilarDayApi {
	date: string;
	/** The page's first paragraph, markdown. */
	abstract_md: string;
	similarity: number;
}

export async function getSimilarDays(date: string, fetchFn: FetchFn = fetch): Promise<SimilarDayApi[]> {
	const res = await fetchFn(`/api/wiki/day/${encodeURIComponent(date)}/similar`);
	if (!res.ok) return [];
	return res.json();
}

export async function getDayChats(
	date: string,
	fetchFn: FetchFn = fetch,
): Promise<DayChatApi[]> {
	const res = await fetchFn(`/api/wiki/day/${encodeURIComponent(date)}/chats`);
	if (!res.ok) return [];
	return res.json();
}

/**
 * Get the chunked timeline view for a day (location stops, transit, gaps).
 * @param date - The date in YYYY-MM-DD format
 */
export async function getDayTimeline(
	date: string,
	fetchFn: FetchFn = fetch,
): Promise<TimelineDayView | null> {
	const res = await fetchFn(`/api/timeline/day/${encodeURIComponent(date)}`);
	if (!res.ok) return null;
	return res.json();
}

// ============================================================================
// Unnamed-place backlog
// ============================================================================

export interface UnnamedPlace {
	id: string;
	name: string;
	ref_count: number;
	latitude: number | null;
	longitude: number | null;
}

/** Places visited but never named — the home "name this place" ask. */
export async function getUnnamedPlaces(
	limit = 3,
	fetchFn: FetchFn = fetch,
): Promise<UnnamedPlace[]> {
	const res = await fetchFn(`/api/places/unnamed?limit=${limit}`);
	if (!res.ok) return [];
	return res.json();
}

