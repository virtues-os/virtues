<!--
	ThingPage.svelte

	Renders a thing page - ideas, philosophies, concepts, objects.
	The catch-all for anything that isn't a person, place, or organization.
-->

<script lang="ts">
	import type { ThingPage as ThingPageType } from "$lib/wiki/types";
	import WikiRightRail from "./WikiRightRail.svelte";
	import "iconify-icon";

	interface Props {
		page: ThingPageType;
	}

	let { page }: Props = $props();

	function formatDate(date: Date): string {
		return date.toLocaleDateString("en-US", {
			month: "long",
			day: "numeric",
			year: "numeric",
		});
	}

	function formatThingType(type: string): string {
		const labels: Record<string, string> = {
			philosophy: "Philosophy",
			framework: "Framework",
			belief: "Belief",
			book: "Book",
			object: "Object",
			concept: "Concept",
			other: "Thing",
		};
		return labels[type] || type;
	}

	// Build content string for TOC
	const fullContent = $derived(`${page.content || ''}

## Discovery

## Core Tenets

## Key Texts

## Connections
`);
</script>

<div class="page-layout">
	<article class="wiki-article">
		<div class="page-content">
			<!-- Header -->
			<header class="page-header">
				{#if page.cover}
					<div class="thing-cover">
						<img src={page.cover} alt={page.title} />
					</div>
				{/if}
				<h1 class="page-title">{page.title}</h1>
				{#if page.subtitle}
					<p class="page-subtitle">{page.subtitle}</p>
				{/if}
				<div class="page-meta">
					<span class="meta-item thing-badge">{formatThingType(page.thingType)}</span>
				</div>
			</header>

			<hr class="divider" />

			<!-- Notes (main narrative content) -->
			{#if page.content}
				<section class="section" id="notes">
					<div class="notes-content">{page.content}</div>
				</section>
			{/if}

			<!-- Discovery -->
			{#if page.firstEncountered || page.introducedBy}
				<section class="section" id="discovery">
					<h2 class="section-title">Discovery</h2>
					<dl class="info-list">
						{#if page.firstEncountered}
							<div class="info-item">
								<dt>First encountered</dt>
								<dd>
									{formatDate(page.firstEncountered.date)}
									{#if page.firstEncountered.context}
										<span class="info-context">— {page.firstEncountered.context}</span>
									{/if}
								</dd>
							</div>
						{/if}
						{#if page.introducedBy}
							<div class="info-item">
								<dt>Introduced by</dt>
								<dd>
									<a href="/wiki/{page.introducedBy.pageSlug}" class="info-link">
										{page.introducedBy.displayName}
									</a>
								</dd>
							</div>
						{/if}
					</dl>
				</section>
			{/if}

			<!-- Core Tenets -->
			{#if page.coreTenets && page.coreTenets.length > 0}
				<section class="section" id="core-tenets">
					<h2 class="section-title">Core Tenets</h2>
					<ol class="tenets-list">
						{#each page.coreTenets as tenet}
							<li class="tenet">{tenet}</li>
						{/each}
					</ol>
				</section>
			{/if}

			<!-- Key Texts -->
			{#if page.keyTexts && page.keyTexts.length > 0}
				<section class="section" id="key-texts">
					<h2 class="section-title">Key Texts</h2>
					<ul class="texts-list">
						{#each page.keyTexts as text}
							<li class="text-item">
								<span class="text-title">{text.title}</span>
								{#if text.author}
									<span class="text-author">by {text.author}</span>
								{/if}
								{#if text.year}
									<span class="text-year">({text.year})</span>
								{/if}
							</li>
						{/each}
					</ul>
				</section>
			{/if}

			<!-- Related Things -->
			{#if page.relatedThings && page.relatedThings.length > 0}
				<section class="section" id="related">
					<h2 class="section-title">Related</h2>
					<ul class="footer-list">
						{#each page.relatedThings as thing}
							<li>
								<a href="/wiki/{thing.pageSlug}" class="footer-link">
									<span class="link-text">{thing.displayName}</span>
								</a>
							</li>
						{/each}
					</ul>
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

			<!-- Associated Places -->
			{#if page.associatedPlaces && page.associatedPlaces.length > 0}
				<section class="section" id="places">
					<h2 class="section-title">Places</h2>
					<ul class="footer-list">
						{#each page.associatedPlaces as place}
							<li>
								<a href="/wiki/{place.pageSlug}" class="footer-link">
									<span class="link-text">{place.displayName}</span>
								</a>
							</li>
						{/each}
					</ul>
				</section>
			{/if}

			<!-- Narrative Context -->
			{#if page.narrativeContext && page.narrativeContext.length > 0}
				<section class="section" id="narrative-context">
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
				<div class="meta-title">{formatThingType(page.thingType)}</div>
				<div class="meta-stats">
					{#if page.coreTenets && page.coreTenets.length > 0}
						<span class="stat">{page.coreTenets.length} tenets</span>
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

	.thing-cover {
		width: 100%;
		max-height: 200px;
		border-radius: 8px;
		overflow: hidden;
		margin-bottom: 1rem;
	}

	.thing-cover img {
		width: 100%;
		height: 100%;
		object-fit: cover;
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

	.thing-badge {
		color: var(--color-foreground-muted);
		font-weight: 500;
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

	.info-context {
		color: var(--color-foreground-muted);
	}

	.info-link {
		color: var(--color-primary);
		text-decoration: none;
	}

	.info-link:hover {
		text-decoration: underline;
	}

	/* Tenets list */
	.tenets-list {
		margin: 0;
		padding-left: 1.25rem;
	}

	.tenet {
		font-size: 0.9375rem;
		color: var(--color-foreground);
		line-height: 1.6;
		padding: 0.25rem 0;
	}

	/* Texts list */
	.texts-list {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.text-item {
		padding: 0.5rem 0;
		border-bottom: 1px solid var(--color-border);
	}

	.text-item:last-child {
		border-bottom: none;
	}

	.text-title {
		font-size: 0.9375rem;
		font-weight: 500;
		color: var(--color-foreground);
	}

	.text-author {
		font-size: 0.875rem;
		color: var(--color-foreground-muted);
		margin-left: 0.5rem;
	}

	.text-year {
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
		margin-left: 0.25rem;
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
