<!--
	YearPage.svelte

	Renders a year overview page - a calendar-based view of a specific year
	showing activity density, key events, and narrative context.
-->

<script lang="ts">
	import type { YearPage as YearPageType } from "$lib/wiki/types";
	import WikiRightRail from "./WikiRightRail.svelte";
	import ActivityHeatmap from "./ActivityHeatmap.svelte";
	import { spaceStore } from "$lib/stores/space.svelte";
	import Icon from "$lib/components/Icon.svelte";

	interface Props {
		page: YearPageType;
	}

	let { page }: Props = $props();

	// Generate activity data from months
	const activityData = $derived.by(() => {
		const data = new Map<string, number>();
		// For now, generate placeholder data based on activeDays
		for (const month of page.months) {
			const daysInMonth = month.totalDays;
			for (let d = 1; d <= daysInMonth; d++) {
				const dateStr = `${page.year}-${String(month.month).padStart(2, "0")}-${String(d).padStart(2, "0")}`;
				// Randomly assign activity levels based on activeDays ratio
				const ratio = month.activeDays / month.totalDays;
				if (Math.random() < ratio) {
					data.set(dateStr, Math.floor(Math.random() * 4) + 1);
				}
			}
		}
		return data;
	});

	function handleDayClick(_date: Date, slug: string) {
		spaceStore.openTabFromRoute(`/wiki/${slug}`);
	}

	function formatPeriod(start: Date, end?: Date): string {
		const startStr = start.toLocaleDateString("en-US", {
			month: "short",
			day: "numeric",
			year: "numeric",
		});
		if (!end) return `${startStr} – present`;
		const endStr = end.toLocaleDateString("en-US", {
			month: "short",
			day: "numeric",
			year: "numeric",
		});
		return `${startStr} – ${endStr}`;
	}

	function getMonthName(monthNum: number): string {
		const date = new Date(page.year, monthNum - 1, 1);
		return date.toLocaleDateString("en-US", { month: "long" });
	}

	// Calculate weeks to show for full year
	const weeksToShow = 52;

	// Build content string for TOC
	const fullContent = $derived(`${page.content || ''}

## Calendar

## Narrative Context

## Key People

## Key Places

## Significant Days

## Themes
`);

	// Calculate total active days
	const totalActiveDays = $derived(
		page.months.reduce((sum, m) => sum + m.activeDays, 0)
	);
</script>

