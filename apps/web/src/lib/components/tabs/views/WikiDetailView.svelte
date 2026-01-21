<script lang="ts">
	import type { Tab } from '$lib/stores/windowTabs.svelte';
	import { windowTabs } from '$lib/stores/windowTabs.svelte';
	import { onMount } from 'svelte';
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
	} from '$lib/wiki/api';
	import {
		apiToPersonPage,
		apiToPlacePage,
		apiToOrganizationPage,
		apiToThingPage,
		apiToDayPage,
	} from '$lib/wiki/converters';
	import { getPageBySlug, getOrCreateDayPage, getOrCreateYearPage, MOCK_DAY_PAGE } from '$lib/wiki';
	import {
		WikiPage,
		DayPage,
		YearPage,
		PersonPage,
		PlacePage,
		OrganizationPage,
		ThingPage,
	} from '$lib/components/wiki';
	import {
		isDayPage,
		isYearPage,
		isPersonPage,
		isPlacePage,
		isOrganizationPage,
		isThingPage,
	} from '$lib/wiki/types';
	import type { WikiPage as WikiPageType } from '$lib/wiki/types';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	let loading = $state(true);
	let error = $state<string | null>(null);
	let wikiPage = $state<WikiPageType | undefined>(undefined);

	const slug = $derived(tab.slug || '');

	// Parse a date string (YYYY-MM-DD) as local date to avoid timezone issues
	function parseDateString(dateStr: string): Date {
		const [year, month, day] = dateStr.split('-').map(Number);
		return new Date(year, month - 1, day);
	}

	// Load wiki page data
	async function loadPage() {
		if (!slug) {
			error = 'No slug provided';
			loading = false;
			return;
		}

		loading = true;
		error = null;

		try {
			// Check if it's a date (day page)
			const dateRegex = /^\d{4}-\d{2}-\d{2}$/;
			if (dateRegex.test(slug)) {
				const day = await getDayByDate(slug);
				if (day) {
					const dayPage = apiToDayPage(day);
					// Fall back to mock data for demo if API returned empty day
					if (!dayPage.autobiography && slug === '2025-12-10') {
						wikiPage = MOCK_DAY_PAGE;
					} else {
						wikiPage = dayPage;
					}
					// Update tab label with formatted date
					const date = parseDateString(slug);
					const formatted = date.toLocaleDateString('en-US', {
						weekday: 'short',
						month: 'short',
						day: 'numeric',
					});
					windowTabs.updateTab(tab.id, { label: formatted });
					loading = false;
					return;
				}
				// If day doesn't exist in API, try mock data
				const mockDay = getOrCreateDayPage(slug);
				if (mockDay) {
					wikiPage = mockDay;
					const date = parseDateString(slug);
					const formatted = date.toLocaleDateString('en-US', {
						weekday: 'short',
						month: 'short',
						day: 'numeric',
					});
					windowTabs.updateTab(tab.id, { label: formatted });
					loading = false;
					return;
				}
			}

			// Check if it's a year
			const yearRegex = /^\d{4}$/;
			if (yearRegex.test(slug)) {
				const yearPage = getOrCreateYearPage(slug);
				if (yearPage) {
					wikiPage = yearPage;
					windowTabs.updateTab(tab.id, { label: slug });
					loading = false;
					return;
				}
			}

			// Try to resolve the slug to find the entity type
			const resolution = await resolveSlug(slug);

			if (!resolution) {
				// Fall back to mock data
				const mockPage = getPageBySlug(slug);
				if (mockPage) {
					wikiPage = mockPage;
					windowTabs.updateTab(tab.id, { label: mockPage.title });
					loading = false;
					return;
				}
				error = `Page "${slug}" not found`;
				loading = false;
				return;
			}

			// Fetch the full entity based on type
			switch (resolution.entity_type) {
				case 'person': {
					const person = await getPersonBySlug(slug);
					if (person) {
						wikiPage = apiToPersonPage(person);
						windowTabs.updateTab(tab.id, { label: person.canonical_name });
					}
					break;
				}
				case 'place': {
					const place = await getPlaceBySlug(slug);
					if (place) {
						wikiPage = apiToPlacePage(place);
						windowTabs.updateTab(tab.id, { label: place.name });
					}
					break;
				}
				case 'organization': {
					const org = await getOrganizationBySlug(slug);
					if (org) {
						wikiPage = apiToOrganizationPage(org);
						windowTabs.updateTab(tab.id, { label: org.canonical_name });
					}
					break;
				}
				case 'thing': {
					const thing = await getThingBySlug(slug);
					if (thing) {
						wikiPage = apiToThingPage(thing);
						windowTabs.updateTab(tab.id, { label: thing.canonical_name });
					}
					break;
				}
				case 'day': {
					const day = await getDayByDate(slug);
					if (day) {
						wikiPage = apiToDayPage(day);
						const date = parseDateString(slug);
						const formatted = date.toLocaleDateString('en-US', {
							weekday: 'short',
							month: 'short',
							day: 'numeric',
						});
						windowTabs.updateTab(tab.id, { label: formatted });
					}
					break;
				}
				case 'act': {
					const act = await getActBySlug(slug);
					if (act) {
						// For now, fall back to mock page for acts
						const mockPage = getPageBySlug(slug);
						if (mockPage) {
							wikiPage = mockPage;
							windowTabs.updateTab(tab.id, { label: act.title });
						}
					}
					break;
				}
				case 'chapter': {
					const chapter = await getChapterBySlug(slug);
					if (chapter) {
						// For now, fall back to mock page for chapters
						const mockPage = getPageBySlug(slug);
						if (mockPage) {
							wikiPage = mockPage;
							windowTabs.updateTab(tab.id, { label: chapter.title });
						}
					}
					break;
				}
				case 'telos': {
					const telos = await getTelosBySlug(slug);
					if (telos) {
						// For now, fall back to mock page for telos
						const mockPage = getPageBySlug(slug);
						if (mockPage) {
							wikiPage = mockPage;
							windowTabs.updateTab(tab.id, { label: telos.title });
						}
					}
					break;
				}
			}

			if (!wikiPage) {
				// Final fallback to mock data
				const mockPage = getPageBySlug(slug);
				if (mockPage) {
					wikiPage = mockPage;
					windowTabs.updateTab(tab.id, { label: mockPage.title });
				} else {
					error = `Page "${slug}" not found`;
				}
			}
		} catch (e) {
			console.error('Failed to load wiki page:', e);
			// Fall back to mock data on error
			const mockPage = getPageBySlug(slug) ?? getOrCreateDayPage(slug) ?? getOrCreateYearPage(slug);
			if (mockPage) {
				wikiPage = mockPage;
				windowTabs.updateTab(tab.id, { label: mockPage.title });
			} else {
				error = e instanceof Error ? e.message : 'Failed to load page';
			}
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		loadPage();
	});

	// Reload when slug changes
	$effect(() => {
		if (slug && active) {
			loadPage();
		}
	});
