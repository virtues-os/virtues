<!--
	PlacePage.svelte

	Renders a place page - a location with meaning in your life.
	Includes a small map, visit history, and associated people/activities.
-->

<script lang="ts">
	import { subjectHref } from "$lib/wiki/links";
	import type { WikiPlaceApi } from "$lib/wiki/api";
	import MovementMap from "$lib/components/timeline/MovementMap.svelte";
	import EntityArticleSection from "./EntityArticleSection.svelte";
	import SubjectBacklinks from "./SubjectBacklinks.svelte";
	import NotesRail from "./NotesRail.svelte";
	import EntityRecordsSection from "./EntityRecordsSection.svelte";
	import Markdown from "$lib/components/Markdown.svelte";
	import { updatePlace } from "$lib/wiki/api";

	interface Props {
		/** The wire shape. See PersonPage for why the converter is gone. */
		page: WikiPlaceApi;
	}

	let { page }: Props = $props();

	// `seen_count`/`first_seen`/`last_seen` are WIRE fields, counted live from
	// `wiki_refs` — the columns of those names were dropped in migration 0025
	// after reading "Total visits: 0" on a place visited weekly.
	const coordinates = $derived(
		page.latitude && page.longitude ? { lat: page.latitude, lng: page.longitude } : null
	);
	const firstVisit = $derived(page.first_seen ? new Date(page.first_seen) : null);
	const lastVisit = $derived(page.last_seen ? new Date(page.last_seen) : null);

	// "Don't record here": the phone keeps no audio while you are inside this
	// place. The flag lives on the place row; the phone caches the muted
	// places when the app opens and honors them offline, so a flip here
	// reaches the mic the next time the app is opened, not this instant.
	let muted = $state(page.is_audio_muted ?? false);
	let muteFailed = $state<string | null>(null);
	$effect(() => {
		muted = page.is_audio_muted ?? false;
	});

	async function toggleMuted() {
		const next = !muted;
		muted = next;
		muteFailed = null;
		const ok = await updatePlace(page.id, { is_audio_muted: next });
		if (!ok) {
			muted = !next;
			muteFailed = "Could not change that";
		}
	}

	function formatDate(date: Date): string {
		return date.toLocaleDateString("en-US", {
			month: "long",
			day: "numeric",
			year: "numeric",
		});
	}

	/**
	 * The badge over a place's name, from its stored category.
	 *
	 * This was two maps: the converter turned `cafe` into `third-place`, and
	 * this function turned `third-place` into "Third Place". One map, one
	 * hop. A category with no entry keeps its own word rather than becoming
	 * "Other", which was losing information to render a vaguer label.
	 */
	function placeLabel(category: string | null): string {
		const labels: Record<string, string> = {
			home: "Home",
			work: "Work",
			gym: "Third Place",
			cafe: "Third Place",
			library: "Third Place",
			airport: "Transit",
			station: "Transit",
			travel: "Travel",
		};
		if (!category) return "Place";
		const key = category.toLowerCase();
		return labels[key] ?? category.charAt(0).toUpperCase() + category.slice(1);
	}

	// Map data for single location
	const stopPoints = $derived(
		coordinates
			? [
					{
						lat: coordinates.lat,
						lng: coordinates.lng,
						label: page.name,
						timeMs: Date.now(),
					},
				]
			: [],
	);

</script>

<div class="page-layout">
	<article class="wiki-article">
		<div class="page-content">
			<!-- Header -->
			<header class="page-header">
				<h1 class="page-title">{page.name}</h1>
				<!-- A subtitle and a city were read here and never set: two more
				     optional fields on a page type nothing filled. The address
				     below is what the record actually holds. -->
				<div class="page-meta">
					<span class="meta-item place-badge">{placeLabel(page.category)}</span>
				</div>
			</header>

			<hr class="divider" />

			<!-- The article: machine-written prose about this entity -->
			<section class="section" id="article">
				<EntityArticleSection
					article={page.article}
					articleUpdatedAt={page.article_updated_at
						? new Date(page.article_updated_at)
						: null}
					name={page.name}
									subjectType="place"
					subjectId={page.id}
					maintained={page.article_maintained}
					onChanged={() => location.reload()}
				/>
			</section>

			<!-- Map -->
			{#if coordinates}
				<section class="section" id="map">
					<MovementMap
						track={stopPoints}
						stops={stopPoints}
						height={200}
					/>
				</section>
			{/if}

			<!-- The record: every data point that references this place -->
			<section class="section" id="the-record">
				<h2 class="section-title">The record</h2>
				<EntityRecordsSection entityId={page.id} />
			</section>

			<SubjectBacklinks subjectType="place" subjectId={page.id} />
			<NotesRail subjectType="place" subjectId={page.id} />

			<!-- Notes: the user's own writing -->
			{#if page.content}
				<section class="section" id="notes">
					<div class="notes-content">
						<Markdown content={page.content} refVariant="quiet" />
					</div>
				</section>
			{/if}

			<!-- Location Details -->
			{#if page.address || coordinates}
				<section class="section" id="location">
					<h2 class="section-title">Location</h2>
					<dl class="info-list">
						{#if page.address}
							<div class="info-item">
								<dt>Address</dt>
								<dd>{page.address}</dd>
							</div>
						{/if}
						{#if coordinates}
							<div class="info-item">
								<dt>Coordinates</dt>
								<dd class="coords">
									{coordinates.lat.toFixed(6)}, {coordinates.lng.toFixed(6)}
								</dd>
							</div>
						{/if}
					</dl>
				</section>
			{/if}

			<!-- Recording: the one thing a place can ask of the phone -->
			{#if coordinates}
				<section class="section" id="recording">
					<h2 class="section-title">Recording</h2>
					<dl class="info-list">
						<div class="info-item">
							<dt>Microphone</dt>
							<dd>
								<button
									type="button"
									class="linkish"
									title={muted
										? "The phone keeps no audio while you are here. Turn this off to record here again."
										: "Ask the phone to keep no audio while you are here. The mic stays on; nothing is kept."}
									onclick={toggleMuted}
								>
									{muted ? "Not recording here" : "Don't record here"}
								</button>
								{#if muteFailed}
									<span class="mute-failed">{muteFailed}</span>
								{/if}
							</dd>
						</div>
					</dl>
				</section>
			{/if}

			<!-- Visit History -->
			{#if firstVisit || lastVisit || page.seen_count}
				<section class="section" id="visit-history">
					<h2 class="section-title">Visit History</h2>
					<dl class="info-list">
						{#if firstVisit}
							<div class="info-item">
								<dt>First visit</dt>
								<dd>{formatDate(firstVisit)}</dd>
							</div>
						{/if}
						{#if lastVisit}
							<div class="info-item">
								<dt>Last visit</dt>
								<dd>{formatDate(lastVisit)}</dd>
							</div>
						{/if}
						{#if page.seen_count}
							<div class="info-item">
								<dt>Total visits</dt>
								<dd>{page.seen_count}</dd>
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

	.linkish {
		background: none;
		border: 0;
		padding: 0;
		font: inherit;
		color: var(--color-foreground);
		text-decoration: underline;
		text-underline-offset: 0.15em;
		text-decoration-color: var(--color-border);
		cursor: pointer;
	}

	.linkish:hover {
		text-decoration-color: currentColor;
	}

	.mute-failed {
		margin-left: 0.5rem;
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
	}

	.coords {
		font-family: var(--font-mono, monospace);
		font-size: 0.8125rem;
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
