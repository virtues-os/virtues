/**
 * Wiki API Client
 *
 * Fetches wiki pages from the backend API.
 * Backend wiki pages are views of entities/narratives.
 */

// ============================================================================
// API Response Types (match Rust backend types)
// ============================================================================

export interface WikiPersonApi {
	id: string;
	canonical_name: string;
	content: string | null;
	article: string | null;
	article_updated_at: string | null;
	picture: string | null;
	cover_image: string | null;
	emails: string[];
	phones: string[];
	birthday: string | null; // ISO date string
	instagram: string | null;
	facebook: string | null;
	linkedin: string | null;
	x: string | null;
	relationship_category: string | null;
	nickname: string | null;
	notes: string | null;
	first_interaction: string | null;
	last_interaction: string | null;
	interaction_count: number | null;
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
	visit_count: number | null;
	first_visit: string | null;
	last_visit: string | null;
	created_at: string;
	updated_at: string;
}

export interface WikiOrganizationApi {
	id: string;
	canonical_name: string;
	content: string | null;
	article: string | null;
	article_updated_at: string | null;
	cover_image: string | null;
	organization_type: string | null;
	relationship_type: string | null;
	role_title: string | null;
	start_date: string | null;
	end_date: string | null;
	interaction_count: number | null;
	first_interaction: string | null;
	last_interaction: string | null;
	created_at: string;
	updated_at: string;
}


