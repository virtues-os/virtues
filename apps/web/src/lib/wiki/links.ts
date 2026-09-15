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
 */

/** The route for a subject id, or `null` if nothing can display it. */
export function subjectHref(id: string | null | undefined): string | null {
	if (!id) return null;
	const prefix = id.slice(0, id.indexOf("_"));
	switch (prefix) {
		case "person":
			return `/person/${id}`;
		case "place":
			return `/place/${id}`;
		case "org":
			return `/org/${id}`;
		case "day":
			return `/day/${id}`;
		case "year":
			return `/year/${id}`;
		case "page":
			return `/page/${id}`;
		case "chat":
			return `/chat/${id}`;
		default:
			// `chapter_` lands here on purpose: chapters have no room yet, and a
			// link to a route that does not exist is the bug this file fixes.
			return null;
	}
}
