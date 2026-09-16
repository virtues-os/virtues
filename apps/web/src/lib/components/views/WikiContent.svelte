<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { onMount, untrack } from "svelte";
	import {
		getPersonById,
		getPlaceById,
		getOrganizationById,
		getDayByDate,
		type WikiPersonApi,
		type WikiPlaceApi,
		type WikiOrganizationApi,
		type WikiDayApi,
	} from "$lib/wiki/api";
	import {
		YearPage,
		DayPage,
		PersonPage,
		PlacePage,
		OrganizationPage,
	} from "$lib/components/wiki";

	/**
	 * What this view is holding, discriminated by the route that loaded it.
	 *
	 * There used to be a `WikiPage` union of five converted page types, with
	 * `isDayPage`-style guards over a `type` field the converters stamped on.
	 * The guards were the only reason that field existed. `kind` is set right
	 * here, beside the fetch that decides it, and the wire shapes go through
	 * untouched — which is what the year page has done since it was written,
	 * and it reads better than the four that did not.
	 */
	type Loaded =
		| { kind: "day"; page: WikiDayApi }
		| { kind: "person"; page: WikiPersonApi }
		| { kind: "place"; page: WikiPlaceApi }
		| { kind: "organization"; page: WikiOrganizationApi };

	interface Props {
		/** The entity ID to display (e.g., person_abc123, day_2026-01-25) */
		entityId?: string;
		/** Whether this content is currently active/visible */
		active?: boolean;
		/** Callback when the page title is loaded (for tab label updates) */
		onLabelChange?: (label: string) => void;
	}

	let { entityId = "", active = true, onLabelChange }: Props = $props();

	let loading = $state(true);
	let error = $state<string | null>(null);
	let wikiPage = $state<Loaded | undefined>(undefined);
	/** Set when the route is a year; rendered on its own. */
	let yearNumber = $state<number | null>(null);

	// Parse a date string (YYYY-MM-DD) as local date to avoid timezone issues
	function parseDateString(dateStr: string): Date {
		const [year, month, day] = dateStr.split("-").map(Number);
		return new Date(year, month - 1, day);
	}

	// Notify parent of label change
	function updateLabel(label: string) {
		onLabelChange?.(label);
	}

	// Extract the type and identifier from an entity ID
	function parseId(id: string): { type: string; identifier: string } | null {
		const match = id.match(/^([a-z]+)_(.+)$/);
		if (!match) return null;
		return { type: match[1], identifier: match[2] };
	}

	// Load wiki page data
	async function loadPage() {
		if (!entityId) {
			error = "No entity ID provided";
			loading = false;
			return;
		}

		loading = true;
		error = null;

		try {
			// Parse the entity ID to determine type
			const parsed = parseId(entityId);

			if (!parsed) {
				error = `Invalid entity ID format: "${entityId}"`;
				loading = false;
				return;
			}

			const { type, identifier } = parsed;

			// Handle each entity type
			switch (type) {
				case "day": {
					// identifier is the date: 2026-01-25
					const day = await getDayByDate(identifier);
					if (day) {
						wikiPage = { kind: "day", page: day };
						// Update label with formatted date
						const date = parseDateString(identifier);
						const formatted = date.toLocaleDateString("en-US", {
							weekday: "short",
							month: "short",
							day: "numeric",
						});
						updateLabel(formatted);
					} else {
						error = `Day "${identifier}" not found`;
					}
					break;
				}

				case "year": {
					// A year does not go through the WikiPage union: it is its own
					// shape (days with ledes, a chapter dateline, and the state the
					// box decided), and squeezing it into the entity type would
					// lose exactly the parts that make it a year.
					const y = Number(entityId.replace(/^year_/, ""));
					if (Number.isFinite(y)) {
						yearNumber = y;
						updateLabel(String(y));
					} else {
						error = `Not a year: ${entityId}`;
					}
					break;
				}

				case "person": {
					const person = await getPersonById(entityId);
					if (person) {
						wikiPage = { kind: "person", page: person };
						updateLabel(person.name);
					} else {
						error = `Person "${entityId}" not found`;
					}
					break;
				}

				case "place": {
					const place = await getPlaceById(entityId);
					if (place) {
						wikiPage = { kind: "place", page: place };
						updateLabel(place.name);
					} else {
						error = `Place "${entityId}" not found`;
					}
					break;
				}

				case "org": {
					const org = await getOrganizationById(entityId);
					if (org) {
						wikiPage = { kind: "organization", page: org };
						updateLabel(org.name);
					} else {
						error = `Organization "${entityId}" not found`;
					}
					break;
				}





				default:
					error = `Unknown entity type: "${type}"`;
			}
		} catch (e) {
			console.error("Failed to load wiki page:", e);
			error = e instanceof Error ? e.message : "Failed to load page";
		} finally {
			loading = false;
		}
	}

	// Track the last loaded entityId to avoid reloading the same page
	let lastLoadedId = $state<string | null>(null);

	onMount(() => {
		if (entityId && entityId !== lastLoadedId) {
			lastLoadedId = entityId;
			loadPage();
		}
	});

	// Reload only when entityId actually changes to a new value
	// Use untrack() to prevent infinite loops from state updates
	$effect(() => {
		const currentId = entityId;
		const isActive = active;

		// Only reload if entityId changed to a different value
		if (currentId && isActive) {
			untrack(() => {
				if (currentId !== lastLoadedId) {
					lastLoadedId = currentId;
					loadPage();
				}
			});
		}
	});
</script>

<div class="wiki-content">
	{#if loading}
		<div class="flex items-center justify-center h-full">
			<Icon icon="ri:loader-4-line" width="20" class="spin" />
		</div>
	{:else if error}
		<div class="error">
			<Icon icon="ri:error-warning-line" />
			<h1>Page not found</h1>
			<p>{error}</p>
		</div>
	{:else if yearNumber !== null}
		<YearPage year={yearNumber} />
	{:else if wikiPage}
		{#if wikiPage.kind === "day"}
			<DayPage page={wikiPage.page} />
		{:else if wikiPage.kind === "person"}
			<PersonPage page={wikiPage.page} />
		{:else if wikiPage.kind === "place"}
			<PlacePage page={wikiPage.page} />
		{:else if wikiPage.kind === "organization"}
			<OrganizationPage page={wikiPage.page} />
		{/if}
	{:else}
		<div class="error">
			<Icon icon="ri:file-unknow-line" />
			<h1>Page not found</h1>
			<p>The page "{entityId}" doesn't exist yet.</p>
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

	.loading :global(svg) {
		font-size: 32px;
	}

	@keyframes spin {
		from {
			transform: rotate(0deg);
		}
		to {
			transform: rotate(360deg);
		}
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

	.error :global(svg) {
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
