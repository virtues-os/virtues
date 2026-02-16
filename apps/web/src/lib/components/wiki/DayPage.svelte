<!--
	DayPage.svelte

	Renders a day page with:
	- DayToolbar: compact week picker, coverage/entropy metrics, generate button
	- Sections: Autobiography, Event Timeline, Movement, Entities, Sources (hidden when empty)
	- WikiRightRail: table of contents + metadata
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
		type WikiDayApi,
	} from "$lib/wiki/api";
	import { getLocalDateSlug } from "$lib/utils/dateUtils";
	import { spaceStore } from "$lib/stores/space.svelte";
	import type { ContextVector as ContextVectorType } from "$lib/wiki/types";
	import ContextVector from "./ContextVector.svelte";
	import DayRibbonChart from "./DayRibbonChart.svelte";
	import DayTimeline from "./DayTimeline.svelte";
	import DayToolbar from "./DayToolbar.svelte";
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

	// Shared hover state for chart ↔ timeline sync
	let hoveredEventId = $state<string | null>(null);

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


	// ─────────────────────────────────────────────────────────────────────────
	// Week navigation
	// ─────────────────────────────────────────────────────────────────────────
	let weekOffset = $state(0);

	$effect(() => {
		page.date; // track
		weekOffset = 0;
	});

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

	const isPast = $derived(currentDateSlug < todaySlug);

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
			if (version !== sourcesLoadVersion) return;
			dataSources = result;
			if (dataSources.length === 0 && dateSlug === "2025-12-10") {
				dataSources = MOCK_DATA_SOURCES;
			}
		} catch {
			if (version !== sourcesLoadVersion) return;
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

	const groupedSources = $derived(() => {
		const groups: Record<string, DaySourceApi[]> = {};
		for (const source of dataSources) {
			const type = source.source_type;
			if (!groups[type]) groups[type] = [];
			groups[type].push(source);
		}
		return groups;
	});

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
		if (sourceType.startsWith("message:")) {
			return "ri:message-3-line";
		}
		return iconMap[sourceType] ?? "ri:database-2-line";
	}

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

	function formatSourceTime(timestamp: string): string {
		const date = new Date(timestamp);
		return date.toLocaleTimeString("en-US", {
			hour: "numeric",
			minute: "2-digit",
			hour12: true,
		});
	}

	// ─────────────────────────────────────────────────────────────────────────
	// Events (timeline + W6H trajectory)
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
			sourceIds: Array.isArray(api.source_ontologies) ? api.source_ontologies as string[] : [],
			userLabel: api.user_label || undefined,
			userLocation: api.user_location || undefined,
			userNotes: api.user_notes || undefined,
			w6hActivation: api.w6h_activation ?? null,
			entropy: api.entropy ?? null,
			w6hEntropy: api.w6h_entropy ?? null,
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
	// Daily Summary (autobiography generation)
	// ─────────────────────────────────────────────────────────────────────────
	let metricsExpanded = $state(false);
	let summaryGenerating = $state(false);
	let summaryText = $state(page.autobiography || "");
	let editingAutobiography = $state(false);
	let contextVector = $state(page.contextVector);
	let chaosScore = $state(page.chaosScore);
	let entropyCalibrationDays = $state(page.entropyCalibrationDays);

	$effect(() => {
		summaryText = page.autobiography || "";
		editingAutobiography = false;
		contextVector = page.contextVector;
		chaosScore = page.chaosScore;
		entropyCalibrationDays = page.entropyCalibrationDays;
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
			const res = await fetch(`/api/wiki/day/${dateSlug()}/summary`, {
				method: "POST",
			});
			if (!res.ok) throw new Error(`Summary generation failed: ${res.status}`);
			const updated: WikiDayApi = await res.json();
			summaryText = updated.autobiography || "";

			const cv = parseContextVector(updated.context_vector);
			if (cv) {
				contextVector = cv;
			}
			chaosScore = updated.chaos_score ?? null;
			entropyCalibrationDays = updated.entropy_calibration_days ?? null;

			// Reload events after generation (Tollbooth creates structured events with W6H)
			loadEvents(dateSlug());
		} catch (e) {
			console.error("Summary generation failed:", e);
		} finally {
			summaryGenerating = false;
		}
	}

	// ─────────────────────────────────────────────────────────────────────────
	// Section visibility (hide empty sections)
	// ─────────────────────────────────────────────────────────────────────────
	const showAutobiography = $derived(!!summaryText || summaryGenerating);
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

	// Dynamic TOC: only include headings for visible sections
	const fullContent = $derived(() => {
		let toc = "";
		if (showTimeline) toc += "## Event Timeline\n\n";
		if (showMovement) toc += "## Movement\n\n";
		if (showEntities) toc += "## Entities\n\n";
		if (showSources) toc += "## Sources\n\n";
		return toc;
	});
