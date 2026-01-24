<script lang="ts">
	import { onMount, untrack } from 'svelte';
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

	interface Props {
		/** The wiki page slug to display */
		slug?: string;
		/** Whether this content is currently active/visible */
		active?: boolean;
		/** Callback when the page title is loaded (for tab label updates) */
		onLabelChange?: (label: string) => void;
	}

	let { slug = '', active = true, onLabelChange }: Props = $props();

	let loading = $state(true);
	let error = $state<string | null>(null);
	let wikiPage = $state<WikiPageType | undefined>(undefined);

	// Parse a date string (YYYY-MM-DD) as local date to avoid timezone issues
	function parseDateString(dateStr: string): Date {
		const [year, month, day] = dateStr.split('-').map(Number);
		return new Date(year, month - 1, day);
	}

	// Notify parent of label change
	function updateLabel(label: string) {
		onLabelChange?.(label);
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
					// Update label with formatted date
					const date = parseDateString(slug);
					const formatted = date.toLocaleDateString('en-US', {
						weekday: 'short',
						month: 'short',
						day: 'numeric',
					});
					updateLabel(formatted);
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
					updateLabel(formatted);
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
					updateLabel(slug);
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
					updateLabel(mockPage.title);
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
						updateLabel(person.canonical_name);
					}
					break;
				}
				case 'place': {
					const place = await getPlaceBySlug(slug);
					if (place) {
						wikiPage = apiToPlacePage(place);
						updateLabel(place.name);
					}
					break;
				}
				case 'organization': {
					const org = await getOrganizationBySlug(slug);
					if (org) {
						wikiPage = apiToOrganizationPage(org);
						updateLabel(org.canonical_name);
					}
					break;
				}
				case 'thing': {
					const thing = await getThingBySlug(slug);
					if (thing) {
						wikiPage = apiToThingPage(thing);
						updateLabel(thing.canonical_name);
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
						updateLabel(formatted);
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
							updateLabel(act.title);
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
							updateLabel(chapter.title);
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
							updateLabel(telos.title);
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
					updateLabel(mockPage.title);
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
				updateLabel(mockPage.title);
			} else {
				error = e instanceof Error ? e.message : 'Failed to load page';
			}
		} finally {
			loading = false;
		}
	}

	// Track the last loaded slug to avoid reloading the same page
	let lastLoadedSlug = $state<string | null>(null);

	onMount(() => {
		console.log('[WikiContent] onMount, slug:', slug, 'active:', active);
		if (slug && slug !== lastLoadedSlug) {
			lastLoadedSlug = slug;
			loadPage();
		}
	});

	// Reload only when slug actually changes to a new value
	// Use untrack() to prevent infinite loops from state updates
	$effect(() => {
		const currentSlug = slug;
		const isActive = active;
		
		// Only reload if slug changed to a different value
		if (currentSlug && isActive) {
			untrack(() => {
				if (currentSlug !== lastLoadedSlug) {
					console.log('[WikiContent] slug changed, reloading:', currentSlug);
					lastLoadedSlug = currentSlug;
					loadPage();
				}
			});
		}
	});
</script>

<div class="wiki-content">
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
		</div>
	{/if}
</div>

<style>
	.wiki-content {
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
</style>
