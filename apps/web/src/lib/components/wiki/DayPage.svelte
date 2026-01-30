<!--
	DayPage.svelte

	Renders a day page with:
	- Data: citations, entities, context vector (always additive)
	- Timeline: events with user-editable overrides
	- Movement: location track for the day
	- Data Sources: ontology records grouped by type
-->

<script lang="ts">
	import { browser } from "$app/environment";
	import type { DayPage as DayPageType } from "$lib/wiki/types";
	import { flattenLinkedEntities } from "$lib/wiki/types";
	import { getDaySources, type DaySourceApi } from "$lib/wiki/api";
	import ContextVector from "./ContextVector.svelte";
	import DayTimeline from "./DayTimeline.svelte";
	import WikiRightRail from "./WikiRightRail.svelte";
	import Icon from "$lib/components/Icon.svelte";
	import MovementMap from "$lib/components/timeline/MovementMap.svelte";
	import { sampleLocationTrack, timeNsToMs } from "$lib/dev/sampleLocationTrack";

	interface Props {
		page: DayPageType;
	}

	let { page }: Props = $props();

	function formatDate(date: Date, dayOfWeek: string): string {
		return `${dayOfWeek}, ${date.toLocaleDateString("en-US", {
			month: "long",
			day: "numeric",
			year: "numeric",
		})}`;
	}

	function formatTimezoneDisplay(
		startTz: string | null,
		endTz: string | null,
	): string | null {
		if (!startTz) return null;

		// Extract city name from IANA timezone (e.g., "America/New_York" -> "New York")
		const formatTz = (tz: string) => {
			const parts = tz.split("/");
			return parts[parts.length - 1].replace(/_/g, " ");
		};

		// If both exist and different, show span
		if (endTz && endTz !== startTz) {
			return `00:00 ${formatTz(startTz)} → 24:00 ${formatTz(endTz)}`;
		}

		// Otherwise just show the timezone
		return formatTz(startTz);
	}

	// Flatten linked entities for entity display
	const allLinkedPages = $derived(flattenLinkedEntities(page.linkedEntities));

	// Timezone display
	const timezoneDisplay = $derived(
		formatTimezoneDisplay(page.startTimezone, page.endTimezone),
	);

	// ─────────────────────────────────────────────────────────────────────────
	// Movement map (Day page)
	// ─────────────────────────────────────────────────────────────────────────
	type TimelineDayLocationChunk = {
		type: "location";
		start_time: string;
		end_time: string;
		place_name: string | null;
		latitude: number;
		longitude: number;
	};

	type TimelineDayView = {
		date: string;
		chunks: Array<
			| TimelineDayLocationChunk
			| { type: "transit" }
			| { type: "missing_data" }
		>;
	};

	let movementStops = $state<TimelineDayLocationChunk[]>([]);
	let movementLoading = $state(false);

	async function loadMovement(dateSlug: string) {
		if (!browser) return;
		movementLoading = true;
		try {
			const res = await fetch(`/api/timeline/day/${dateSlug}`);
			if (!res.ok) throw new Error(`timeline day api ${res.status}`);
			const dayView = (await res.json()) as TimelineDayView;
			movementStops = dayView.chunks.filter(
				(c): c is TimelineDayLocationChunk =>
					c?.type === "location" &&
					typeof (c as any).latitude === "number" &&
					typeof (c as any).longitude === "number",
			);
		} catch {
			movementStops = [];
		} finally {
			movementLoading = false;
		}
	}

	$effect(() => {
		// Day pages are date-slugged (YYYY-MM-DD)
		if (browser && page?.slug) loadMovement(page.slug);
	});

	// Use sample Rome location track for demo date (2025-12-10)
	const sampleStopPoints = $derived(
		sampleLocationTrack.map((p) => ({
			lat: p.lat,
			lng: p.lng,
			label: undefined,
			timeMs: timeNsToMs(p.timeNs),
		})),
	);

	const stopPoints = $derived(
		movementStops.length > 0
			? movementStops.map((c) => ({
					lat: c.latitude,
					lng: c.longitude,
					label: c.place_name ?? undefined,
					timeMs: Date.parse(c.start_time),
				}))
			: page.slug === "2025-12-10"
				? sampleStopPoints
				: [],
	);

	// Check if we have real location data
	const hasLocationData = $derived(stopPoints.length >= 2);

	// For the demo, only show first/last as stop markers (not every point)
	const stopMarkers = $derived(
		stopPoints.length >= 2
			? [stopPoints[0], stopPoints[stopPoints.length - 1]]
			: stopPoints,
	);

	// ─────────────────────────────────────────────────────────────────────────
	// Data Sources (ontology records for the day)
	// ─────────────────────────────────────────────────────────────────────────
	let dataSources = $state<DaySourceApi[]>([]);
	let sourcesLoading = $state(false);

	// Mock data sources for demo (2025-12-10)
	const MOCK_DATA_SOURCES: DaySourceApi[] = [
		{ source_type: "sleep", id: "sleep-1", timestamp: "2025-12-10T06:42:00Z", label: "Sleep ended", preview: "6h 18m total, 2 REM cycles" },
		{ source_type: "calendar", id: "cal-1", timestamp: "2025-12-10T10:00:00Z", label: "Architecture Review", preview: "With engineering team" },
		{ source_type: "calendar", id: "cal-2", timestamp: "2025-12-10T20:30:00Z", label: "Catch up", preview: "With Sarah Chen" },
		{ source_type: "location", id: "loc-1", timestamp: "2025-12-10T08:35:00Z", label: "Arrived at Office", preview: "Via transit, 20 min" },
		{ source_type: "location", id: "loc-2", timestamp: "2025-12-10T17:30:00Z", label: "Arrived at Gym", preview: null },
		{ source_type: "transaction", id: "tx-1", timestamp: "2025-12-10T12:07:00Z", label: "Café Roma", preview: "$14.20" },
		{ source_type: "workout", id: "wo-1", timestamp: "2025-12-10T17:30:00Z", label: "Functional Training", preview: "42 min, 156 avg HR" },
		{ source_type: "message:imessage", id: "msg-1", timestamp: "2025-12-10T14:12:00Z", label: "Sarah Chen", preview: "7 messages exchanged" },
	];

	async function loadDataSources(dateSlug: string) {
		if (!browser) return;
		sourcesLoading = true;
		try {
			dataSources = await getDaySources(dateSlug);
			// Fall back to mock data for demo
			if (dataSources.length === 0 && dateSlug === "2025-12-10") {
				dataSources = MOCK_DATA_SOURCES;
			}
		} catch {
			// Fall back to mock data for demo
			if (dateSlug === "2025-12-10") {
				dataSources = MOCK_DATA_SOURCES;
			} else {
				dataSources = [];
			}
		} finally {
			sourcesLoading = false;
		}
	}

	$effect(() => {
		if (browser && page?.slug) loadDataSources(page.slug);
	});

	// Group sources by type for display
	const groupedSources = $derived(() => {
		const groups: Record<string, DaySourceApi[]> = {};
		for (const source of dataSources) {
			const type = source.source_type;
			if (!groups[type]) groups[type] = [];
			groups[type].push(source);
		}
		return groups;
	});

	// Get icon for source type
	function getSourceIcon(sourceType: string): string {
		const iconMap: Record<string, string> = {
			calendar: "ri:calendar-line",
			email: "ri:mail-line",
			location: "ri:map-pin-line",
			workout: "ri:run-line",
			sleep: "ri:zzz-line",
			transaction: "ri:bank-card-line",
		};
		// Handle message:platform types
		if (sourceType.startsWith("message:")) {
			return "ri:message-3-line";
		}
		return iconMap[sourceType] ?? "ri:database-2-line";
	}

	// Get display name for source type
	function getSourceTypeName(sourceType: string): string {
		const nameMap: Record<string, string> = {
			calendar: "Calendar",
			email: "Email",
			location: "Location",
			workout: "Workout",
			sleep: "Sleep",
			transaction: "Transaction",
		};
		if (sourceType.startsWith("message:")) {
			const platform = sourceType.split(":")[1];
			return `Message (${platform})`;
		}
		return nameMap[sourceType] ?? sourceType;
	}

	// Format time for display
	function formatSourceTime(timestamp: string): string {
		const date = new Date(timestamp);
		return date.toLocaleTimeString("en-US", {
			hour: "numeric",
			minute: "2-digit",
			hour12: true,
		});
	}

	// Build content string for TOC that includes page structure headings
	const fullContent = $derived(`## Timeline

## Movement

## Data Sources

## Entities
`);
</script>

