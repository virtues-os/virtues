<!--
	ThingPage.svelte

	Renders a thing page - catchall entity for pets, projects, concepts, etc.
-->

<script lang="ts">
	import type { ThingPage as ThingPageType } from "$lib/wiki/types";
	import WikiRightRail from "./WikiRightRail.svelte";

	interface Props {
		page: ThingPageType;
	}

	let { page }: Props = $props();

	const fullContent = $derived(page.content || '');
</script>

<div class="page-layout">
	<article class="wiki-article">
		<div class="page-content">
			<!-- Header -->
			<header class="page-header">
				{#if page.cover}
					<div class="thing-image">
						<img src={page.cover} alt={page.title} />
					</div>
				{/if}
				<h1 class="page-title">{page.title}</h1>
				{#if page.description}
					<p class="page-subtitle">{page.description}</p>
				{/if}
				{#if page.category}
					<div class="page-meta">
						<span class="meta-item category-badge">{page.category}</span>
					</div>
				{/if}
			</header>

			<hr class="divider" />

			<!-- Content -->
			{#if page.content}
				<section class="section" id="notes">
					<div class="notes-content">{page.content}</div>
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
				{#if page.category}
					<div class="meta-title">{page.category}</div>
				{/if}
				<div class="meta-stats">
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

	.page-header {
		margin-bottom: 1rem;
	}

	.thing-image {
		width: 64px;
		height: 64px;
		border-radius: 8px;
		overflow: hidden;
		margin-bottom: 1rem;
		border: 1px solid var(--color-border);
	}

	.thing-image img {
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

	.category-badge {
		color: var(--color-foreground-muted);
		font-weight: 500;
		text-transform: capitalize;
	}

	.divider {
		border: none;
		border-top: 1px solid var(--color-border);
		margin: 1rem 0 1.5rem;
	}

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

	.footer-list {
		list-style: none;
		margin: 0;
		padding: 0;
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

	.sidebar-meta {
		text-align: center;
	}

	.meta-title {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 0.875rem;
		font-weight: 500;
		color: var(--color-foreground);
		text-transform: capitalize;
		margin-bottom: 0.5rem;
	}

	.meta-stats {
		display: flex;
		justify-content: center;
		gap: 0.375rem;
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
	}

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
	}
</style>