</script>

<div class="day-page-outer">
	<DayToolbar
		pageDate={page.date}
		weekDays={weekDays()}
		{currentDateSlug}
		{todaySlug}
		contextVector={contextVector}
		{chaosScore}
		{entropyCalibrationDays}
		{summaryGenerating}
		summaryExists={!!summaryText}
		onNavigateDay={navigateToDay}
		onWeekPrev={() => weekOffset--}
		onWeekNext={() => weekOffset++}
		onGenerate={handleGenerateClick}
		onToggleMetrics={() => (metricsExpanded = !metricsExpanded)}
	/>

	<div class="day-page-layout">
		<article class="day-article wiki-article">
			<div class="day-content">
				<!-- Header -->
				<header class="day-header">
					<h1 class="day-title">
						{formatDate(page.date, page.dayOfWeek)}
					</h1>
					<div class="day-subtitle">
						{#if relativeDateLabel()}
							<span class="date-badge">{relativeDateLabel()}</span>
						{/if}
						{#if timezoneDisplay}
							<span class="day-timezone">{timezoneDisplay}</span>
						{/if}
					</div>
				</header>

				<!-- Lead paragraph (autobiography — above everything as the summary) -->
				{#if showAutobiography}
					<div class="lead-section">
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
								<button
									class="lead-edit-btn"
									onclick={startEditingAutobiography}
									type="button"
									title="Edit"
								>
									<Icon icon="ri:pencil-line" />
								</button>
							</div>
						{:else}
							<p class="empty-placeholder">Generating...</p>
						{/if}
					</div>
				{/if}

				<!-- Expanded metrics (toggled from toolbar metric labels) -->
				{#if metricsExpanded}
					<div class="expanded-metrics" transition:slide={{ duration: 200 }}>
						{#if contextVector}
							<ContextVector
								contextVector={contextVector}
								expanded={true}
								showToggle={false}
							/>
						{/if}
						{#if entropyCalibrationDays != null}
							{@const calibDays = entropyCalibrationDays}
							{@const isCalibrated = calibDays >= 7}
							{@const hasScore =
								chaosScore != null && isCalibrated}
							<div class="vix-details">
								{#if hasScore}
									<div class="vix-bar">
										<div
											class="vix-fill"
											style="width: {Math.round(chaosScore! * 100)}%"
										></div>
									</div>
									<p class="metric-description">
										How unpredictable your day was. Low
										entropy means routine and structure;
										high entropy means novelty and disorder.
									</p>
								{:else}
									<div class="vix-bar">
										<div
											class="vix-fill vix-fill-calibrating"
											style="width: {Math.round((calibDays / 7) * 100)}%"
										></div>
									</div>
									<p class="metric-description">
										Entropy needs ~7 days of data to
										establish your baseline. {calibDays === 0
											? "This is your first day."
											: `${calibDays} day${calibDays === 1 ? "" : "s"} recorded so far.`}
									</p>
								{/if}
							</div>
						{:else}
							<div class="vix-details">
								<p class="metric-description">
									Entropy measures how unpredictable your day was compared to your baseline. Summarize at least one day to begin calibrating.
								</p>
							</div>
						{/if}
					</div>
				{/if}

				{#if hasAnyContent}
					<!-- Timeline -->
					{#if showTimeline}
						<section class="section" id="timeline">
							<h2 class="section-title">Event Timeline</h2>
							{#if dayEvents.some((e) => e.entropy != null)}
								<DayRibbonChart events={dayEvents} timezone={page.startTimezone} {hoveredEventId} onhover={(id) => hoveredEventId = id} />
							{/if}
							<DayTimeline events={dayEvents} timezone={page.startTimezone} {hoveredEventId} onhover={(id) => hoveredEventId = id} />
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

					<!-- Sources -->
					{#if showSources}
						<section class="section" id="sources">
							<h2 class="section-title">Sources</h2>
							{#if sourcesLoading}
								<p class="empty-placeholder">Loading...</p>
							{:else}
								<div class="sources-list">
									{#each Object.entries(groupedSources()) as [sourceType, sources]}
										{@const isExpanded =
											expandedGroups.has(sourceType)}
										{@const visibleSources = isExpanded
											? sources
											: sources.slice(
													0,
													SOURCE_PREVIEW_LIMIT,
												)}
										{@const hasMore =
											sources.length > SOURCE_PREVIEW_LIMIT}
										<div class="source-group">
											<div class="source-group-header">
												<Icon
													icon={getSourceIcon(
														sourceType,
													)}
													class="source-group-icon"
												/>
												<span
													class="source-group-name"
													>{getSourceTypeName(
														sourceType,
													)}</span
												>
												<span
													class="source-group-count"
													>{sources.length}</span
												>
											</div>
											<ul class="source-items">
												{#each visibleSources as source}
													<li class="source-item">
														<span
															class="source-time"
															>{formatSourceTime(
																source.timestamp,
															)}</span
														>
														<span
															class="source-label"
															>{source.label}</span
														>
														{#if source.preview}
															<span
																class="source-preview"
																>{source.preview}</span
															>
														{/if}
													</li>
												{/each}
											</ul>
											{#if hasMore}
												<button
													class="source-show-more"
													onclick={() =>
														toggleGroupExpand(
															sourceType,
														)}
													type="button"
												>
													{isExpanded
														? "Show less"
														: `Show all ${sources.length}`}
												</button>
											{/if}
										</div>
									{/each}
								</div>
							{/if}
						</section>
					{/if}
				{:else}
					<!-- Empty state: context-aware -->
					<div class="empty-state">
						{#if currentDateSlug > todaySlug}
							<p class="empty-state-text">This day hasn't happened yet.</p>
						{:else if currentDateSlug === todaySlug}
							<p class="empty-state-text">Your day is still in progress.</p>
						{:else if dataSources.length > 0}
							<p class="empty-state-text">{dataSources.length} sources recorded.</p>
							<button
								class="empty-state-generate"
								onclick={handleGenerateClick}
								disabled={summaryGenerating}
								type="button"
							>
								{#if summaryGenerating}
									<Icon icon="ri:loader-4-line" class="spin-icon" />
									Summarizing...
								{:else}
									Summarize this day
								{/if}
							</button>
						{:else}
							<p class="empty-state-text">No source data recorded for this day.</p>
						{/if}
					</div>
				{/if}
			</div>
		</article>

		<WikiRightRail content={fullContent()}>
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
							>{dayEvents.filter((e) => !e.isUnknown).length} events</span
						>
						<span class="stat-sep">·</span>
						<span class="stat">{dataSources.length} sources</span>
					</div>
				</div>
			{/snippet}
		</WikiRightRail>
	</div>
</div>

<!-- Regenerate confirmation modal -->
<Modal
	open={showRegenerateConfirm}
	onClose={() => (showRegenerateConfirm = false)}
	title="Regenerate Day"
	width="sm"
>
	<p class="regenerate-confirm-text">
		This will regenerate the autobiography, completeness, and entropy scores.
		Any manual edits will be overwritten.
	</p>
	{#snippet footer()}
		<button
			class="modal-btn modal-btn-secondary"
			onclick={() => (showRegenerateConfirm = false)}
		>
			Cancel
		</button>
		<button class="modal-btn modal-btn-primary" onclick={confirmRegenerate}>
			Regenerate
		</button>
	{/snippet}
</Modal>

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

	.day-content {
		max-width: 48rem;
		margin: 0 auto;
		padding-top: 2rem;
		padding-bottom: 4rem;
	}

	/* Header */
	.day-header {
		margin-bottom: 1.5rem;
	}

	.day-title {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.75rem;
		font-weight: 400;
		color: var(--color-foreground);
		margin: 0 0 0.25rem;
		line-height: 1.3;
	}

	.day-subtitle {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin-top: 0.25rem;
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

	/* Expanded metrics panel */
	.expanded-metrics {
		display: flex;
		flex-direction: column;
		gap: 1rem;
		padding: 1rem 0;
		margin-bottom: 1rem;
		border-bottom: 1px solid var(--color-border);
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

	:global(.source-group-icon) {
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
		transition:
			color 0.1s ease,
			background 0.1s ease;
	}

	.source-show-more:hover {
		color: var(--color-foreground);
		background: color-mix(
			in srgb,
			var(--color-foreground) 4%,
			transparent
		);
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
