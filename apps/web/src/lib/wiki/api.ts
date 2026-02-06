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
	end_timezone: string | null;
	autobiography: string | null;
	autobiography_sections: unknown | null;
	last_edited_by: string | null;
	cover_image: string | null;
	act_id: string | null;
	chapter_id: string | null;
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
	source_ontologies: unknown | null;
	is_unknown: boolean | null;
	is_transit: boolean | null;
	is_user_added: boolean | null;
	is_user_edited: boolean | null;
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
 * @param sourceType - The type of wiki page (person, place, organization, thing, telos, act, chapter, day)
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

/**
 * Delete all auto-generated events for a day (for regeneration).
 * @param dayId - The UUID of the day
 */
export async function deleteAutoEventsForDay(
	dayId: string,
	fetchFn: FetchFn = fetch
): Promise<number> {
	const res = await fetchFn(`/api/wiki/day/${dayId}/auto-events`, {
		method: "DELETE",
	});
	if (!res.ok) return 0;
	const data = await res.json();
	return data.deleted ?? 0;
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
	const res = await fetchFn(`/api/wiki/day/${encodeURIComponent(date)}/sources`);
	if (!res.ok) return [];
	return res.json();
}
