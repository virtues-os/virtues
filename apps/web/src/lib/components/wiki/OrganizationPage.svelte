<!--
	OrganizationPage.svelte

	Renders an organization page - employers, schools, communities.
	Tracks your relationship with groups over time.
-->

<script lang="ts">
	import type { OrganizationPage as OrganizationPageType } from "$lib/wiki/types";
	import EntitySummarySection from "./EntitySummarySection.svelte";
	import EntityRecordsSection from "./EntityRecordsSection.svelte";
	import CitedMarkdown from "$lib/components/CitedMarkdown.svelte";

	interface Props {
		page: OrganizationPageType;
	}

	let { page }: Props = $props();

	function formatDate(date: Date): string {
		return date.toLocaleDateString("en-US", {
			month: "long",
			year: "numeric",
		});
	}

	function formatOrgType(type: string): string {
		const labels: Record<string, string> = {
			employer: "Employer",
			school: "School",
			community: "Community",
			institution: "Institution",
			other: "Organization",
		};
		return labels[type] || type;
	}

	function formatPeriod(period: { start: Date; end?: Date }): string {
		const start = formatDate(period.start);
		const end = period.end ? formatDate(period.end) : "Present";
		return `${start} — ${end}`;
	}

</script>

<div class="page-layout">
	<article class="wiki-article">
		<div class="page-content">
			<!-- Header -->
			<header class="page-header">
				{#if page.cover}
					<div class="org-logo">
						<img src={page.cover} alt={page.title} />
					</div>
				{/if}
				<h1 class="page-title">{page.title}</h1>
				{#if page.subtitle}
					<p class="page-subtitle">{page.subtitle}</p>
				{/if}
				<div class="page-meta">
					<span class="meta-item org-badge">{formatOrgType(page.orgType)}</span>
					{#if page.role}
						<span class="meta-sep">·</span>
						<span class="meta-item">{page.role}</span>
					{/if}
				</div>
			</header>

			<hr class="divider" />

			<!-- Summary: the machine-written article -->
			<section class="section" id="summary">
				<EntitySummarySection
					summary={page.summary}
					summarizedAt={page.summarizedAt}
					name={page.title}
				/>
			</section>

			<!-- The record: every data point that references this organization -->
			<section class="section" id="the-record">
				<h2 class="section-title">The record</h2>
				<EntityRecordsSection entityId={page.id} />
			</section>

			<!-- Notes: the user's own writing -->
			{#if page.content}
				<section class="section" id="notes">
					<div class="notes-content">
						<CitedMarkdown content={page.content} refVariant="quiet" />
					</div>
				</section>
			{/if}

			<!-- Your Role -->
			{#if page.role || page.period}
				<section class="section" id="your-role">
					<h2 class="section-title">Your Role</h2>
					<dl class="info-list">
						{#if page.role}
							<div class="info-item">
								<dt>Position</dt>
								<dd>{page.role}</dd>
							</div>
						{/if}
						{#if page.period}
							<div class="info-item">
								<dt>Period</dt>
								<dd>{formatPeriod(page.period)}</dd>
							</div>
						{/if}
					</dl>
				</section>
			{/if}

			<!-- Key Contacts -->
			{#if page.keyContacts && page.keyContacts.length > 0}
				<section class="section" id="key-contacts">
					<h2 class="section-title">Key Contacts</h2>
					<ul class="footer-list">
						{#each page.keyContacts as person}
							<li>
								<a href="/wiki/{person.pageId}" class="footer-link">
									<span class="link-text">{person.displayName}</span>
									{#if person.preview}
										<span class="link-preview">{person.preview}</span>
									{/if}
								</a>
							</li>
						{/each}
					</ul>
				</section>
			{/if}

			<!-- Locations -->
			{#if page.locations && page.locations.length > 0}
				<section class="section" id="locations">
					<h2 class="section-title">Locations</h2>
					<ul class="footer-list">
						{#each page.locations as place}
							<li>
								<a href="/wiki/{place.pageId}" class="footer-link">
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
								<a href="/wiki/{context.pageId}" class="footer-link">
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

	.org-logo {
		width: 64px;
		height: 64px;
		border-radius: 8px;
		overflow: hidden;
		margin-bottom: 1rem;
		border: 1px solid var(--color-border);
	}

	.org-logo img {
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

	.org-badge {
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

	/* Footer sections */
	.footer-list {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.footer-link {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
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

	.link-preview {
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
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
