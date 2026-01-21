import type { PageLoad } from "./$types";
import {
	resolveSlug,
	getPersonBySlug,
	getPlaceBySlug,
	getOrganizationBySlug,
	getThingBySlug,
	getDayByDate,
	getActBySlug,
	getChapterBySlug,
	getTelosBySlug,
	type WikiPersonApi,
	type WikiPlaceApi,
	type WikiOrganizationApi,
	type WikiThingApi,
	type WikiDayApi,
	type WikiActApi,
	type WikiChapterApi,
	type WikiTelosApi,
} from "$lib/wiki/api";

export type WikiPageData =
	| { type: "person"; data: WikiPersonApi }
	| { type: "place"; data: WikiPlaceApi }
	| { type: "organization"; data: WikiOrganizationApi }
	| { type: "thing"; data: WikiThingApi }
	| { type: "day"; data: WikiDayApi }
	| { type: "act"; data: WikiActApi }
	| { type: "chapter"; data: WikiChapterApi }
	| { type: "telos"; data: WikiTelosApi }
	| { type: "not_found"; slug: string };

export const load: PageLoad = async ({ params, fetch }) => {
	const { slug } = params;

	// Check if it's a date (day page)
	const dateRegex = /^\d{4}-\d{2}-\d{2}$/;
	if (dateRegex.test(slug)) {
		const day = await getDayByDate(slug, fetch);
		if (day) {
			return { page: { type: "day", data: day } satisfies WikiPageData };
		}
	}

	// Try to resolve the slug to find the entity type
	const resolution = await resolveSlug(slug, fetch);

	if (!resolution) {
		return { page: { type: "not_found", slug } satisfies WikiPageData };
	}

	// Fetch the full entity based on type
	switch (resolution.entity_type) {
		case "person": {
			const person = await getPersonBySlug(slug, fetch);
			if (person) {
				return { page: { type: "person", data: person } satisfies WikiPageData };
			}
			break;
		}
		case "place": {
			const place = await getPlaceBySlug(slug, fetch);
			if (place) {
				return { page: { type: "place", data: place } satisfies WikiPageData };
			}
			break;
		}
		case "organization": {
			const org = await getOrganizationBySlug(slug, fetch);
			if (org) {
				return { page: { type: "organization", data: org } satisfies WikiPageData };
			}
			break;
		}
		case "thing": {
			const thing = await getThingBySlug(slug, fetch);
			if (thing) {
				return { page: { type: "thing", data: thing } satisfies WikiPageData };
			}
			break;
		}
		case "day": {
			const day = await getDayByDate(slug, fetch);
			if (day) {
				return { page: { type: "day", data: day } satisfies WikiPageData };
			}
			break;
		}
		case "act": {
			const act = await getActBySlug(slug, fetch);
			if (act) {
				return { page: { type: "act", data: act } satisfies WikiPageData };
			}
			break;
		}
		case "chapter": {
			const chapter = await getChapterBySlug(slug, fetch);
			if (chapter) {
				return { page: { type: "chapter", data: chapter } satisfies WikiPageData };
			}
			break;
		}
		case "telos": {
			const telos = await getTelosBySlug(slug, fetch);
			if (telos) {
				return { page: { type: "telos", data: telos } satisfies WikiPageData };
			}
			break;
		}
	}

	return { page: { type: "not_found", slug } satisfies WikiPageData };
};
