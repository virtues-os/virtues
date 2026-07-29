<!--
	PersonPage.svelte

	Personal CRM wiki page for a person.
	Structured like other wiki pages (DayPage, PlacePage, etc.)
-->

<script lang="ts">
	import type { PersonPage as PersonPageType } from "$lib/wiki/types";
	import EntitySummarySection from "./EntitySummarySection.svelte";
	import EntityRecordsSection from "./EntityRecordsSection.svelte";
	import CitedMarkdown from "$lib/components/CitedMarkdown.svelte";

	interface Props {
		page: PersonPageType;
	}

	let { page }: Props = $props();

	// Connection tier display
	const tierLabels: Record<string, string> = {
		"inner-circle": "Inner Circle",
		"close": "Close",
		"regular": "Regular",
		"distant": "Distant",
		"acquaintance": "Acquaintance",
	};

	function formatBirthday(date: Date): string {
		return date.toLocaleDateString("en-US", {
			month: "long",
			day: "numeric",
		});
	}

	// Sections render only when they have content — a wiki article never
	// prints an empty heading (the summary stub is the one exception, as an
	// honest invitation).
	const hasContact = $derived(
		Boolean(
			page.emails?.length ||
				page.phones?.length ||
				page.socials?.linkedin ||
				page.socials?.twitter ||
				page.socials?.instagram
		)
	);
	const hasAbout = $derived(
		Boolean(page.location || page.company || page.role || page.birthday)
	);

</script>