export interface WikiDayApi {
	id: string;
	date: string; // ISO date string
	start_timezone: string | null;
	autobiography: string | null;
	autobiography_sections: Array<{ id: string; heading: string; content: string; authored_by: string; last_edited_at: string }> | null;
	epigraph: string | null;
	last_edited_by: string | null;
	cover_image: string | null;
	act_id: string | null;
	chapter_id: string | null;
	morning_baseline: number | null;
	battery_curve: string | null;
	data_quality: {
		coverage: { who: number; whom: number; what: number; when: number; where: number; why: number; how: number };
		overall: number;
		note: string;
	} | null;
	new_entity_count: number;
	new_topic_count: number;
	readiness_score: number | null;
	readiness_details: { hrv: number; rhr: number; sleep_duration: number; deep_rem: number; consistency: number } | null;
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

/**
 * A story: a themed article that spans time. Unlike an act, it has no required
 * dates and no place in an ordered spine — "the story of my wedding" overlaps
 * whatever else was going on.
 */
export interface WikiStoryApi {
	id: string;
	title: string;
	subtitle: string | null;
	content: string | null;
	cover_image: string | null;
	start_date: string | null;
	end_date: string | null;
	sort_order: number;
	themes: string[] | null;
	created_at: string;
	updated_at: string;
}

export interface WikiActApi {
	id: string;
	title: string;
	subtitle: string | null;
	description: string | null;
	content: string | null;
	cover_image: string | null;
	location: string | null;
	start_date: string;
	end_date: string | null;
	sort_order: number;
	telos_id: string | null;
	themes: string[] | null;
	created_at: string;
	updated_at: string;
}

export interface WikiChapterApi {
	id: string;
	title: string;
	subtitle: string | null;
	description: string | null;
	content: string | null;
	cover_image: string | null;
	start_date: string;
	end_date: string | null;
	sort_order: number;
	act_id: string | null;
	themes: string[] | null;
	created_at: string;
	updated_at: string;
}

export interface WikiTelosApi {
	id: string;
	title: string;
	description: string | null;
	content: string | null;
	cover_image: string | null;
	is_active: boolean | null;
	created_at: string;
	updated_at: string;
}

export interface IdResolution {
	entity_type: string;
	id: string;
}

// ============================================================================
// List Item Types
// ============================================================================

export interface WikiPersonListItem {
	id: string;
	canonical_name: string;
	picture: string | null;
	relationship_category: string | null;
	last_interaction: string | null;
}

export interface WikiPlaceListItem {
	id: string;
	name: string;
	category: string | null;
	address: string | null;
	visit_count: number | null;
}

export interface WikiOrganizationListItem {
	id: string;
	canonical_name: string;
	organization_type: string | null;
	relationship_type: string | null;
}


// ============================================================================
// API Functions
// ============================================================================

type FetchFn = typeof fetch;

/**
 * Parse an entity ID to extract the type.
 * IDs follow the format: {type}_{hash} (e.g., person_abc123)
 */
export function parseEntityId(id: string): IdResolution | null {
	const parts = id.split('_');
	if (parts.length < 2) return null;
	return {
		entity_type: parts[0],
		id: id
	};
}

// --- Person ---

export async function getPersonById(
	id: string,
	fetchFn: FetchFn = fetch
): Promise<WikiPersonApi | null> {
	const res = await fetchFn(`/api/wiki/person/${encodeURIComponent(id)}`);
	if (!res.ok) return null;
	return res.json();
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

export async function getActiveTelos(fetchFn: FetchFn = fetch): Promise<WikiTelosApi | null> {
	const res = await fetchFn("/api/wiki/telos/active");
	if (!res.ok) return null;
	return res.json();
}

export async function getTelosById(
	id: string,
	fetchFn: FetchFn = fetch
): Promise<WikiTelosApi | null> {
	const res = await fetchFn(`/api/wiki/telos/${encodeURIComponent(id)}`);
	if (!res.ok) return null;
	return res.json();
}

// --- Act ---

export async function getActById(
	id: string,
	fetchFn: FetchFn = fetch
): Promise<WikiActApi | null> {
	const res = await fetchFn(`/api/wiki/act/${encodeURIComponent(id)}`);
	if (!res.ok) return null;
	return res.json();
}

export async function listActs(fetchFn: FetchFn = fetch): Promise<WikiActApi[]> {
	const res = await fetchFn("/api/wiki/acts");
	if (!res.ok) return [];
	return res.json();
}

// --- Story ---

export async function getStoryById(
	id: string,
	fetchFn: FetchFn = fetch
): Promise<WikiStoryApi | null> {
	const res = await fetchFn(`/api/wiki/story/${encodeURIComponent(id)}`);
	if (!res.ok) return null;
	return res.json();
}

export async function listStories(fetchFn: FetchFn = fetch): Promise<WikiStoryApi[]> {
	const res = await fetchFn("/api/wiki/stories");
	if (!res.ok) return [];
	return res.json();
}

// --- Chapter ---

export async function getChapterById(
	id: string,
	fetchFn: FetchFn = fetch
): Promise<WikiChapterApi | null> {
	const res = await fetchFn(`/api/wiki/chapter/${encodeURIComponent(id)}`);
	if (!res.ok) return null;
	return res.json();
}

export async function listChaptersForAct(
	actId: string,
	fetchFn: FetchFn = fetch
): Promise<WikiChapterApi[]> {
	const res = await fetchFn(`/api/wiki/act/${actId}/chapters`);
	if (!res.ok) return [];
	return res.json();
}

// --- Day ---

export async function getDayByDate(
	date: string,
	fetchFn: FetchFn = fetch
): Promise<WikiDayApi | null> {
	const res = await fetchFn(`/api/wiki/day/${encodeURIComponent(date)}`);
	if (!res.ok) return null;
	return res.json();
}

export async function updateDay(
	date: string,
	data: Partial<WikiDayApi>,
	fetchFn: FetchFn = fetch
): Promise<WikiDayApi | null> {
	const res = await fetchFn(`/api/wiki/day/${encodeURIComponent(date)}`, {
		method: "PUT",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(data),
	});
	if (!res.ok) return null;
	return res.json();
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
	epigraph: string | null;
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

export interface NarrativeIdentityApi {
	id: string;
	content: string;
	created_at: string;
	updated_at: string;
}

export async function getNarrativeIdentity(
	fetchFn: FetchFn = fetch
): Promise<NarrativeIdentityApi | null> {
	const res = await fetchFn(`/api/wiki/narrative-identity`);
	if (!res.ok) return null;
	return res.json();
}

export async function updateNarrativeIdentity(
	content: string,
	fetchFn: FetchFn = fetch
): Promise<NarrativeIdentityApi | null> {
	const res = await fetchFn(`/api/wiki/narrative-identity`, {
		method: "PUT",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({ content }),
	});
	if (!res.ok) return null;
	return res.json();
}

// ============================================================================
// Citation Types
// ============================================================================

export interface CitationApi {
	id: string;
	source_type: string;
	source_id: string;
	target_table: string;
	target_id: string;
	citation_index: number;
	label: string | null;
	preview: string | null;
	is_hidden: boolean | null;
	added_by: string | null;
	created_at: string;
	updated_at: string;
}

export interface CreateCitationRequest {
	source_type?: string; // Set from path in handler
	source_id?: string; // Set from path in handler
	target_table: string;
	target_id: string;
	citation_index: number;
	label?: string;
	preview?: string;
	is_hidden?: boolean;
	added_by?: string;
}

export interface UpdateCitationRequest {
	label?: string;
	preview?: string;
	is_hidden?: boolean;
	citation_index?: number;
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
	user_created: boolean | null;
	// Autonomic scoring
	avg_hr: number | null;
	autonomic_z: number | null;
	hr_z: number | null;
	hrv_z: number | null;
	// Entity/topic novelty
	entities: string[] | null;
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
// Citation API Functions
// ============================================================================

/**
 * Get citations for a wiki page.
 * @param sourceType - The type of wiki page (person, place, organization, telos, act, chapter, day)
 * @param sourceId - The UUID of the wiki page
 */
export async function getCitations(
	sourceType: string,
	sourceId: string,
	fetchFn: FetchFn = fetch
): Promise<CitationApi[]> {
	const res = await fetchFn(`/api/wiki/${sourceType}/${sourceId}/citations`);
	if (!res.ok) return [];
	return res.json();
}

/**
 * Create a citation for a wiki page.
 */
export async function createCitation(
	sourceType: string,
	sourceId: string,
	data: CreateCitationRequest,
	fetchFn: FetchFn = fetch
): Promise<CitationApi | null> {
	const res = await fetchFn(`/api/wiki/${sourceType}/${sourceId}/citations`, {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(data),
	});
	if (!res.ok) return null;
	return res.json();
}

/**
 * Update a citation.
 */
export async function updateCitation(
	citationId: string,
	data: UpdateCitationRequest,
	fetchFn: FetchFn = fetch
): Promise<CitationApi | null> {
	const res = await fetchFn(`/api/wiki/citations/${citationId}`, {
		method: "PUT",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(data),
	});
	if (!res.ok) return null;
	return res.json();
}

/**
 * Delete a citation.
 */
export async function deleteCitation(
	citationId: string,
	fetchFn: FetchFn = fetch
): Promise<boolean> {
	const res = await fetchFn(`/api/wiki/citations/${citationId}`, {
		method: "DELETE",
	});
	return res.ok;
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
	// where the owner currently is (see docs/timezone-model.md). Harmless for past
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
}

export interface TimelineDayView {
	date: string;
	chunks: TimelineDayChunk[];
	points: TimelineDayPoint[];
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
// Today Streams — the three raw record streams, as spans, before synthesis
// ============================================================================

export interface TodayLocationSpan {
	id: string;
	start_time: string;
	end_time: string;
	place_name: string | null;
	place_category: string | null;
	duration_minutes: number | null;
}

export interface TodayCalendarSpan {
	id: string;
	start_time: string;
	end_time: string;
	title: string;
	is_all_day: boolean;
	is_sacred: boolean;
	location_name: string | null;
	calendar_name: string | null;
}

export interface TodayAudioSpan {
	id: string;
	start_time: string;
	end_time: string;
	/** the box flagged this ~5-min chunk as silence */
	is_silent: boolean;
}

export interface TodayStreamsView {
	date: string;
	timezone: string;
	location: TodayLocationSpan[];
	calendar: TodayCalendarSpan[];
	audio: TodayAudioSpan[];
}

/**
 * Get the three raw record streams (location, calendar, audio) for a day, as
 * spans — the "day before synthesis" homepage view. Passes the viewing device's
 * IANA zone so an in-progress "today" is anchored to where the owner is.
 * @param date - The date in YYYY-MM-DD format
 */
export async function getTodayStreams(
	date: string,
	fetchFn: FetchFn = fetch,
): Promise<TodayStreamsView | null> {
	const tz = Intl.DateTimeFormat().resolvedOptions().timeZone;
	const qs = tz ? `?tz=${encodeURIComponent(tz)}` : "";
	const res = await fetchFn(`/api/today/${encodeURIComponent(date)}/streams${qs}`);
	if (!res.ok) return null;
	return res.json();
}

// ============================================================================
// Home-page loops — weather, upcoming calendar, unnamed-place backlog
// ============================================================================

export interface WeatherNow {
	temperature_c: number | null;
	apparent_c: number | null;
	humidity_pct: number | null;
	wind_kph: number | null;
	is_day: boolean | null;
	weather_code: number | null;
	condition: string;
	temp_max_c: number | null;
	temp_min_c: number | null;
	sunrise: string | null;
	sunset: string | null;
	valid_time: string;
}

/** Current weather for the masthead. Null until the weather_sync cron runs. */
export async function getWeatherNow(fetchFn: FetchFn = fetch): Promise<WeatherNow | null> {
	const res = await fetchFn("/api/weather/current");
	if (!res.ok) return null;
	return res.json();
}

export interface UpcomingEvent {
	id: string;
	title: string;
	start_time: string;
	end_time: string;
	is_all_day: boolean;
	location_name: string | null;
	is_sacred: boolean;
}

/** The next few calendar events (holidays/birthdays filtered out). */
export async function getCalendarUpcoming(
	limit = 5,
	fetchFn: FetchFn = fetch,
): Promise<UpcomingEvent[]> {
	const res = await fetchFn(`/api/calendar/upcoming?limit=${limit}`);
	if (!res.ok) return [];
	return res.json();
}

export interface UnnamedPlace {
	id: string;
	name: string;
	visit_count: number;
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

