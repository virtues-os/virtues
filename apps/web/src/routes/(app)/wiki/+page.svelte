<script lang="ts">
	import { goto } from "$app/navigation";
	import { page } from "$app/state";
	import {
		getAllPages,
		getAllActs,
		getCurrentAct,
		getMockActivityData,
	} from "$lib/wiki";
	import type { WikiPageType } from "$lib/wiki/types/base";
	import { ActivityHeatmap } from "$lib/components/wiki";
	import "iconify-icon";

	const allPages = getAllPages();
	const allActs = getAllActs();
	const currentAct = getCurrentAct();
	const activityData = getMockActivityData();

	// Generate years from earliest act to current year
	const years = $derived.by(() => {
		if (allActs.length === 0) return [];
		const earliestAct = allActs[allActs.length - 1];
		const startYear = earliestAct.period.start.getFullYear();
		const currentYear = new Date().getFullYear();
		const yearList: number[] = [];
		for (let y = currentYear; y >= startYear; y--) {
			yearList.push(y);
		}
		return yearList;
	});

	// Redirect legacy query params to dedicated routes
	const typeFilter = $derived(
		page.url.searchParams.get("type") as WikiPageType | null,
	);

	$effect(() => {
		if (typeFilter === "person") {
			goto("/wiki/people", { replaceState: true });
		} else if (typeFilter === "place") {
			goto("/wiki/places", { replaceState: true });
		} else if (typeFilter === "organization") {
			goto("/wiki/orgs", { replaceState: true });
		} else if (typeFilter === "thing") {
			goto("/wiki/things", { replaceState: true });
		}
	});

	// Group pages by type
	const pagesByType = $derived.by(() => {
		const groups = new Map<WikiPageType, typeof allPages>();
		for (const p of allPages) {
			if (!groups.has(p.type)) {
				groups.set(p.type, []);
			}
			const arr = groups.get(p.type);
			if (arr) arr.push(p);
		}
		return groups;
	});

	// Entity types for display
	const entityTypes: WikiPageType[] = ["person", "place", "organization", "thing"];

	// Entity labels (plural)
	const entityLabels: Record<string, string> = {
		person: "People",
		place: "Places",
		organization: "Organizations",
		thing: "Things",
	};

	// Entity route mapping
	const entityRoutes: Record<string, string> = {
		person: "/wiki/people",
		place: "/wiki/places",
		organization: "/wiki/orgs",
		thing: "/wiki/things",
	};

	// Handle day click from heatmap
	function handleDayClick(_date: Date, slug: string) {
		goto(`/wiki/${slug}`);
	}

	// Format period for acts
	function formatPeriod(start: Date, end?: Date): string {
		const startYear = start.getFullYear();
		if (!end) return `${startYear}–present`;
		const endYear = end.getFullYear();
		if (startYear === endYear) return `${startYear}`;
		return `${startYear}–${endYear}`;
	}

	// Today's formatted date
	const today = new Date();
	const todaySlug = today.toISOString().split("T")[0];
	const todayFormatted = today.toLocaleDateString("en-US", {
		weekday: "long",
		month: "long",
		day: "numeric",
		year: "numeric",
	});

	// Current act number (for "Act III" style)
	const actNumber = $derived(
		currentAct ? allActs.length - allActs.indexOf(currentAct) : 0,
	);

	// Roman numeral conversion for act numbers
	function toRoman(num: number): string {
		const romanNumerals: [number, string][] = [
			[10, "X"], [9, "IX"], [5, "V"], [4, "IV"], [1, "I"]
		];
		let result = "";
		for (const [value, numeral] of romanNumerals) {
			while (num >= value) {
				result += numeral;
				num -= value;
			}
		}
		return result;
	}
</script>

