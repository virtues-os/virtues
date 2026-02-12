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
	import { getDaySources, updateDay, type DaySourceApi, type WikiDayApi } from "$lib/wiki/api";
	import { getLocalDateSlug } from "$lib/utils/dateUtils";
	import { spaceStore } from "$lib/stores/space.svelte";
	import type { ContextVector as ContextVectorType } from "$lib/wiki/types";
	import ContextVector from "./ContextVector.svelte";
	import DayTimeline from "./DayTimeline.svelte";
	import WikiRightRail from "./WikiRightRail.svelte";
	import Icon from "$lib/components/Icon.svelte";
	import { slide } from "svelte/transition";
	import Modal from "$lib/components/Modal.svelte";
	import MovementMap from "$lib/components/timeline/MovementMap.svelte";
	import { sampleLocationTrack, timeNsToMs } from "$lib/dev/sampleLocationTrack";

	interface Props {
		page: DayPageType;
	}

	let { page }: Props = $props();

	/** YYYY-MM-DD string for API calls */
	const dateSlug = $derived(() => getLocalDateSlug(page.date));

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
	// Week navigation
	// ─────────────────────────────────────────────────────────────────────────
	const DAY_LETTERS = ["S", "M", "T", "W", "T", "F", "S"];

	let weekOffset = $state(0);

	// Reset weekOffset when page.date changes (navigating between days)
	$effect(() => {
		page.date; // track
		weekOffset = 0;
	});

	// Get the Sunday that starts the week containing page.date, then apply offset
	const weekDays = $derived(() => {
		const ref = new Date(page.date);
		ref.setDate(ref.getDate() - ref.getDay() + weekOffset * 7);
		const days: Date[] = [];
		for (let i = 0; i < 7; i++) {
			const d = new Date(ref);
			d.setDate(ref.getDate() + i);
			days.push(d);
		}
		return days;
	});

	const currentDateSlug = $derived(getLocalDateSlug(page.date));
	const todaySlug = $derived(getLocalDateSlug(new Date()));

	function navigateToDay(date: Date) {
		const slug = getLocalDateSlug(date);
		if (slug === currentDateSlug) return;
		spaceStore.openTabFromRoute(`/day/day_${slug}`);
	}

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
	let movementLoadVersion = 0;

	async function loadMovement(dateSlug: string) {
		if (!browser) return;
		const version = ++movementLoadVersion;
		movementLoading = true;
		try {
			const res = await fetch(`/api/timeline/day/${dateSlug}`);
			if (version !== movementLoadVersion) return; // stale
			if (!res.ok) throw new Error(`timeline day api ${res.status}`);
			const dayView = (await res.json()) as TimelineDayView;
			if (version !== movementLoadVersion) return; // stale
			movementStops = dayView.chunks.filter(
				(c): c is TimelineDayLocationChunk =>
					c?.type === "location" &&
					typeof (c as any).latitude === "number" &&
					typeof (c as any).longitude === "number",
			);
		} catch {
			if (version !== movementLoadVersion) return;
			movementStops = [];
		} finally {
			if (version === movementLoadVersion) movementLoading = false;
		}
	}

	$effect(() => {
		if (browser && page?.date) loadMovement(dateSlug());
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
			: dateSlug() === "2025-12-10"
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
	let sourcesLoadVersion = 0;

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
		const version = ++sourcesLoadVersion;
		sourcesLoading = true;
		try {
			const result = await getDaySources(dateSlug);
			if (version !== sourcesLoadVersion) return; // stale
			dataSources = result;
			// Fall back to mock data for demo
			if (dataSources.length === 0 && dateSlug === "2025-12-10") {
				dataSources = MOCK_DATA_SOURCES;
			}
		} catch {
			if (version !== sourcesLoadVersion) return;
			// Fall back to mock data for demo
			if (dateSlug === "2025-12-10") {
				dataSources = MOCK_DATA_SOURCES;
			} else {
				dataSources = [];
			}
		} finally {
			if (version === sourcesLoadVersion) sourcesLoading = false;
		}
	}

	$effect(() => {
		if (browser && page?.date) loadDataSources(dateSlug());
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

	// Per-group expand state: show 3 by default, expand to show all
	const SOURCE_PREVIEW_LIMIT = 3;
	let expandedGroups = $state<Set<string>>(new Set());

	function toggleGroupExpand(sourceType: string) {
		const next = new Set(expandedGroups);
		if (next.has(sourceType)) {
			next.delete(sourceType);
		} else {
			next.add(sourceType);
		}
		expandedGroups = next;
	}

	// Get icon for source type
	function getSourceIcon(sourceType: string): string {
		const iconMap: Record<string, string> = {
			calendar: "ri:calendar-line",
			email: "ri:mail-line",
			location: "ri:map-pin-line",
			workout: "ri:run-line",
			sleep: "ri:zzz-line",
			transaction: "ri:bank-card-line",
			transcription: "ri:mic-line",
			steps: "ri:footprint-line",
			chat: "ri:chat-3-line",
			page: "ri:file-text-line",
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
			location: "Places",
			workout: "Workout",
			sleep: "Sleep",
			transaction: "Transaction",
			transcription: "Voice Transcription",
			steps: "Steps",
			chat: "Chats",
			page: "Pages",
		};
		if (sourceType.startsWith("message:")) {
			const platform = sourceType.split(":")[1];
			return `Message (${platform})`;
		}
		return nameMap[sourceType] ?? sourceType;
	}

	// Format time for display in the user's local timezone
	function formatSourceTime(timestamp: string): string {
		const date = new Date(timestamp);
		return date.toLocaleTimeString("en-US", {
			hour: "numeric",
			minute: "2-digit",
			hour12: true,
		});
	}

	// ─────────────────────────────────────────────────────────────────────────
	// Daily Summary (autobiography generation)
	// ─────────────────────────────────────────────────────────────────────────
	let vixExpanded = $state(false);
	let summaryGenerating = $state(false);
	let summaryText = $state(page.autobiography || "");
	let editingAutobiography = $state(false);

	// Sync summaryText when page prop changes (navigating between days)
	$effect(() => {
		summaryText = page.autobiography || "";
		editingAutobiography = false;
	});

	function startEditingAutobiography() {
		editingAutobiography = true;
	}

	async function saveAutobiography(newText: string) {
		const trimmed = newText.trim();
		if (trimmed === summaryText) {
			editingAutobiography = false;
			return;
		}
		try {
			await updateDay(dateSlug(), { autobiography: trimmed, last_edited_by: "user" });
			summaryText = trimmed;
		} catch (e) {
			console.error("Failed to save autobiography:", e);
		} finally {
			editingAutobiography = false;
		}
	}

	function handleAutobiographyBlur(e: FocusEvent) {
		const target = e.currentTarget as HTMLElement;
		saveAutobiography(target.textContent || "");
	}

	function handleAutobiographyKeydown(e: KeyboardEvent) {
		if (e.key === "Escape") {
			editingAutobiography = false;
		}
	}

	function parseContextVector(raw: string | null): ContextVectorType | null {
		if (!raw) return null;
		try {
			return JSON.parse(raw);
		} catch {
			return null;
		}
	}

	let showRegenerateConfirm = $state(false);

	function handleGenerateClick() {
		if (summaryText) {
			showRegenerateConfirm = true;
		} else {
			generateSummary();
		}
	}

	function confirmRegenerate() {
		showRegenerateConfirm = false;
		generateSummary();
	}

	async function generateSummary() {
		summaryGenerating = true;
		try {
			const res = await fetch(`/api/wiki/day/${dateSlug()}/summary`, { method: "POST" });
			if (!res.ok) throw new Error(`Summary generation failed: ${res.status}`);
			const updated: WikiDayApi = await res.json();
			summaryText = updated.autobiography || "";

			// Update context vector and chaos score from the response
			const cv = parseContextVector(updated.context_vector);
			if (cv) {
				page.contextVector = cv;
			}
			page.chaosScore = updated.chaos_score ?? null;
			page.entropyCalibrationDays = updated.entropy_calibration_days ?? null;
		} catch (e) {
			console.error("Summary generation failed:", e);
		} finally {
			summaryGenerating = false;
		}
	}

	// Build content string for TOC that includes page structure headings
	const fullContent = $derived(`## Autobiography

## Timeline

## Movement

## Entities

## Activity
`);
</script>

<div class="day-page-layout">
	<article class="day-article wiki-article">
		<div class="day-content">
			<!-- Header -->
			<header class="day-header">
				<div class="day-header-row">
					<h1 class="day-title">
						{formatDate(page.date, page.dayOfWeek)}
					</h1>
					<button
						class="generate-day-btn"
						onclick={handleGenerateClick}
						disabled={summaryGenerating}
						type="button"
						title={summaryText ? "Regenerate day" : "Generate day"}
					>
						{#if summaryGenerating}
							<Icon icon="ri:loader-4-line" class="spin-icon" />
							<span>Generating...</span>
						{:else}
							<Icon icon="ri:refresh-line" />
							<span>{summaryText ? 'Regenerate' : 'Generate'}</span>
						{/if}
					</button>
				</div>
				{#if timezoneDisplay}
					<div class="day-timezone">Timezone: {timezoneDisplay}</div>
				{/if}
			</header>

			<!-- Week Navigation -->
			<nav class="week-nav">
				<button
					class="week-nav-chevron"
					onclick={() => weekOffset--}
					type="button"
					aria-label="Previous week"
				>
					<Icon icon="ri:arrow-left-s-line" />
				</button>
				<div class="week-days">
					{#each weekDays() as day, i}
						{@const slug = getLocalDateSlug(day)}
						{@const isCurrent = slug === currentDateSlug}
						{@const isToday = slug === todaySlug}
						<button
							class="week-day"
							class:current={isCurrent}
							class:today={isToday}
							onclick={() => navigateToDay(day)}
							type="button"
							aria-label={day.toLocaleDateString("en-US", { weekday: "long", month: "long", day: "numeric" })}
						>
							<span class="week-day-letter">{DAY_LETTERS[i]}</span>
							<span class="week-day-number">{day.getDate()}</span>
						</button>
					{/each}
				</div>
				<button
					class="week-nav-chevron"
					onclick={() => weekOffset++}
					type="button"
					aria-label="Next week"
				>
					<Icon icon="ri:arrow-right-s-line" />
				</button>
			</nav>

			<!-- Metrics -->
			<div class="day-metrics">
				<ContextVector contextVector={page.contextVector} />
				{#if page.entropyCalibrationDays != null}
					{@const calibDays = page.entropyCalibrationDays}
					{@const isCalibrated = calibDays >= 7}
					{@const hasScore = page.chaosScore != null && isCalibrated}
					<div class="vix-section">
						<button class="vix-toggle" onclick={() => (vixExpanded = !vixExpanded)}>
							{#if hasScore}
								<span class="vix-toggle-label">Entropy · {Math.round(page.chaosScore! * 100)}</span>
							{:else}
								<span class="vix-toggle-label">Entropy · Calibrating ({calibDays}/7 days)</span>
							{/if}
							<Icon icon={vixExpanded ? "ri:arrow-up-s-line" : "ri:arrow-down-s-line"} />
						</button>
						{#if vixExpanded}
							<div class="vix-details" transition:slide={{ duration: 200 }}>
								{#if hasScore}
									<div class="vix-bar">
										<div class="vix-fill" style="width: {Math.round(page.chaosScore! * 100)}%"></div>
									</div>
									<p class="metric-description">How unpredictable your day was. Low entropy means routine and structure; high entropy means novelty and disorder.</p>
								{:else}
									<div class="vix-bar">
										<div class="vix-fill vix-fill-calibrating" style="width: {Math.round((calibDays / 7) * 100)}%"></div>
									</div>
									<p class="metric-description">Entropy needs ~7 days of data to establish your baseline. {calibDays === 0 ? 'This is your first day.' : `${calibDays} day${calibDays === 1 ? '' : 's'} recorded so far.`}</p>
								{/if}
							</div>
						{/if}
					</div>
				{/if}
			</div>

			<hr class="divider" />

			<!-- Autobiography -->
			<section class="section autobiography-section">
				<div class="autobiography-header">
					<h2 class="section-title">Autobiography</h2>
					{#if summaryText && !editingAutobiography}
						<button
							class="autobiography-edit-btn"
							onclick={startEditingAutobiography}
							type="button"
							title="Edit autobiography"
						>
							<Icon icon="ri:pencil-line" />
						</button>
					{/if}
				</div>
				{#if editingAutobiography}
					<!-- svelte-ignore a11y_no_static_element_interactions -->
					<div
						class="autobiography-text autobiography-editable"
						contenteditable="true"
						onblur={handleAutobiographyBlur}
						onkeydown={handleAutobiographyKeydown}
						role="textbox"
						aria-label="Edit autobiography"
					>{summaryText}</div>
				{:else if summaryText}
					<p class="autobiography-text">{summaryText}</p>
				{:else}
					<p class="empty-placeholder">No autobiography yet — generate one from today's data.</p>
				{/if}
			</section>

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
					<p class="empty-placeholder">No location data for this day</p>
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
									href="/wiki/{entity.pageId}"
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

			<!-- Activity -->
			<section class="section" id="activity">
				<h2 class="section-title">Activity</h2>
				{#if sourcesLoading}
					<p class="empty-placeholder">Loading...</p>
				{:else if dataSources.length > 0}
					<div class="sources-list">
						{#each Object.entries(groupedSources()) as [sourceType, sources]}
							{@const isExpanded = expandedGroups.has(sourceType)}
							{@const visibleSources = isExpanded ? sources : sources.slice(0, SOURCE_PREVIEW_LIMIT)}
							{@const hasMore = sources.length > SOURCE_PREVIEW_LIMIT}
							<div class="source-group">
								<div class="source-group-header">
									<Icon icon={getSourceIcon(sourceType)} class="source-group-icon"/>
									<span class="source-group-name">{getSourceTypeName(sourceType)}</span>
									<span class="source-group-count">{sources.length}</span>
								</div>
								<ul class="source-items">
									{#each visibleSources as source}
										<li class="source-item">
											<span class="source-time">{formatSourceTime(source.timestamp)}</span>
											<span class="source-label">{source.label}</span>
											{#if source.preview}
												<span class="source-preview">{source.preview}</span>
											{/if}
										</li>
									{/each}
								</ul>
								{#if hasMore}
									<button
										class="source-show-more"
										onclick={() => toggleGroupExpand(sourceType)}
										type="button"
									>
										{isExpanded ? "Show less" : `Show all ${sources.length}`}
									</button>
								{/if}
							</div>
						{/each}
					</div>
				{:else}
					<p class="empty-placeholder">No activity for this day</p>
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

<!-- Regenerate confirmation modal -->
<Modal
	open={showRegenerateConfirm}
	onClose={() => showRegenerateConfirm = false}
	title="Regenerate Day"
	width="sm"
>
	<p class="regenerate-confirm-text">
		This will regenerate the autobiography, completeness, and entropy scores. Any manual edits will be overwritten.
	</p>
	{#snippet footer()}
		<button class="modal-btn modal-btn-secondary" onclick={() => showRegenerateConfirm = false}>
			Cancel
		</button>
		<button class="modal-btn modal-btn-primary" onclick={confirmRegenerate}>
			Regenerate
		</button>
	{/snippet}
</Modal>

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

	.day-header-row {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 1rem;
	}

	.day-title {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.75rem;
		font-weight: 400;
		color: var(--color-foreground);
		margin: 0 0 0.5rem;
		line-height: 1.3;
	}

	.generate-day-btn {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		background: none;
		border: none;
		padding: 0;
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
		cursor: pointer;
		white-space: nowrap;
		flex-shrink: 0;
	}
	.generate-day-btn:hover {
		color: var(--color-foreground-muted);
	}
	.generate-day-btn:disabled {
		opacity: 0.5;
		cursor: default;
	}

	/* Week navigation */
	.week-nav {
		display: flex;
		align-items: center;
		gap: 0.25rem;
		margin-bottom: 0.75rem;
	}

	.week-nav-chevron {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 24px;
		height: 24px;
		background: none;
		border: none;
		padding: 0;
		color: var(--color-foreground-subtle);
		cursor: pointer;
		border-radius: 4px;
		flex-shrink: 0;
	}
	.week-nav-chevron:hover {
		color: var(--color-foreground-muted);
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
	}

	.week-days {
		display: flex;
		gap: 0.125rem;
		flex: 1;
		justify-content: space-between;
	}

	.week-day {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.125rem;
		padding: 0.25rem 0.5rem;
		background: none;
		border: none;
		border-radius: 6px;
		cursor: pointer;
		color: var(--color-foreground-subtle);
		transition: all 0.1s ease;
		min-width: 2rem;
	}
	.week-day:hover {
		color: var(--color-foreground-muted);
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
	}
	.week-day.current {
		color: var(--color-foreground);
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
	}
	.week-day.today:not(.current) {
		color: var(--color-foreground-muted);
	}
	.week-day.today::after {
		content: "";
		display: block;
		width: 3px;
		height: 3px;
		border-radius: 50%;
		background: var(--color-foreground-muted);
		margin-top: -0.125rem;
	}

	.week-day-letter {
		font-size: 0.625rem;
		text-transform: uppercase;
		letter-spacing: 0.02em;
	}

	.week-day-number {
		font-size: 0.8125rem;
		font-weight: 500;
		line-height: 1;
	}

	.day-timezone {
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
		margin-top: 0.25rem;
	}

	/* Metrics block (completeness + entropy) */
	.day-metrics {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		margin-top: 0.75rem;
	}

	/* VIX / Entropy index */
	.vix-section {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		width: 100%;
	}

	.vix-toggle {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		padding: 0;
		background: none;
		border: none;
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
		cursor: pointer;
		align-self: flex-start;
	}

	.vix-toggle:hover {
		color: var(--color-foreground-muted);
	}

	.vix-toggle-label {
		font-size: 0.8125rem;
	}

	.vix-details {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		width: 100%;
	}

	.vix-bar {
		height: 10px;
		background-image: radial-gradient(
			color-mix(in srgb, var(--color-foreground) 20%, transparent) 1px,
			transparent 0
		);
		background-size: 6px 6px;
		border-radius: 2px;
		overflow: hidden;
	}

	.vix-fill {
		height: 100%;
		background: var(--color-foreground-muted);
		border-radius: 2px;
	}

	.vix-fill-calibrating {
		opacity: 0.4;
	}

	.metric-description {
		font-size: 0.75rem;
		line-height: 1.5;
		color: var(--color-foreground-subtle);
		margin: 0;
	}

	/* Autobiography section */
	.autobiography-section {
		margin-bottom: 0;
	}

	.autobiography-header {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
		margin-bottom: 0.75rem;
	}

	.autobiography-header .section-title {
		margin: 0;
	}

	.autobiography-edit-btn {
		display: inline-flex;
		align-items: center;
		background: none;
		border: none;
		padding: 0;
		color: var(--color-foreground-subtle);
		cursor: pointer;
		opacity: 0;
		transition: opacity 0.15s ease;
		font-size: 0.875rem;
	}
	.autobiography-section:hover .autobiography-edit-btn {
		opacity: 1;
	}
	.autobiography-edit-btn:hover {
		color: var(--color-foreground-muted);
	}

	.autobiography-text {
		font-size: 0.9375rem;
		line-height: 1.6;
		color: var(--color-foreground-muted);
		margin: 0;
	}

	.autobiography-editable {
		outline: none;
		border-radius: 4px;
		padding: 0.375rem 0.5rem;
		margin: -0.375rem -0.5rem;
		background: color-mix(in srgb, var(--color-foreground) 3%, transparent);
		cursor: text;
	}
	.autobiography-editable:focus {
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
	}

	.regenerate-confirm-text {
		font-size: 0.875rem;
		line-height: 1.5;
		color: var(--color-foreground-muted);
		margin: 0;
	}

	:global(.spin-icon) {
		animation: spin 1s linear infinite;
	}

	@keyframes spin {
		from { transform: rotate(0deg); }
		to { transform: rotate(360deg); }
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

	.source-show-more {
		display: block;
		width: 100%;
		padding: 0.375rem 0.75rem;
		background: none;
		border: none;
		border-top: 1px solid var(--color-border);
		color: var(--color-foreground-muted);
		font-size: 0.75rem;
		cursor: pointer;
		text-align: center;
		transition: color 0.1s ease, background 0.1s ease;
	}

	.source-show-more:hover {
		color: var(--color-foreground);
		background: color-mix(in srgb, var(--color-foreground) 4%, transparent);
	}

	/* Movement map */
	.movement-loading {
		font-size: 0.875rem;
		color: var(--color-foreground-subtle);
		margin-bottom: 0.5rem;
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