<div class="day-page-layout">
	<article class="day-article wiki-article">
		<div class="day-content">
			<!-- Header -->
			<header class="day-header">
				<h1 class="day-title">
					{formatDate(page.date, page.dayOfWeek)}
				</h1>
				{#if timezoneDisplay}
					<div class="day-timezone">Timezone: {timezoneDisplay}</div>
				{/if}
				<div class="day-meta">
					<ContextVector contextVector={page.contextVector} />
				</div>
			</header>

			<hr class="divider" />

			<!-- Timeline -->
			<section class="section" id="timeline">
				<h2 class="section-title">Timeline</h2>
				<DayTimeline events={page.events} />
			</section>

			<!-- Movement -->
			<section class="section" id="movement">
				<h2 class="section-title">Movement</h2>
				{#if movementLoading}
					<div class="movement-loading">Loading…</div>
				{:else if hasLocationData}
					<MovementMap
						track={stopPoints}
						stops={stopMarkers}
						height={240}
					/>
				{:else}
					<div class="movement-empty">
						<div class="movement-empty-map">
							<MovementMap
								track={[]}
								stops={[]}
								height={200}
							/>
						</div>
						<div class="movement-empty-overlay">
							<Icon icon="ri:map-pin-line" class="movement-empty-icon"/>
							<span class="movement-empty-text">No location data for this day</span>
						</div>
					</div>
				{/if}
			</section>

			<!-- Data Sources -->
			<section class="section" id="data-sources">
				<h2 class="section-title">Data Sources</h2>
				{#if sourcesLoading}
					<p class="empty-placeholder">Loading sources...</p>
				{:else if dataSources.length > 0}
					<div class="sources-list">
						{#each Object.entries(groupedSources()) as [sourceType, sources]}
							<div class="source-group">
								<div class="source-group-header">
									<Icon icon={getSourceIcon(sourceType)} class="source-group-icon"/>
									<span class="source-group-name">{getSourceTypeName(sourceType)}</span>
									<span class="source-group-count">{sources.length}</span>
								</div>
								<ul class="source-items">
									{#each sources as source}
										<li class="source-item">
											<span class="source-time">{formatSourceTime(source.timestamp)}</span>
											<span class="source-label">{source.label}</span>
											{#if source.preview}
												<span class="source-preview">{source.preview}</span>
											{/if}
										</li>
									{/each}
								</ul>
							</div>
						{/each}
					</div>
				{:else}
					<p class="empty-placeholder">No data sources for this day</p>
				{/if}
			</section>

			<!-- Entities -->
			<section class="section" id="entities">
				<h2 class="section-title">Entities</h2>
				{#if allLinkedPages.length > 0}
					<ul class="footer-list">
						{#each allLinkedPages as entity}
							<li>
								<a
									href="/wiki/{entity.pageSlug}"
									class="footer-link"
								>
									<span class="link-text"
										>{entity.displayName}</span
									>
								</a>
							</li>
						{/each}
					</ul>
				{:else}
					<p class="empty-placeholder">No entities</p>
				{/if}
			</section>
		</div>
	</article>

	<WikiRightRail content={fullContent}>
		{#snippet metadata()}
			<div class="sidebar-meta">
				<div class="meta-title">{page.dayOfWeek}</div>
				<div class="meta-date">
					{page.date.toLocaleDateString("en-US", {
						month: "long",
						day: "numeric",
						year: "numeric",
					})}
				</div>
				<div class="meta-stats">
					<span class="stat"
						>{page.events.filter((e) => !e.isUnknown).length} events</span
					>
					<span class="stat-sep">·</span>
					<span class="stat"
						>{dataSources.length} sources</span
					>
				</div>
			</div>
		{/snippet}
	</WikiRightRail>
</div>

<style>
	.day-page-layout {
		display: flex;
		height: 100%;
		width: 100%;
		overflow: hidden;
	}

	.day-article {
		flex: 1;
		min-width: 0;
		overflow-y: auto;
		scrollbar-width: none;
		-ms-overflow-style: none;
		padding: 2rem;
	}

	.day-article::-webkit-scrollbar {
		display: none;
	}

	.day-content {
		max-width: 48rem;
		margin: 0 auto;
		padding-top: 2rem;
		padding-bottom: 4rem;
	}

	/* Header */
	.day-header {
		margin-bottom: 1rem;
	}

	.day-title {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.75rem;
		font-weight: 400;
		color: var(--color-foreground);
		margin: 0 0 0.5rem;
		line-height: 1.3;
	}

	.day-timezone {
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
		margin-top: 0.25rem;
	}

	.day-meta {
		margin-top: 0.5rem;
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

	/* Data Sources */
	.sources-list {
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}

	.source-group {
		border: 1px solid var(--color-border);
		border-radius: 6px;
		overflow: hidden;
	}

	.source-group-header {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.5rem 0.75rem;
		background: var(--color-surface-raised);
		border-bottom: 1px solid var(--color-border);
	}

	.source-group-icon {
		font-size: 1rem;
		color: var(--color-foreground-muted);
	}

	.source-group-name {
		font-size: 0.8125rem;
		font-weight: 500;
		color: var(--color-foreground);
		flex: 1;
	}

	.source-group-count {
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
		background: var(--color-surface);
		padding: 0.125rem 0.5rem;
		border-radius: 9999px;
	}

	.source-items {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.source-item {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
		padding: 0.375rem 0.75rem;
		border-bottom: 1px solid var(--color-border);
		font-size: 0.8125rem;
	}

	.source-item:last-child {
		border-bottom: none;
	}

	.source-time {
		flex-shrink: 0;
		width: 5rem;
		color: var(--color-foreground-muted);
		font-size: 0.75rem;
	}

	.source-label {
		flex: 1;
		color: var(--color-foreground);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.source-preview {
		flex-shrink: 0;
		max-width: 12rem;
		color: var(--color-foreground-subtle);
		font-size: 0.75rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	/* Movement map */
	.movement-loading {
		font-size: 0.875rem;
		color: var(--color-foreground-subtle);
		margin-bottom: 0.5rem;
	}

	.movement-empty {
		position: relative;
		border-radius: 8px;
		overflow: hidden;
	}

	.movement-empty-map {
		opacity: 0.4;
		filter: grayscale(0.5);
	}

	.movement-empty-overlay {
		position: absolute;
		inset: 0;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 0.5rem;
		background: linear-gradient(
			to bottom,
			transparent 0%,
			color-mix(in srgb, var(--color-surface) 60%, transparent) 100%
		);
	}

	.movement-empty-icon {
		font-size: 2rem;
		color: var(--color-foreground-subtle);
		opacity: 0.6;
	}

	.movement-empty-text {
		font-size: 0.875rem;
		color: var(--color-foreground-muted);
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

	.meta-date {
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
		.day-page-layout {
			flex-direction: column;
		}

		.day-article {
			padding: 1rem;
		}

		.day-title {
			font-size: 1.5rem;
		}
	}
</style>