</script>

<div class="wiki-detail-view">
	{#if loading}
		<div class="loading">
			<iconify-icon icon="ri:loader-4-line" class="animate-spin"></iconify-icon>
			<span>Loading...</span>
		</div>
	{:else if error}
		<div class="error">
			<iconify-icon icon="ri:error-warning-line"></iconify-icon>
			<h1>Page not found</h1>
			<p>{error}</p>
			<button onclick={() => windowTabs.openTabFromRoute('/wiki')} class="back-link">
				Back to Wiki
			</button>
		</div>
	{:else if wikiPage}
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
		<div class="error">
			<iconify-icon icon="ri:file-unknow-line"></iconify-icon>
			<h1>Page not found</h1>
			<p>The page "{slug}" doesn't exist yet.</p>
			<button onclick={() => windowTabs.openTabFromRoute('/wiki')} class="back-link">
				Back to Wiki
			</button>
		</div>
	{/if}
</div>

<style>
	.wiki-detail-view {
		height: 100%;
		overflow: hidden;
	}

	.loading {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 12px;
		height: 100%;
		color: var(--color-foreground-muted);
	}

	.loading iconify-icon {
		font-size: 32px;
	}

	@keyframes spin {
		from { transform: rotate(0deg); }
		to { transform: rotate(360deg); }
	}

	.animate-spin {
		animation: spin 1s linear infinite;
	}

	.error {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 8px;
		height: 100%;
		text-align: center;
		padding: 2rem;
	}

	.error iconify-icon {
		font-size: 48px;
		color: var(--color-foreground-subtle);
		margin-bottom: 8px;
	}

	.error h1 {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.5rem;
		font-weight: normal;
		color: var(--color-foreground);
		margin: 0;
	}

	.error p {
		font-size: 0.9375rem;
		color: var(--color-foreground-muted);
		margin: 0;
	}

	.back-link {
		margin-top: 16px;
		font-size: 0.875rem;
		color: var(--color-primary);
		background: none;
		border: none;
		cursor: pointer;
		font: inherit;
	}

	.back-link:hover {
		text-decoration: underline;
	}
</style>
