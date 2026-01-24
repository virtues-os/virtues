<!--
	PlacePage.svelte

	Renders a place page - a location with meaning in your life.
	Includes a small map, visit history, and associated people/activities.
-->

<script lang="ts">
	import type { PlacePage as PlacePageType } from "$lib/wiki/types";
	import WikiRightRail from "./WikiRightRail.svelte";
	import MovementMap from "$lib/components/timeline/MovementMap.svelte";
	import "iconify-icon";

	interface Props {
		page: PlacePageType;
	}

	let { page }: Props = $props();

	function formatDate(date: Date): string {
		return date.toLocaleDateString("en-US", {
			month: "long",
			day: "numeric",
			year: "numeric",
		});
	}

	function formatPlaceType(type: string): string {
		const labels: Record<string, string> = {
			home: "Home",
			work: "Work",
			"third-place": "Third Place",
			transit: "Transit",
			travel: "Travel",
			other: "Other",
		};
		return labels[type] || type;
	}

	// Map data for single location
	const stopPoints = $derived(
		page.coordinates
			? [
					{
						lat: page.coordinates.lat,
						lng: page.coordinates.lng,
						label: page.title,
						timeMs: Date.now(),
					},
				]
			: [],
	);

	// Build content string for TOC
	const fullContent = $derived(`${page.content || ''}

## Location

## Visit History

## Connections
`);
</script>

<div class="page-layout">
	<article class="wiki-article">
		<div class="page-content">
			<!-- Header -->
			<header class="page-header">
				<h1 class="page-title">{page.title}</h1>
				{#if page.subtitle}
					<p class="page-subtitle">{page.subtitle}</p>
				{/if}
				<div class="page-meta">
					<span class="meta-item place-badge">{formatPlaceType(page.placeType)}</span>
					{#if page.city}
						<span class="meta-sep">·</span>
						<span class="meta-item">{page.city}</span>
					{/if}
				</div>
			</header>

			<hr class="divider" />

			<!-- Map -->
			{#if page.coordinates}
				<section class="section" id="map">
					<MovementMap
						track={stopPoints}
						stops={stopPoints}
						height={200}
					/>
				</section>
			{/if}

			<!-- Notes (main narrative content) -->
			{#if page.content}
				<section class="section" id="notes">
					<div class="notes-content">{page.content}</div>
				</section>
			{/if}

			<!-- Location Details -->
			{#if page.address || page.coordinates}
				<section class="section" id="location">
					<h2 class="section-title">Location</h2>
					<dl class="info-list">
						{#if page.address}
							<div class="info-item">
								<dt>Address</dt>
								<dd>{page.address}</dd>
							</div>
						{/if}
						{#if page.coordinates}
							<div class="info-item">
								<dt>Coordinates</dt>
								<dd class="coords">
									{page.coordinates.lat.toFixed(6)}, {page.coordinates.lng.toFixed(6)}
								</dd>
							</div>
						{/if}
					</dl>
				</section>
			{/if}

			<!-- Visit History -->
			{#if page.firstVisit || page.lastVisit || page.visitCount}
				<section class="section" id="visit-history">
					<h2 class="section-title">Visit History</h2>
					<dl class="info-list">
						{#if page.firstVisit}
							<div class="info-item">
								<dt>First visit</dt>
								<dd>{formatDate(page.firstVisit)}</dd>
							</div>
						{/if}
						{#if page.lastVisit}
							<div class="info-item">
								<dt>Last visit</dt>
								<dd>{formatDate(page.lastVisit)}</dd>
							</div>
						{/if}
						{#if page.visitCount}
							<div class="info-item">
								<dt>Total visits</dt>
								<dd>{page.visitCount}</dd>
							</div>
						{/if}
					</dl>
				</section>
			{/if}

			<!-- Associated People -->
			{#if page.associatedPeople && page.associatedPeople.length > 0}
				<section class="section" id="people">
					<h2 class="section-title">People</h2>
					<ul class="footer-list">
						{#each page.associatedPeople as person}
							<li>
								<a href="/wiki/{person.pageSlug}" class="footer-link">
									<span class="link-text">{person.displayName}</span>
								</a>
							</li>
						{/each}
					</ul>
				</section>
			{/if}

			<!-- Narrative Context -->
			{#if page.narrativeContext && page.narrativeContext.length > 0}
				<section class="section" id="connections">
					<h2 class="section-title">Narrative Context</h2>
					<ul class="footer-list">
						{#each page.narrativeContext as context}
							<li>
								<a href="/wiki/{context.pageSlug}" class="footer-link">
									<span class="link-text">{context.displayName}</span>
								</a>
							</li>
						{/each}
					</ul>
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
				<div class="meta-title">{formatPlaceType(page.placeType)}</div>
				{#if page.city}
					<div class="meta-city">{page.city}</div>
				{/if}
				<div class="meta-stats">
					{#if page.visitCount}
						<span class="stat">{page.visitCount} visits</span>
						<span class="stat-sep">·</span>
					{/if}
					<span class="stat">{page.citations?.length || 0} sources</span>
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
		font-size: 1.75rem;
		font-weight: 400;
		color: var(--color-foreground);
		margin: 0 0 0.25rem;
		line-height: 1.3;
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

	.place-badge {
		color: var(--color-foreground-muted);
		font-weight: 500;
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

	/* Info list */
	.info-list {
		margin: 0;
		padding: 0;
	}

	.info-item {
		display: flex;
		gap: 1rem;
		padding: 0.5rem 0;
		border-bottom: 1px solid var(--color-border);
	}

	.info-item:last-child {
		border-bottom: none;
	}

	.info-item dt {
		flex-shrink: 0;
		width: 120px;
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
	}

	.info-item dd {
		margin: 0;
		font-size: 0.875rem;
		color: var(--color-foreground);
	}

	.coords {
		font-family: var(--font-mono, monospace);
		font-size: 0.8125rem;
	}

	/* Footer sections */
	.footer-list {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.footer-link {
		display: block;
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
		font-size: 0.875rem;
		font-weight: 500;
		color: var(--color-foreground);
		margin-bottom: 0.125rem;
	}

	.meta-city {
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
			font-size: 1.5rem;
		}

		.info-item {
			flex-direction: column;
			gap: 0.25rem;
		}

		.info-item dt {
			width: auto;
		}
	}
</style>
