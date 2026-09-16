/**
 * Where a subject lives, from its id alone.
 *
 * Ids carry their type as a prefix (`person_ab12`, `day_2026-03-03`), and the
 * tab registry routes each type at its own path — `/person/{id}`, never
 * `/wiki/{id}`. Thirteen call sites wrote `/wiki/{id}` anyway. That path
 * matches neither `/wiki` nor the wiki-section pattern, so it fell through the
 * registry to the chat fallback: **clicking an entity opened a new chat.**
 * Most of those sites sat in sections that never render, which is why it
 * survived; the day page's entity list is not one of them.
 *
 * One function so the next component cannot get it wrong, and so a section
 * that starts rendering later does not resurrect the bug.
 *
 * ## This table mirrors the server's
 *
 * `SUBJECT_ROUTES` is the client half of `virtues-core/src/api/subjects.rs`,
 * which is where a subject kind's facts live. The Rust test
 * `the_client_route_table_matches_the_registry` reads THIS FILE and fails if
 * the two disagree, so a rung added on one side cannot quietly go missing on
 * the other — which is how chapters came to have articles, a room and no way
 * to be linked.
 *
 * A prefix mapped to `null` is a subject with no page of its own. That is a
 * fact about the product, not an oversight, and `null` is the honest answer:
 * an href to a route that does not exist renders as a link that goes nowhere.
 */
const SUBJECT_ROUTES: Record<string, string | null> = {
	person: 'person',
	place: 'place',
	// `org`, not `organization`: the id prefix and the schema's word differ for
	// exactly this one kind, and this table is keyed on the prefix.
	org: 'org',
	day: 'day',
	year: 'year',
	chapter: null,
	story: null,
	nar: null
};

/** Namespaces that are not wiki subjects but are still addressable. */
const OTHER_ROUTES: Record<string, string> = {
	page: 'page',
	chat: 'chat'
};

/** The route for a subject id, or `null` if nothing can display it. */
export function subjectHref(id: string | null | undefined): string | null {
	if (!id) return null;
	const cut = id.indexOf('_');
	if (cut < 1) return null;
	const prefix = id.slice(0, cut);
	if (prefix in SUBJECT_ROUTES) {
		const route = SUBJECT_ROUTES[prefix];
		return route ? `/${route}/${id}` : null;
	}
	const other = OTHER_ROUTES[prefix];
	return other ? `/${other}/${id}` : null;
}