<div class="page-layout">
	<article class="wiki-article">
		<div class="page-content">
			<!-- Header -->
			<header class="page-header">
				<h1 class="page-title">{page.year}</h1>
				{#if page.subtitle}
					<p class="page-subtitle">{page.subtitle}</p>
				{/if}
				<div class="page-meta">
					<span class="meta-item">{totalActiveDays} days recorded</span>
					{#if page.themes.length > 0}
						<span class="meta-sep">·</span>
						<span class="meta-item">{page.themes.slice(0, 2).join(", ")}</span>
					{/if}
				</div>
			</header>

			<hr class="divider" />

			<!-- Content/Reflection -->
			{#if page.content}
				<section class="section" id="reflection">
					<div class="notes-content">{page.content}</div>
				</section>
			{/if}

			<!-- Calendar Heatmap -->
			<section class="section" id="calendar">
				<h2 class="section-title">Calendar</h2>
				<div class="heatmap-container">
					<ActivityHeatmap
						activityData={activityData}
						weeksToShow={weeksToShow}
						onDayClick={handleDayClick}
					/>
				</div>
			</section>

			<!-- Narrative Context (Acts & Chapters) -->
			<section class="section" id="narrative-context">
				<h2 class="section-title">Narrative Context</h2>
				{#if page.acts.length > 0 || page.chapters.length > 0}
					<ul class="footer-list">
						{#each page.acts as act}
							<li>
								<a href="/wiki/{act.pageId}" class="footer-link">
									<span class="link-text">{act.displayName}</span>
									<span class="link-type">act</span>
								</a>
							</li>
						{/each}
						{#each page.chapters as chapter}
							<li>
								<a href="/wiki/{chapter.pageId}" class="footer-link">
									<span class="link-text">{chapter.displayName}</span>
									<span class="link-type">chapter</span>
								</a>
							</li>
						{/each}
					</ul>
				{:else}
					<p class="empty-placeholder">No narrative context</p>
				{/if}
			</section>

			<!-- Key People -->
			<section class="section" id="key-people">
				<h2 class="section-title">Key People</h2>
				{#if page.keyPeople.length > 0}
					<ul class="footer-list">
						{#each page.keyPeople as person}
							<li>
								<a href="/wiki/{person.pageId}" class="footer-link">
									<span class="link-text">{person.displayName}</span>
								</a>
							</li>
						{/each}
					</ul>
				{:else}
					<p class="empty-placeholder">No key people</p>
				{/if}
			</section>

			<!-- Key Places -->
			<section class="section" id="key-places">
				<h2 class="section-title">Key Places</h2>
				{#if page.keyPlaces.length > 0}
					<ul class="footer-list">
						{#each page.keyPlaces as place}
							<li>
								<a href="/wiki/{place.pageId}" class="footer-link">
									<span class="link-text">{place.displayName}</span>
								</a>
							</li>
						{/each}
					</ul>
				{:else}
					<p class="empty-placeholder">No key places</p>
				{/if}
			</section>

			<!-- Significant Days -->
			<section class="section" id="significant-days">
				<h2 class="section-title">Significant Days</h2>
				{#if page.significantDays.length > 0}
					<ul class="footer-list">
						{#each page.significantDays as day}
							<li>
								<a href="/wiki/{day.pageId}" class="footer-link">
									<span class="link-text">{day.displayName}</span>
								</a>
							</li>
						{/each}
					</ul>
				{:else}
					<p class="empty-placeholder">No significant days</p>
				{/if}
			</section>

			<!-- Themes -->
			{#if page.themes.length > 0}
				<section class="section" id="themes">
					<h2 class="section-title">Themes</h2>
					<div class="themes-list">
						{#each page.themes as theme}
							<span class="theme-tag">{theme}</span>
						{/each}
					</div>
				</section>
			{/if}

			<!-- Citations -->
			{#if page.citations && page.citations.length > 0}
				<section class="section" id="data-sources">
					<h2 class="section-title">Data Sources</h2>
					<ul class="footer-list">
						{#each page.citations as citation}
							<li class="citation-item">
								<span class="citation-index">[{citation.index}]</span>
								<span class="citation-label">{citation.label}</span>
							</li>
						{/each}
					</ul>
				</section>
			{/if}
		</div>
	</article>

	<WikiRightRail content={fullContent}>
		{#snippet metadata()}
			<div class="sidebar-meta">
				<div class="meta-title">{page.year}</div>
				<div class="meta-period">
					{formatPeriod(page.period.start, page.period.end)}
				</div>
				<div class="meta-stats">
					<span class="stat">{totalActiveDays} days</span>
					<span class="stat-sep">·</span>
					<span class="stat">{page.acts.length} acts</span>
				</div>
			</div>
		{/snippet}
	</WikiRightRail>
</div>

<style>
	.page-layout {
		display: flex;
		height: 100%;
		width: 100%;
		overflow: hidden;
	}

	.wiki-article {
		flex: 1;
		min-width: 0;
		overflow-y: auto;
		scrollbar-width: none;
		-ms-overflow-style: none;
		padding: 2rem;
	}

	.wiki-article::-webkit-scrollbar {
		display: none;
	}

	.page-content {
		max-width: 48rem;
		margin: 0 auto;
		padding-top: 2rem;
		padding-bottom: 4rem;
	}

	/* Header */
	.page-header {
		margin-bottom: 1rem;
	}

	.page-title {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 2.5rem;
		font-weight: 400;
		color: var(--color-foreground);
		margin: 0 0 0.25rem;
		line-height: 1.2;
		letter-spacing: -0.02em;
	}

	.page-subtitle {
		font-size: 1rem;
		color: var(--color-foreground-muted);
		margin: 0 0 0.5rem;
	}

	.page-meta {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin-top: 0.5rem;
		font-size: 0.875rem;
		color: var(--color-foreground-subtle);
	}

	.meta-sep {
		color: var(--color-border-strong);
	}

	.divider {
		border: none;
		border-top: 1px solid var(--color-border);
		margin: 1rem 0 1.5rem;
	}

	/* Sections */
	.section {
		margin-bottom: 2rem;
	}

	.section-title {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.375rem;
		font-weight: 400;
		line-height: 1.35;
		color: var(--color-foreground);
		margin: 0 0 0.75rem;
	}

	.notes-content {
		font-size: 0.875rem;
		color: var(--color-foreground);
		line-height: 1.6;
		white-space: pre-wrap;
	}

	.heatmap-container {
		overflow-x: auto;
	}

	/* Footer sections */
	.footer-list {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.footer-link {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0.375rem 0;
		color: var(--color-primary);
		text-decoration: none;
	}

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

	.footer-link:hover .link-text {
		background-size: 100% 100%;
	}

	.link-type {
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
		text-transform: lowercase;
	}

	.empty-placeholder {
		font-size: 0.875rem;
		color: var(--color-foreground-subtle);
		font-style: italic;
		margin: 0;
	}

	/* Themes */
	.themes-list {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
	}

	.theme-tag {
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
		padding: 0.25rem 0.625rem;
		border-radius: 9999px;
	}

	/* Citations */
	.citation-item {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
		padding: 0.375rem 0;
	}

	.citation-index {
		font-size: 0.8125rem;
		font-weight: 400;
		color: var(--color-primary);
		flex-shrink: 0;
	}

	.citation-label {
		font-size: 0.875rem;
		color: var(--color-foreground);
		flex: 1;
	}

	/* Sidebar metadata */
	.sidebar-meta {
		text-align: center;
	}

	.meta-title {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.25rem;
		font-weight: 500;
		color: var(--color-foreground);
		margin-bottom: 0.125rem;
	}

	.meta-period {
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
		margin-bottom: 0.5rem;
	}

	.meta-stats {
		display: flex;
		justify-content: center;
		gap: 0.375rem;
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
	}

	.stat-sep {
		color: var(--color-border-strong);
	}

	/* Responsive */
	@media (max-width: 900px) {
		.page-layout {
			flex-direction: column;
		}

		.wiki-article {
			padding: 1rem;
		}

		.page-title {
			font-size: 2rem;
		}
	}
</style>