<div class="wiki-page">
		<header class="page-header">
			<h1>Wiki</h1>
		</header>

		<!-- Context: Today & Current Act -->
		<div class="context">
			<p class="context-line">
				Today is <a href="/wiki/{todaySlug}" class="context-link"><span class="link-text">{todayFormatted}</span></a>.
			</p>
			{#if currentAct}
				<p class="context-line">
					You are in <a href="/wiki/{currentAct.slug}" class="context-link"><span class="link-text">Act {toRoman(actNumber)}: {currentAct.title}</span></a>.
				</p>
			{/if}
		</div>

		<hr class="divider" />

		<!-- Activity Heatmap -->
		<section class="section">
			<ActivityHeatmap {activityData} onDayClick={handleDayClick} />
		</section>

		<hr class="divider" />

		<!-- Your Story intro -->
		<section class="section intro-section">
			<h2>Your Story</h2>
			<p class="intro-text">
				The wiki organizes your life in three ways:
			</p>
		</section>

		<!-- NARRATIVE -->
		<section class="section category-section">
			<h3 class="category-header">Narrative</h3>
			<p class="category-description">
				How you've divided your life into meaning. Your story is told through acts (major seasons) and chapters (arcs within them).
			</p>

			<div class="category-content">
				{#each allActs as act, i}
					<div class="act-row">
						<a href="/wiki/{act.slug}" class="act-link">
							<span class="act-number">{toRoman(allActs.length - i)}.</span>
							<span class="act-title">{act.title}</span>
							<span class="act-period">{formatPeriod(act.period.start, act.period.end)}</span>
						</a>
						{#if act.chapters.length > 0}
							<div class="chapters-row">
								{#each act.chapters as chapter, j}
									<a href="/wiki/{chapter.pageSlug}" class="chapter-link">{chapter.displayName}</a>{#if j < act.chapters.length - 1}<span class="chapter-sep">·</span>{/if}
								{/each}
							</div>
						{/if}
					</div>
				{/each}

				{#if allActs.length === 0}
					<p class="empty-placeholder">No acts defined yet.</p>
				{/if}
			</div>
		</section>

		<!-- TEMPORAL -->
		<section class="section category-section">
			<h3 class="category-header">Temporal</h3>
			<p class="category-description">
				How time actually passed. Years and days exist whether you record them or not.
			</p>

			<div class="category-content">
				{#if years.length > 0}
					<div class="years-row">
						{#each years as year, i}
							<a href="/wiki/{year}" class="year-link">{year}</a>{#if i < years.length - 1}<span class="year-sep">·</span>{/if}
						{/each}
					</div>
				{:else}
					<p class="empty-placeholder">No years with data yet.</p>
				{/if}
			</div>
		</section>

		<!-- ENTITIES -->
		<section class="section category-section">
			<h3 class="category-header">Entities</h3>
			<p class="category-description">
				The people, places, and things in your story. Reference pages for who and what appears across your life.
			</p>

			<div class="category-content">
				<div class="entity-row">
					{#each entityTypes as type}
						{@const count = pagesByType.get(type)?.length || 0}
						<a href={entityRoutes[type]} class="entity-link">
							{entityLabels[type]}{#if count > 0} ({count}){/if}
						</a>
					{/each}
				</div>
			</div>
		</section>
</div>

<style>
	.wiki-page {
		max-width: 42rem;
		margin: 0 auto;
		padding: 2.5rem 2rem;
		height: 100%;
		overflow-y: auto;
	}

	/* Header */
	.page-header {
		margin-bottom: 1.5rem;
	}

	.page-header h1 {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 2rem;
		font-weight: 400;
		color: var(--color-foreground);
		margin: 0;
		letter-spacing: -0.02em;
	}

	/* Context block */
	.context {
		margin-bottom: 0.5rem;
	}

	.context-line {
		margin: 0;
		font-size: 0.9375rem;
		color: var(--color-foreground-muted);
		line-height: 1.6;
	}

	.context-link {
		color: var(--color-primary);
		text-decoration: none;
	}

	/* Slide-up link decoration */
	.link-text {
		display: inline;
		position: relative;
		background-image: linear-gradient(
			to top,
			color-mix(in srgb, var(--color-primary) 15%, transparent),
			color-mix(in srgb, var(--color-primary) 15%, transparent)
		);
		background-repeat: no-repeat;
		background-size: 100% 0%;
		background-position: 0 100%;
		transition: background-size 0.2s ease;
	}

	.context-link:hover .link-text {
		background-size: 100% 100%;
	}

	/* Divider */
	.divider {
		border: none;
		border-top: 1px solid var(--color-border);
		margin: 1.5rem 0;
	}

	/* Sections */
	.section {
		margin-bottom: 2rem;
	}

	.section h2 {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.375rem;
		font-weight: 400;
		line-height: 1.35;
		color: var(--color-foreground);
		margin: 0 0 0.5rem 0;
	}

	/* Intro section */
	.intro-section {
		margin-bottom: 1.5rem;
	}

	.intro-text {
		font-size: 0.9375rem;
		color: var(--color-foreground-muted);
		margin: 0;
		line-height: 1.5;
	}

	/* Category sections */
	.category-section {
		margin-bottom: 2.5rem;
	}

	.category-header {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.125rem;
		font-weight: 500;
		color: var(--color-foreground);
		margin: 0 0 0.375rem 0;
		letter-spacing: 0.02em;
	}

	.category-description {
		font-size: 0.875rem;
		color: var(--color-foreground-muted);
		margin: 0 0 1rem 0;
		line-height: 1.5;
	}

	.category-content {
		padding-left: 0.5rem;
	}

	/* Act rows */
	.act-row {
		margin-bottom: 0.75rem;
	}

	.act-row:last-child {
		margin-bottom: 0;
	}

	.act-link {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
		text-decoration: none;
	}

	.act-link:hover .act-title {
		text-decoration: underline;
	}

	.act-number {
		font-size: 0.875rem;
		color: var(--color-foreground-subtle);
		min-width: 1.5rem;
	}

	.act-title {
		font-size: 0.9375rem;
		color: var(--color-foreground);
	}

	.act-period {
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
	}

	/* Chapters row */
	.chapters-row {
		display: flex;
		flex-wrap: wrap;
		gap: 0.25rem;
		margin-top: 0.25rem;
		padding-left: 2rem;
	}

	.chapter-link {
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
		text-decoration: none;
	}

	.chapter-link:hover {
		color: var(--color-foreground);
		text-decoration: underline;
	}

	.chapter-sep {
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
	}

	/* Years row */
	.years-row {
		display: flex;
		flex-wrap: wrap;
		gap: 0.25rem;
	}

	.year-link {
		font-size: 0.9375rem;
		color: var(--color-foreground);
		text-decoration: none;
	}

	.year-link:hover {
		text-decoration: underline;
	}

	.year-sep {
		font-size: 0.9375rem;
		color: var(--color-foreground-subtle);
	}

	/* Entity row */
	.entity-row {
		display: flex;
		flex-wrap: wrap;
		gap: 0.25rem 1.5rem;
	}

	.entity-link {
		font-size: 0.9375rem;
		color: var(--color-foreground);
		text-decoration: none;
	}

	.entity-link:hover {
		text-decoration: underline;
	}

	/* Empty placeholder */
	.empty-placeholder {
		font-size: 0.875rem;
		color: var(--color-foreground-subtle);
		font-style: italic;
		margin: 0;
	}

	/* Responsive */
	@media (max-width: 640px) {
		.wiki-page {
			padding: 1.5rem;
		}

		.act-period {
			display: none;
		}

		.entity-row {
			flex-direction: column;
			gap: 0.5rem;
		}
	}
</style>
