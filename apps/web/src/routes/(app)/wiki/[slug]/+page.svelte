<script lang="ts">
	import { page } from "$app/stores";
	import { getPageBySlug, getOrCreateDayPage, getOrCreateYearPage, MOCK_DAY_PAGE } from "$lib/wiki";
	import {
		WikiPage,
		DayPage,
		YearPage,
		PersonPage,
		PlacePage,
		OrganizationPage,
		ThingPage,
	} from "$lib/components/wiki";
	import {
		isDayPage,
		isYearPage,
		isPersonPage,
		isPlacePage,
		isOrganizationPage,
		isThingPage,
	} from "$lib/wiki/types";
	import type { PageData } from "./$types";
	import { apiToPersonPage, apiToPlacePage, apiToOrganizationPage, apiToThingPage, apiToDayPage } from "$lib/wiki/converters";

	interface Props {
		data: PageData;
	}

	let { data }: Props = $props();

	const slug = $derived($page.params.slug ?? "");

	// Convert API data to frontend page type, or fall back to mock data
	const wikiPage = $derived.by(() => {
		const pageData = data.page;

		// If we got API data, convert it
		if (pageData.type === "person") {
			return apiToPersonPage(pageData.data);
		}
		if (pageData.type === "place") {
			return apiToPlacePage(pageData.data);
		}
		if (pageData.type === "organization") {
			return apiToOrganizationPage(pageData.data);
		}
		if (pageData.type === "thing") {
			return apiToThingPage(pageData.data);
		}
		if (pageData.type === "day") {
			const dayPage = apiToDayPage(pageData.data);
			// Fall back to mock data for demo if API returned empty day
			if (!dayPage.autobiography && slug === "2025-12-10") {
				return MOCK_DAY_PAGE;
			}
			return dayPage;
		}

		// Fall back to mock data for development
		if (slug) {
			return getOrCreateDayPage(slug) ?? getOrCreateYearPage(slug) ?? getPageBySlug(slug);
		}
		return undefined;
	});
</script>

{#if wikiPage}
	{#if isDayPage(wikiPage)}
		<DayPage page={wikiPage} />
	{:else if isYearPage(wikiPage)}
		<YearPage page={wikiPage} />
	{:else if isPersonPage(wikiPage)}
		<PersonPage page={wikiPage} />
	{:else if isPlacePage(wikiPage)}
		<PlacePage page={wikiPage} />
	{:else if isOrganizationPage(wikiPage)}
		<OrganizationPage page={wikiPage} />
	{:else if isThingPage(wikiPage)}
		<ThingPage page={wikiPage} />
	{:else}
		<WikiPage page={wikiPage} />
	{/if}
{:else}
	<div class="wiki-not-found">
		<h1>Page not found</h1>
		<p>The page "{slug}" doesn't exist yet.</p>
		<a href="/wiki">← Back to Wiki</a>
	</div>
{/if}

<style>
	.wiki-not-found {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		height: 100%;
		text-align: center;
		padding: 2rem;
	}

	.wiki-not-found h1 {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.5rem;
		font-weight: normal;
		color: var(--color-foreground);
		margin: 0 0 0.5rem 0;
	}

	.wiki-not-found p {
		font-size: 0.9375rem;
		color: var(--color-foreground-muted);
		margin: 0 0 1.5rem 0;
	}

	.wiki-not-found a {
		font-size: 0.875rem;
		color: var(--color-primary);
		text-decoration: none;
	}

	.wiki-not-found a:hover {
		text-decoration: underline;
	}
</style>
