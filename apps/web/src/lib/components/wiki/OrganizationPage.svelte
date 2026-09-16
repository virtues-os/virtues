<!--
	OrganizationPage.svelte

	Renders an organization page - employers, schools, communities.
	Tracks your relationship with groups over time.
-->

<script lang="ts">
	import { subjectHref } from "$lib/wiki/links";
	import type { WikiOrganizationApi } from "$lib/wiki/api";
	import EntityArticleSection from "./EntityArticleSection.svelte";
	import SubjectBacklinks from "./SubjectBacklinks.svelte";
	import NotesRail from "./NotesRail.svelte";
	import EntityRecordsSection from "./EntityRecordsSection.svelte";
	import AliasEditor from "./AliasEditor.svelte";
	import Markdown from "$lib/components/Markdown.svelte";
	import { updateOrganization } from "$lib/wiki/api";

	interface Props {
		/** The wire shape. See PersonPage for why the converter is gone. */
		page: WikiOrganizationApi;
	}

	let { page }: Props = $props();

	function formatDate(date: Date): string {
		return date.toLocaleDateString("en-US", {
			month: "long",
			year: "numeric",
		});
	}

	/**
	 * The badge over an organization's name, from its stored type.
	 *
	 * One map where there were two: the converter folded `company` into
	 * `employer` and `university` into `school`, and this function then turned
	 * those into words. A type with no entry keeps its own word instead of
	 * flattening to "Organization".
	 */
	function orgLabel(type: string | null): string {
		const labels: Record<string, string> = {
			employer: "Employer",
			company: "Employer",
			school: "School",
			university: "School",
			community: "Community",
			church: "Community",
			club: "Community",
			institution: "Institution",
			government: "Institution",
			hospital: "Institution",
		};
		if (!type) return "Organization";
		const key = type.toLowerCase();
		return labels[key] ?? type.charAt(0).toUpperCase() + type.slice(1);
	}

	/** "March 2019 — Present", from the two wire dates. */
	const period = $derived.by(() => {
		if (!page.started_at) return null;
		const start = formatDate(new Date(page.started_at));
		const end = page.ended_at ? formatDate(new Date(page.ended_at)) : "Present";
		return `${start} — ${end}`;
	});

	async function saveAliases(next: string[]) {
		const saved = await updateOrganization(page.id, { aliases: next });
		if (!saved) throw new Error("Could not save aliases");
	}

</script>

<div class="page-layout">
	<article class="wiki-article">
		<div class="page-content">
			<!-- Header -->
			<header class="page-header">
				<!-- `cover_image` has a PUT that accepts it and no client that
				     sends one, so this has never drawn. Kept as the one place
				     an org cover would go if anything ever wrote one. -->
				{#if page.cover_image}
					<div class="org-logo">
						<img src={page.cover_image} alt={page.name} />
					</div>
				{/if}
				<h1 class="page-title">{page.name}</h1>
				<!-- A subtitle was read here and never set. -->
				<div class="page-meta">
					<span class="meta-item org-badge">{orgLabel(page.organization_type)}</span>
					{#if page.role_title}
						<span class="meta-sep">·</span>
						<span class="meta-item">{page.role_title}</span>
					{/if}
				</div>
			</header>

			<!-- Also known as. Same placement as the person page: it corrects
			     the record calling this org by a surface the resolver does not
			     recognise — Gusto the sender vs Gusto the company. Writing one
			     here backfills every past mention of it (migration 0037). -->
			<div class="org-aliases">
				<AliasEditor
					aliases={page.aliases ?? []}
					canonicalName={page.name}
					onSave={saveAliases}
				/>
			</div>

			<hr class="divider" />

			<!-- The article: machine-written prose about this entity -->
			<section class="section" id="article">
				<EntityArticleSection
					article={page.article}
					articleUpdatedAt={page.article_updated_at
						? new Date(page.article_updated_at)
						: null}
					name={page.name}
									subjectType="organization"
					subjectId={page.id}
					maintained={page.article_maintained}
					onChanged={() => location.reload()}
				/>
			</section>

			<!-- The record: every data point that references this organization -->
			<section class="section" id="the-record">
				<h2 class="section-title">The record</h2>
				<EntityRecordsSection entityId={page.id} />
			</section>

			<SubjectBacklinks subjectType="organization" subjectId={page.id} />
			<NotesRail subjectType="organization" subjectId={page.id} />

			<!-- Notes: the user's own writing -->
			{#if page.content}
				<section class="section" id="notes">
					<div class="notes-content">
						<Markdown content={page.content} refVariant="quiet" />
					</div>
				</section>
			{/if}

			<!-- Your Role -->
			{#if page.role_title || period}
				<section class="section" id="your-role">
					<h2 class="section-title">Your Role</h2>
					<dl class="info-list">
						{#if page.role_title}
							<div class="info-item">
								<dt>Position</dt>
								<dd>{page.role_title}</dd>
							</div>
						{/if}
						{#if period}
							<div class="info-item">
								<dt>Period</dt>
								<dd>{period}</dd>
							</div>
						{/if}
					</dl>
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

	/* Between the header and the article's rule, matching the person page. */
	.org-aliases {
		margin-top: 0.625rem;
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
