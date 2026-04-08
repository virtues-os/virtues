<!--
	DayPage.svelte

	Renders a day page with:
	- DayToolbar: compact week picker, generate button
	- Sections: Autobiography, Event Timeline, Movement, Entities, Sources (hidden when empty)
	- Full-width layout (no right rail)
-->

<script lang="ts">
	import { browser } from "$app/environment";
	import type { DayPage as DayPageType, DayEvent } from "$lib/wiki/types";
	import { flattenLinkedEntities } from "$lib/wiki/types";
	import {
		getDaySources,
		getDayEvents,
		updateDay,
		type DaySourceApi,
		type TemporalEventApi,

	} from "$lib/wiki/api";
	import { getLocalDateSlug } from "$lib/utils/dateUtils";
	import { spaceStore } from "$lib/stores/space.svelte";
	import EventTimeline from "./EventTimeline.svelte";
	import DaylineChart from "./DaylineChart.svelte";
	import DaylineTerrainChart from "./DaylineTerrainChart.svelte";
	import DayToolbar from "./DayToolbar.svelte";
	import DayHeaderPolaroid from "./DayHeaderPolaroid.svelte";
	import DataQualityCoverage from "./DataQualityCoverage.svelte";
	import JournalCard from "./JournalCard.svelte";
	import UniversalDataGrid, { type Column } from "$lib/components/UniversalDataGrid.svelte";
	import TableOfContents, { type TocHeading } from "$lib/components/TableOfContents.svelte";

	import Icon from "$lib/components/Icon.svelte";

	import MovementMap from "$lib/components/timeline/MovementMap.svelte";

	interface Props {
		page: DayPageType;
	}

	let { page }: Props = $props();

	// Shared hover state for chart ↔ timeline sync
	let hoveredEventId = $state<string | null>(null);

	// Timeline component ref for expand/collapse all
	let timelineRef = $state<{ toggleAll: () => void; allExpanded: boolean } | null>(null);

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
		const formatTz = (tz: string) => {
			const parts = tz.split("/");
			return parts[parts.length - 1].replace(/_/g, " ");
		};
		if (endTz && endTz !== startTz) {
			return `00:00 ${formatTz(startTz)} → 24:00 ${formatTz(endTz)}`;
		}
		return formatTz(startTz);
	}

	// Flatten linked entities for entity display
	const allLinkedPages = $derived(flattenLinkedEntities(page.linkedEntities));

	// Timezone display — fallback to browser timezone for ungenerated days
	function getBrowserTimezone(): string | null {
		if (!browser) return null;
		const tz = Intl.DateTimeFormat().resolvedOptions().timeZone;
		const parts = tz.split("/");
		return parts[parts.length - 1].replace(/_/g, " ");
	}

	const timezoneDisplay = $derived(
		formatTimezoneDisplay(page.startTimezone, page.endTimezone) ?? getBrowserTimezone(),
	);


	const currentDateSlug = $derived(getLocalDateSlug(page.date));
	const todaySlug = $derived(getLocalDateSlug(new Date()));

	const isPast = $derived(currentDateSlug < todaySlug);

	// ─────────────────────────────────────────────────────────────────────────
	// Day header illustration — served as BLOB via API route.
	// Falls back to static file for dev test images.
	// ─────────────────────────────────────────────────────────────────────────
	const illustrationUrl = $derived(
		page.hasIllustration
			? `/api/wiki/day/${currentDateSlug}/illustration`
			: `/images/day-illustrations/${currentDateSlug}.png`,
	);

	// Relative date badge: "Today", "Yesterday", "2 days ago", "Tomorrow", "Future"
	const relativeDateLabel = $derived(() => {
		if (currentDateSlug === todaySlug) return "Today";
		const pageTime = new Date(`${currentDateSlug}T12:00:00`).getTime();
		const todayTime = new Date(`${todaySlug}T12:00:00`).getTime();
		const diffDays = Math.round((pageTime - todayTime) / 86400000);
		if (diffDays === -1) return "Yesterday";
		if (diffDays === 1) return "Tomorrow";
		if (diffDays >= 2) return "Future";
		if (diffDays <= -2 && diffDays >= -6) return `${Math.abs(diffDays)} days ago`;
		return null;
	});

	function navigateToDay(date: Date) {
		const slug = getLocalDateSlug(date);
		if (slug === currentDateSlug) return;
		spaceStore.openTabFromRoute(`/day/day_${slug}`);
	}

	// ─────────────────────────────────────────────────────────────────────────
	// Header scroll observer (show date in toolbar when h1 scrolls away)
	// ─────────────────────────────────────────────────────────────────────────
	let headerEl = $state<HTMLElement | null>(null);
	let scrollContainerEl = $state<HTMLElement | null>(null);
	let headerScrolledAway = $state(false);

	$effect(() => {
		if (!browser || !headerEl || !scrollContainerEl) return;
		const observer = new IntersectionObserver(
			([entry]) => { headerScrolledAway = !entry.isIntersecting; },
			{ root: scrollContainerEl, threshold: 0 },
		);
		observer.observe(headerEl);
		return () => observer.disconnect();
	});

	// ─────────────────────────────────────────────────────────────────────────
	// Movement map
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
			if (version !== movementLoadVersion) return;
			if (!res.ok) throw new Error(`timeline day api ${res.status}`);
			const dayView = (await res.json()) as TimelineDayView;
			if (version !== movementLoadVersion) return;
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

	const stopPoints = $derived(
		movementStops.length > 0
			? movementStops.map((c) => ({
					lat: c.latitude,
					lng: c.longitude,
					label: c.place_name ?? undefined,
					timeMs: Date.parse(c.start_time),
				}))
			: [],
	);

	const hasLocationData = $derived(stopPoints.length >= 2);

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

	async function loadDataSources(dateSlug: string) {
		if (!browser) return;
		const version = ++sourcesLoadVersion;
		sourcesLoading = true;
		try {
			const result = await getDaySources(dateSlug);
			if (version !== sourcesLoadVersion) return;
			dataSources = result;
		} catch {
			if (version !== sourcesLoadVersion) return;
			dataSources = [];
		} finally {
			if (version === sourcesLoadVersion) sourcesLoading = false;
		}
	}

	$effect(() => {
		if (browser && page?.date) loadDataSources(dateSlug());
	});

	// ─────────────────────────────────────────────────────────────────────────
	// Unified source table (one chronological stream)
	// ─────────────────────────────────────────────────────────────────────────

	type SourceRow = DaySourceApi & { id: string };

	const sourceRows = $derived<SourceRow[]>(
		dataSources.map((s) => ({ ...s, id: s.id })),
	);

	/** Map source_type back to the ontology display name */
	function getOntologyName(sourceType: string): string {
		const map: Record<string, string> = {
			calendar: "Calendar Events",
			email: "Email",
			email_sent: "Email",
			location: "Location Visits",
			workout: "Workouts",
			sleep: "Sleep Sessions",
			transaction: "Financial Transactions",
			transcription: "Voice Transcriptions",
			steps: "Steps",
			chat: "Chat Sessions",
			page: "Page Edits",
			listening: "Listening History",
			app_usage: "App Usage",
			web_browsing: "Web Browsing",
			document: "Documents",
			bookmark: "Bookmarks",
		};
		// "message:slack", "message:#design-team" etc. → Messages
		if (sourceType.startsWith("message:")) return "Messages";
		return map[sourceType] ?? sourceType;
	}

	const sourceColumns: Column<SourceRow>[] = [
		{
			key: "timestamp",
			label: "Time",
			icon: "ri:time-line",
			width: "5.5rem",
			minWidth: "5.5rem",
			getValue: (item) => {
				const d = new Date(item.timestamp);
				return d.toLocaleTimeString("en-US", {
					hour: "numeric",
					minute: "2-digit",
					hour12: true,
				});
			},
		},
		{
			key: "source_type",
			label: "Ontology",
			width: "10rem",
			minWidth: "7rem",
			getValue: (item) => getOntologyName(item.source_type),
		},
		{
			key: "label",
			label: "Description",
		},
		{
			key: "preview",
			label: "Detail",
			hideOnMobile: true,
			getValue: (item) => item.preview ?? "",
		},
	];

	// ─────────────────────────────────────────────────────────────────────────
	// Events (timeline)
	// ─────────────────────────────────────────────────────────────────────────
	let dayEvents = $state<DayEvent[]>([]);
	let eventsLoadVersion = 0;

	function apiEventToDayEvent(api: TemporalEventApi): DayEvent {
		const start = new Date(api.start_time);
		const end = new Date(api.end_time);
		return {
			id: api.id,
			startTime: start,
			endTime: end,
			durationMinutes: Math.round((end.getTime() - start.getTime()) / 60000),
			autoLabel: api.auto_label ?? "Unknown",
			autoLocation: api.auto_location ?? undefined,
			sourceIds: Array.isArray(api.source_ontologies) ? api.source_ontologies : [],
			userLabel: api.user_label || undefined,
			userLocation: api.user_location || undefined,
			userNotes: api.user_notes || undefined,
			noveltyZ: api.novelty_z ?? null,
			autonomicZ: api.autonomic_z ?? null,
			avgHr: api.avg_hr ?? null,
			hrZ: api.hr_z ?? null,
			hrvZ: api.hrv_z ?? null,
			topics: api.topics ?? [],
			eventSummary: api.event_summary ?? null,
			agentAction: (api.agent_action as DayEvent["agentAction"]) ?? null,
			isSleep: api.is_sleep ?? false,
			userHidden: api.user_hidden ?? false,
			userCreated: api.user_created ?? false,
			entities: Array.isArray(api.entities) ? api.entities : [],
			topicNovelty: api.topic_novelty ?? null,
			entityNovelty: api.entity_novelty ?? null,
			entityTimestamps: api.entity_timestamps ?? null,
			isUserAdded: api.is_user_added ?? false,
			isUserEdited: api.is_user_edited ?? false,
			isTransit: api.is_transit ?? false,
			isUnknown: api.is_unknown ?? false,
		};
	}

	async function loadEvents(dateSlug: string) {
		if (!browser) return;
		const version = ++eventsLoadVersion;
		try {
			const result = await getDayEvents(dateSlug);
			if (version !== eventsLoadVersion) return;
			dayEvents = result.map(apiEventToDayEvent);
		} catch {
			if (version !== eventsLoadVersion) return;
			dayEvents = [];
		}
	}

	$effect(() => {
		if (browser && page?.date) loadEvents(dateSlug());
	});

	// ─────────────────────────────────────────────────────────────────────────
	// Autobiography (read-only display + inline edit)
	// ─────────────────────────────────────────────────────────────────────────
	let summaryText = $state(page.autobiography || "");
	let editingAutobiography = $state(false);

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
			await updateDay(dateSlug(), {
				autobiography: trimmed,
				last_edited_by: "user",
			});
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

	// ─────────────────────────────────────────────────────────────────────────
	// Section visibility (hide empty sections)
	// ─────────────────────────────────────────────────────────────────────────
	const showAutobiography = $derived(!!summaryText);
	const showTimeline = $derived(
		dayEvents.filter((e) => !e.isUnknown).length > 0,
	);
	const showMovement = $derived(hasLocationData);
	const showEntities = $derived(allLinkedPages.length > 0);
	const showSources = $derived(dataSources.length > 0);

	const hasAnyContent = $derived(
		showAutobiography ||
			showTimeline ||
			showMovement ||
			showEntities ||
			showSources,
	);

	// ─────────────────────────────────────────────────────────────────────────
	// Table of contents headings (derived from visible sections)
	// ─────────────────────────────────────────────────────────────────────────
	const tocHeadings = $derived.by<TocHeading[]>(() => {
		const h: TocHeading[] = [];
		if (showAutobiography) h.push({ id: "summary", text: "The Day", level: 2 });
		h.push({ id: "dayline", text: "Dayline", level: 2 });
		if (showTimeline) h.push({ id: "timeline", text: "Event Timeline", level: 2 });
		if (showMovement) h.push({ id: "movement", text: "Movement", level: 2 });
		if (showEntities) h.push({ id: "entities", text: "Entities", level: 2 });
		if (showSources) h.push({ id: "ontologies", text: "Ontologies", level: 2 });
		h.push({ id: "metadata", text: "Metadata", level: 3 });
		return h;
	});