<div class="person-page-layout">
	<article class="person-article wiki-article">
		<div class="person-content">
			<!-- Header -->
			<header class="person-header">
				<h1 class="person-title">{page.title}</h1>
				{#if page.nickname && page.nickname !== page.title}
					<p class="person-subtitle">"{page.nickname}"</p>
				{/if}
				<div class="person-meta">
					<span class="meta-badge">{page.relationship}</span>
					{#if page.connectionTier}
						<span class="meta-sep">·</span>
						<span class="meta-tier">{tierLabels[page.connectionTier]}</span>
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

			<!-- The record: every data point that references this person -->
			<section class="section" id="the-record">
				<h2 class="section-title">The record</h2>
				<EntityRecordsSection entityId={page.id} />
			</section>

			{#if hasContact}
			<!-- Contact -->
			<section class="section" id="contact">
				<h2 class="section-title">Contact</h2>
				{#if page.emails?.length || page.phones?.length || page.socials}
					<dl class="info-list">
						{#if page.emails && page.emails.length > 0}
							<div class="info-row">
								<dt>Email</dt>
								<dd>
									{#each page.emails as email, i}
										<a href="mailto:{email}" class="info-link">{email}</a>{#if i < page.emails.length - 1}, {/if}
									{/each}
								</dd>
							</div>
						{/if}
						{#if page.phones && page.phones.length > 0}
							<div class="info-row">
								<dt>Phone</dt>
								<dd>
									{#each page.phones as phone, i}
										<a href="tel:{phone}" class="info-link">{phone}</a>{#if i < page.phones.length - 1}, {/if}
									{/each}
								</dd>
							</div>
						{/if}
						{#if page.socials?.linkedin}
							<div class="info-row">
								<dt>LinkedIn</dt>
								<dd><a href="https://linkedin.com/in/{page.socials.linkedin}" target="_blank" class="info-link">{page.socials.linkedin}</a></dd>
							</div>
						{/if}
						{#if page.socials?.twitter}
							<div class="info-row">
								<dt>Twitter</dt>
								<dd><a href="https://x.com/{page.socials.twitter}" target="_blank" class="info-link">@{page.socials.twitter}</a></dd>
							</div>
						{/if}
						{#if page.socials?.instagram}
							<div class="info-row">
								<dt>Instagram</dt>
								<dd><a href="https://instagram.com/{page.socials.instagram}" target="_blank" class="info-link">@{page.socials.instagram}</a></dd>
							</div>
						{/if}
					</dl>
				{:else}
					<p class="empty-placeholder">No contact info</p>
				{/if}
			</section>
			{/if}

			{#if hasAbout}
			<!-- About -->
			<section class="section" id="about">
				<h2 class="section-title">About</h2>
				{#if page.location || page.company || page.role || page.birthday}
					<dl class="info-list">
						{#if page.role || page.company}
							<div class="info-row">
								<dt>Work</dt>
								<dd>
									{#if page.role && page.company}
										{page.role} at {page.company}
									{:else if page.role}
										{page.role}
									{:else}
										{page.company}
									{/if}
								</dd>
							</div>
						{/if}
						{#if page.location}
							<div class="info-row">
								<dt>Location</dt>
								<dd>{page.location}</dd>
							</div>
						{/if}
						{#if page.birthday}
							<div class="info-row">
								<dt>Birthday</dt>
								<dd>{formatBirthday(page.birthday)}</dd>
							</div>
						{/if}
					</dl>
				{:else}
					<p class="empty-placeholder">No info</p>
				{/if}
			</section>
			{/if}

			{#if page.linkedPages && page.linkedPages.length > 0}
			<!-- Connections -->
			<section class="section" id="connections">
				<h2 class="section-title">Connections</h2>
				<ul class="footer-list">
					{#each page.linkedPages as linked}
						<li>
							<a href="/wiki/{linked.pageId}" class="footer-link">
								<span class="link-text">{linked.displayName}</span>
							</a>
						</li>
					{/each}
				</ul>
			</section>
			{/if}

			{#if page.content}
			<!-- Notes: the user's own writing -->
			<section class="section" id="notes">
				<h2 class="section-title">Notes</h2>
				<div class="notes-content">
					<CitedMarkdown content={page.content} refVariant="quiet" />
				</div>
			</section>
			{/if}
		</div>
	</article>
</div>

<style>
	.person-page-layout {
		display: flex;
		height: 100%;
		width: 100%;
		overflow: hidden;
	}

	.person-article {
		flex: 1;
		min-width: 0;
		overflow-y: auto;
		scrollbar-width: none;
		-ms-overflow-style: none;
		padding: 2rem;
	}

	.person-article::-webkit-scrollbar {
		display: none;
	}

	.person-content {
		max-width: 48rem;
		margin: 0 auto;
		padding-top: 2rem;
		padding-bottom: 4rem;
	}

	/* Header */
	.person-header {
		margin-bottom: 1rem;
	}

	.person-title {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.75rem;
		font-weight: 400;
		color: var(--color-foreground);
		margin: 0;
		line-height: 1.3;
	}

	.person-subtitle {
		font-size: 1rem;
		color: var(--color-foreground-muted);
		margin: 0.25rem 0 0;
		font-style: italic;
	}

	.person-meta {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin-top: 0.5rem;
		font-size: 0.875rem;
	}

	.meta-badge {
		color: var(--color-primary);
		font-weight: 500;
	}

	.meta-sep {
		color: var(--color-border-strong);
	}

	.meta-tier {
		color: var(--color-foreground-muted);
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

	/* Info list (dt/dd) */
	.info-list {
		margin: 0;
	}

	.info-row {
		display: flex;
		gap: 1rem;
		padding: 0.375rem 0;
		border-bottom: 1px solid var(--color-border);
	}

	.info-row:last-child {
		border-bottom: none;
	}

	.info-row dt {
		width: 80px;
		flex-shrink: 0;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}

	.info-row dd {
		flex: 1;
		margin: 0;
		font-size: 0.875rem;
		color: var(--color-foreground);
	}

	.info-link {
		color: var(--color-primary);
		text-decoration: none;
	}

	.info-link:hover {
		text-decoration: underline;
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

	.empty-placeholder {
		font-size: 0.875rem;
		color: var(--color-foreground-subtle);
		font-style: italic;
		margin: 0;
	}

	/* Responsive */
	@media (max-width: 900px) {
		.person-page-layout {
			flex-direction: column;
		}

		.person-article {
			padding: 1rem;
		}

		.person-title {
			font-size: 1.5rem;
		}
	}
</style>