</script>

<div class="day-page-outer">
	<DayToolbar
		pageDate={page.date}
		{currentDateSlug}
		{todaySlug}
		onNavigateDay={navigateToDay}
		{headerScrolledAway}
	/>

	<div class="day-page-layout">
		<article class="day-article wiki-article" bind:this={scrollContainerEl}>
			<div class="day-content-with-toc">
			<div class="day-content">
				<!-- Header: title-page layout (illustration → h1 → meta → rule) -->
				<header class="day-header" bind:this={headerEl}>
					<div class="day-header-illustration">
						<DayHeaderPolaroid
							imageUrl={illustrationUrl}
							aspect="1:1"
							variant="naked"
							width={220}
						/>
					</div>
					<h1 class="day-title">
						{formatDate(page.date, page.dayOfWeek)}
					</h1>
					{#if relativeDateLabel()}
						<div class="day-subtitle">
							<span class="date-badge">{relativeDateLabel()}</span>
						</div>
					{/if}
					{#if page.epigraph}
						<p class="day-epigraph">{page.epigraph}</p>
					{/if}
					<div class="day-title-rule" aria-hidden="true"></div>
				</header>

				<!-- Narrative first: the day told in words (unfolds from the epigraph) -->
				{#if showAutobiography}
					<section class="section lead-section" id="summary">
						<h2 class="section-title">The Day</h2>
						{#if editingAutobiography}
							<!-- svelte-ignore a11y_no_static_element_interactions -->
							<div
								class="lead-text lead-editable"
								contenteditable="true"
								onblur={handleAutobiographyBlur}
								onkeydown={handleAutobiographyKeydown}
								role="textbox"
								aria-label="Edit autobiography"
							>
								{summaryText}
							</div>
						{:else if summaryText}
							<div class="lead-content">
								<p class="lead-text">{summaryText}</p>
							</div>
						{:else}
							<p class="empty-placeholder">Generating...</p>
						{/if}
					</section>
				{/if}

				<!-- Journal: personal writing for this day -->
				<JournalCard date={currentDateSlug} />

				<!-- Dayline chart: visual bridge between narrative and timeline -->
				<section class="section" id="dayline">
					<DaylineChart events={dayEvents} timezone={page.startTimezone} pageDate={page.date} readinessScore={page.readinessScore} />
				</section>

				<!-- Experimental: single-line-with-fill terrain chart -->
				<section class="section" id="dayline-v2">
					<DaylineTerrainChart />
				</section>

				{#if hasAnyContent}
					<!-- Timeline -->
					{#if showTimeline}
						<section class="section" id="timeline">
							<div class="section-header-row">
								<h2 class="section-title">Event Timeline</h2>
								<div class="section-actions">
								<button class="section-action-btn" type="button" onclick={() => timelineRef?.toggleAll()}>
									{timelineRef?.allExpanded ? 'Collapse all' : 'Expand all'}
								</button>
							</div>
							</div>
							<EventTimeline bind:this={timelineRef} events={dayEvents} timezone={page.startTimezone} {hoveredEventId} onhover={(id) => hoveredEventId = id} pageDate={page.date} />
						</section>
					{/if}

					<!-- Movement -->
					{#if showMovement}
						<section class="section" id="movement">
							<h2 class="section-title">Movement</h2>
							<MovementMap
								track={stopPoints}
								stops={stopMarkers}
								height={240}
							/>
						</section>
					{/if}

					<!-- Entities -->
					{#if showEntities}
						<section class="section" id="entities">
							<h2 class="section-title">Entities</h2>
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
						</section>
					{/if}

					<!-- Ontologies: one chronological table -->
					{#if showSources}
						<section class="section" id="ontologies">
							<h2 class="section-title">Ontologies</h2>
							<div class="sources-table-wrapper">
								<UniversalDataGrid
									items={sourceRows}
									columns={sourceColumns}
									entityType="day-sources"
									loading={sourcesLoading}
									emptyIcon="ri:database-2-line"
									emptyMessage="No source data"
									loadingMessage="Loading sources..."
									searchPlaceholder="Filter sources..."
									pageSize={8}
								/>
							</div>
						</section>
					{/if}

					<!-- Metadata: audit trail + ambient day context -->
					<section class="section" id="metadata">
						<h2 class="section-title">Metadata</h2>
						<dl class="metadata-grid">
							{#if page.startTimezone}
								<dt>Timezone</dt>
								<dd>{timezoneDisplay}</dd>
							{/if}
							{#if page.createdAt}
								<dt>Created</dt>
								<dd>{new Date(page.createdAt).toLocaleString()}</dd>
							{/if}
							{#if page.updatedAt}
								<dt>Last updated</dt>
								<dd>
									{new Date(page.updatedAt).toLocaleString()}
									{#if page.lastEditedBy}
										<span class="metadata-dim">· by {page.lastEditedBy}</span>
									{/if}
								</dd>
							{/if}
							<dt>Events</dt>
							<dd>{dayEvents.length}</dd>
							<dt>Sources</dt>
							<dd>{dataSources.length}</dd>
							<dt>New entities</dt>
							<dd>{page.newEntityCount}</dd>
							<dt>New topics</dt>
							<dd>{page.newTopicCount}</dd>
							{#if page.readinessScore != null}
								<dt>Readiness</dt>
								<dd>{page.readinessScore}%</dd>
							{/if}
							<dt>Page ID</dt>
							<dd class="metadata-mono">{page.id}</dd>
							{#if page.dataQuality}
								<dt>Coverage</dt>
								<dd>
									<DataQualityCoverage dataQuality={page.dataQuality} />
								</dd>
							{/if}
						</dl>
					</section>
				{:else}
					<!-- Empty state: context-aware -->
					<div class="empty-state">
						{#if currentDateSlug > todaySlug}
							<p class="empty-state-text">This day hasn't happened yet.</p>
						{:else if currentDateSlug === todaySlug}
							<p class="empty-state-text">Your day is still in progress.</p>
						{:else if dataSources.length > 0}
							<p class="empty-state-text">{dataSources.length} sources recorded. Events will be generated automatically.</p>
						{:else}
							<p class="empty-state-text">No source data recorded for this day.</p>
						{/if}
					</div>
				{/if}
			</div>
			<TableOfContents headings={tocHeadings} scrollContainer={scrollContainerEl} />
			</div>
		</article>

	</div>
</div>

<style>
	.day-page-outer {
		display: flex;
		flex-direction: column;
		height: 100%;
		width: 100%;
		overflow: hidden;
	}

	.day-page-layout {
		display: flex;
		flex: 1;
		min-height: 0;
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

	.day-content-with-toc {
		display: flex;
		justify-content: center;
		gap: 2rem;
		max-width: 68rem;
		margin: 0 auto;
	}

	.day-content {
		max-width: 48rem;
		width: 100%;
		padding-top: 2rem;
		padding-bottom: 4rem;
	}

	/* Header: title-page layout — illustration, h1, meta, rule, all centered */
	.day-header {
		margin-bottom: 4rem;
		display: flex;
		flex-direction: column;
		align-items: center;
		text-align: center;
		gap: 1.25rem;
	}

	.day-header-illustration {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.75rem;
	}

	.day-title-rule {
		width: 3rem;
		height: 1px;
		background: color-mix(in srgb, var(--color-foreground) 15%, transparent);
		margin-top: 0.25rem;
	}

	.day-epigraph {
		font-family: var(--font-sans, system-ui, sans-serif);
		font-style: italic;
		font-weight: 400;
		font-size: 0.9375rem;
		line-height: 1.5;
		color: var(--color-foreground-subtle);
		letter-spacing: 0.01em;
		margin: 0;
		max-width: 32rem;
	}

	.day-title {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 2.25rem;
		font-weight: 400;
		color: var(--color-foreground);
		margin: 0;
		line-height: 1.2;
		letter-spacing: -0.01em;
	}

	.day-subtitle {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 0.625rem;
		margin-top: 0.375rem;
	}

	.date-badge {
		font-family: var(--font-sans, system-ui, sans-serif);
		font-size: 0.6875rem;
		font-weight: 500;
		color: var(--color-foreground-muted);
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
		padding: 1px 8px;
		border-radius: 9999px;
	}

	.day-timezone {
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}

	/* Lead paragraph (autobiography without heading) */
	.lead-section {
		margin-bottom: 2rem;
	}

	.lead-content {
		position: relative;
	}

	.lead-text {
		font-size: 0.9375rem;
		line-height: 1.7;
		color: var(--color-foreground);
		margin: 0;
	}

	.lead-edit-btn {
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
		margin-top: 0.5rem;
	}
	.lead-content:hover .lead-edit-btn {
		opacity: 1;
	}
	.lead-edit-btn:hover {
		color: var(--color-foreground-muted);
	}

	.lead-editable {
		outline: none;
		border-radius: 4px;
		padding: 0.375rem 0.5rem;
		margin: -0.375rem -0.5rem;
		background: color-mix(
			in srgb,
			var(--color-foreground) 3%,
			transparent
		);
		cursor: text;
	}
	.lead-editable:focus {
		background: color-mix(
			in srgb,
			var(--color-foreground) 5%,
			transparent
		);
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
		from {
			transform: rotate(0deg);
		}
		to {
			transform: rotate(360deg);
		}
	}

	/* Sections */
	.section {
		position: relative;
		margin-bottom: 3.5rem;
	}

	.section-title {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.375rem;
		font-weight: 400;
		line-height: 1.35;
		color: var(--color-foreground);
		margin: 0 0 0.75rem;
	}

	.section-header-row {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 1rem;
	}

	.section-header-row .section-title {
		margin-bottom: 0.75rem;
	}

	.section-actions {
		display: flex;
		align-items: center;
		gap: 0.25rem;
		flex-shrink: 0;
	}

	.section-action-btn {
		background: none;
		border: none;
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
		cursor: pointer;
		padding: 0.125rem 0.25rem;
		border-radius: 3px;
	}

	.section-action-btn:hover {
		color: var(--color-foreground-muted);
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
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

	/* Empty state */
	.empty-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 1rem;
		padding: 4rem 2rem;
	}

	.empty-state-text {
		font-size: 0.9375rem;
		color: var(--color-foreground-subtle);
		margin: 0;
	}

	.empty-state-generate {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		background: none;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		padding: 0.5rem 1rem;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
		cursor: pointer;
	}
	.empty-state-generate:hover {
		color: var(--color-foreground);
		border-color: var(--color-border-strong);
	}
	.empty-state-generate:disabled {
		opacity: 0.5;
		cursor: default;
	}

	/* Sources table */
	.sources-table-wrapper {
		margin: 0 -2rem;
	}

	/* Metadata grid */
	.metadata-grid {
		display: grid;
		grid-template-columns: max-content 1fr;
		gap: 0.375rem 1.5rem;
		font-size: 0.8125rem;
		margin: 0;
	}
	.metadata-grid dt {
		color: var(--color-foreground-subtle);
		font-weight: 400;
	}
	.metadata-grid dd {
		color: var(--color-foreground-muted);
		margin: 0;
	}
	.metadata-dim {
		color: var(--color-foreground-subtle);
	}
	.metadata-mono {
		font-family: var(--font-mono, "SF Mono", Menlo, monospace);
		font-size: 0.75rem;
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
			font-size: 1.75rem;
		}

		.day-header {
			gap: 1rem;
		}
	}
</style>
