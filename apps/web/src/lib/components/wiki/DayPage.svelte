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
		getDayTimeline,
		getDayChats,
		updateDay,
		getArticle,
		type DaySourceApi,
		type DayChatApi,
		type TimelineDayLocationChunk,
	} from "$lib/wiki/api";
	import { apiToDayEvent } from "$lib/wiki/converters";
	import Markdown from "$lib/components/Markdown.svelte";
	import { getOntologyName } from "$lib/wiki/ontology";
	import { getLocalDateSlug } from "$lib/utils/dateUtils";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import EventTimeline from "./EventTimeline.svelte";
	import DaylineChart from "./DaylineChart.svelte";
	import DayToolbar from "./DayToolbar.svelte";
	import DataQualityCoverage from "./DataQualityCoverage.svelte";
	import JournalCard from "./JournalCard.svelte";
	import NotesRail from "./NotesRail.svelte";
	import UniversalDataGrid, { type Column } from "$lib/components/datagrid/UniversalDataGrid.svelte";
	import TableOfContents, { type TocHeading } from "$lib/components/TableOfContents.svelte";

	import Icon from "$lib/components/Icon.svelte";


	interface Props {
		page: DayPageType;
	}

	let { page }: Props = $props();

	// Shared hover state for chart ↔ timeline sync
	let hoveredEventId = $state<string | null>(null);

	// Timeline component ref for expand/collapse all
	let timelineRef = $state<{ toggleAll: () => void; allExpanded: boolean } | null>(null);

	function formatDate(date: Date, dayOfWeek: string): string {
		return `${dayOfWeek}, ${date.toLocaleDateString("en-US", {
			month: "long",
			day: "numeric",
			year: "numeric",
		})}`;
	}

	function formatTimezoneDisplay(startTz: string | null): string | null {
		if (!startTz) return null;
		const parts = startTz.split("/");
		return parts[parts.length - 1].replace(/_/g, " ");
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
		formatTimezoneDisplay(page.startTimezone) ?? getBrowserTimezone(),
	);

	// Render timestamps in the SAME zone the server windowed this day in: the
	// locked per-day start_timezone, else the viewing device's zone (which is
	// also what get_day_sources used for an in-progress today). Keeps the Time
	// column consistent with which records appear. See docs/timezone-model.md.
	const rowTz = $derived(page.startTimezone ?? undefined);


	const currentDateSlug = $derived(getLocalDateSlug(page.date));
	const todaySlug = $derived(getLocalDateSlug(new Date()));

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
		windowShellStore.openTabFromRoute(`/day/day_${slug}`);
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
	// Versioned loader: drops stale results when slug changes mid-flight.
	// ─────────────────────────────────────────────────────────────────────────
	function makeLoader<T>(
		fetcher: (slug: string) => Promise<T>,
		apply: (result: T | null) => void,
	) {
		let version = 0;
		return async (slug: string) => {
			const v = ++version;
			let result: T | null = null;
			try {
				result = await fetcher(slug);
			} catch {
				result = null;
			}
			if (v === version) apply(result);
		};
	}

	// ─────────────────────────────────────────────────────────────────────────
	// Movement map
	// ─────────────────────────────────────────────────────────────────────────
	let movementStops = $state<TimelineDayLocationChunk[]>([]);
	let movementTrack = $state<{ lat: number; lng: number; timeMs: number }[]>(
		[],
	);

	const loadMovement = makeLoader(
		(slug) => getDayTimeline(slug),
		(view) => {
			movementStops = view
				? view.chunks.filter(
						(c): c is TimelineDayLocationChunk =>
							c?.type === "location" &&
							typeof (c as { latitude?: unknown }).latitude === "number" &&
							typeof (c as { longitude?: unknown }).longitude === "number",
					)
				: [];
			movementTrack = view?.points
				? view.points.map((p) => ({
						lat: p.latitude,
						lng: p.longitude,
						timeMs: Date.parse(p.timestamp),
					}))
				: [];
		},
	);

	$effect(() => {
		if (browser && page?.date) loadMovement(currentDateSlug);
	});

	const stopPoints = $derived(
		movementStops.length > 0
			? movementStops.map((c) => ({
					lat: c.latitude,
					lng: c.longitude,
					label: c.place_name ?? "Unknown",
					timeMs: Date.parse(c.start_time),
					placeId: c.place_id,
				}))
			: [],
	);

	const hasLocationData = $derived(stopPoints.length >= 1);

	// Deduplicate map markers by place_id so multiple visits to the same place
	// (e.g. WeWork morning + afternoon) render as one pin, not two stacked.
	// Visits without a place_id fall back to coarse lat/lon as the dedup key.
	const dedupedMarkers = $derived.by(() => {
		const seen = new Map<string, (typeof stopPoints)[number]>();
		for (const p of stopPoints) {
			const key = p.placeId ?? `${p.lat.toFixed(4)},${p.lng.toFixed(4)}`;
			if (!seen.has(key)) seen.set(key, p);
		}
		return Array.from(seen.values());
	});

	// ─────────────────────────────────────────────────────────────────────────
	// Data Sources (ontology records for the day)
	// ─────────────────────────────────────────────────────────────────────────
	let dataSources = $state<DaySourceApi[]>([]);
	let sourcesLoading = $state(false);

	const loadDataSources = makeLoader(
		(slug) => getDaySources(slug),
		(result) => {
			dataSources = result ?? [];
			sourcesLoading = false;
		},
	);

	$effect(() => {
		if (browser && page?.date) {
			sourcesLoading = true;
			loadDataSources(currentDateSlug);
		}
	});

	// ─────────────────────────────────────────────────────────────────────────
	// Unified source table (one chronological stream)
	// ─────────────────────────────────────────────────────────────────────────

	type SourceRow = DaySourceApi & { id: string };

	const sourceRows = $derived<SourceRow[]>(
		dataSources.map((s) => ({ ...s, id: s.id })),
	);

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
					timeZone: rowTz,
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

	// Filter chips: one per ontology present that day. Continuous streams
	// (heart rate, steps, HRV) are high-frequency, so they default OFF — the
	// person toggles them on, or filters discrete ontologies off, per chip.
	type SourceTypeChip = {
		type: string;
		/** Every raw source_type this chip covers (they share one label). */
		types: Set<string>;
		name: string;
		count: number;
		continuous: boolean;
	};

	// Keyed by DISPLAY name, not raw source_type: some ontologies carry a
	// sub-discriminator in source_type ("message:imessage" + "message:sms",
	// "email" + "email_sent") that collapses to one label — keying on the raw
	// type rendered two identical "Messages" chips. Each chip carries the set
	// of raw types it covers, so toggling toggles the whole group.
	const sourceTypeChips = $derived.by<SourceTypeChip[]>(() => {
		const map = new Map<string, SourceTypeChip>();
		for (const s of dataSources) {
			const name = getOntologyName(s.source_type);
			const existing = map.get(name);
			if (existing) {
				existing.count++;
				existing.types.add(s.source_type);
				existing.continuous = existing.continuous && s.continuous;
			} else {
				map.set(name, {
					type: s.source_type,
					types: new Set([s.source_type]),
					name,
					count: 1,
					continuous: s.continuous,
				});
			}
		}
		return [...map.values()].sort((a, b) => a.name.localeCompare(b.name));
	});

	// Which ontology types are currently shown. Re-defaults whenever the day's
	// sources change: discrete on, continuous off.
	let activeSourceTypes = $state<Set<string>>(new Set());
	$effect(() => {
		const next = new Set<string>();
		for (const chip of sourceTypeChips)
			if (!chip.continuous) for (const t of chip.types) next.add(t);
		activeSourceTypes = next;
	});

	function toggleSourceChip(chip: SourceTypeChip) {
		const next = new Set(activeSourceTypes);
		const anyOn = [...chip.types].some((t) => next.has(t));
		for (const t of chip.types) {
			if (anyOn) next.delete(t);
			else next.add(t);
		}
		activeSourceTypes = next;
	}

	const visibleSourceRows = $derived(
		sourceRows.filter((r) => activeSourceTypes.has(r.source_type)),
	);

	const sourcesEmptyMessage = $derived(
		dataSources.length === 0
			? "No data points recorded for this day."
			: "No data points match the active filters.",
	);

	// ─────────────────────────────────────────────────────────────────────────
	// Events (timeline)
	// ─────────────────────────────────────────────────────────────────────────
	let dayEvents = $state<DayEvent[]>([]);

	// The prior day's trailing sleep. The detective cuts every timeline at
	// midnight, so an 11pm–6:30am night is split across two days' events —
	// the sleep chart needs the evening half to draw the night whole.
	let priorSleepEvents = $state<DayEvent[]>([]);

	const loadEvents = makeLoader(
		(slug) => getDayEvents(slug),
		(result) => {
			dayEvents = result ? result.map(apiToDayEvent) : [];
		},
	);

	$effect(() => {
		if (browser && page?.date) {
			loadEvents(currentDateSlug);
			const prev = new Date(`${currentDateSlug}T12:00:00`);
			prev.setDate(prev.getDate() - 1);
			// Only sleep that touches this day's midnight — the evening half of
			// tonight's split night. The prior day's own overnight block would
			// otherwise stretch the sleep chart across thirty hours.
			const midnight = new Date(`${currentDateSlug}T00:00:00`).getTime();
			getDayEvents(getLocalDateSlug(prev))
				.then((evs) => {
					priorSleepEvents = (evs ?? [])
						.map(apiToDayEvent)
						.filter(
							(e) =>
								e.isSleep &&
								!e.userHidden &&
								e.endTime.getTime() >= midnight - 10 * 60_000
						);
				})
				.catch(() => (priorSleepEvents = []));
		}
	});

	// ─────────────────────────────────────────────────────────────────────────
	// AI Chats (in-app Virtues + external imported conversations)
	// ─────────────────────────────────────────────────────────────────────────
	let dayChats = $state<DayChatApi[]>([]);

	const loadChats = makeLoader(
		(slug) => getDayChats(slug),
		(result) => {
			dayChats = result ?? [];
		},
	);

	$effect(() => {
		if (browser && page?.date) loadChats(currentDateSlug);
	});

	function formatChatTime(iso: string): string {
		return new Date(iso).toLocaleTimeString("en-US", {
			hour: "numeric",
			minute: "2-digit",
			hour12: true,
			timeZone: rowTz,
		});
	}

	function providerLabel(provider: string | null): string {
		if (!provider) return "External";
		const normalized = provider.toLowerCase();
		if (normalized === "chatgpt" || normalized === "openai") return "ChatGPT";
		if (normalized === "claude" || normalized === "anthropic") return "Claude";
		if (normalized === "gemini" || normalized === "google") return "Gemini";
		return provider.charAt(0).toUpperCase() + provider.slice(1);
	}

	function openChat(chatId: string) {
		windowShellStore.openTabFromRoute(`/chat/${chatId}`);
	}

	// ─────────────────────────────────────────────────────────────────────────
	// Autobiography (read-only display + inline edit)
	// ─────────────────────────────────────────────────────────────────────────
	let summaryText = $state(page.autobiography || "");
	let editingAutobiography = $state(false);

	$effect(() => {
		summaryText = page.autobiography || "";
		editingAutobiography = false;
	});

	async function saveAutobiography(newText: string) {
		const trimmed = newText.trim();
		if (trimmed === summaryText) {
			editingAutobiography = false;
			return;
		}
		try {
			await updateDay(currentDateSlug, {
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

	// The day article IS a page — Edit opens the page editor. The first real
	// edit claims it (the server flips auto_update off) and the nightly
	// narration stops rewriting that day.
	async function openDayArticle() {
		const a = await getArticle("day", page.id);
		if (a?.page_id) windowShellStore.openTabFromRoute(`/page/${a.page_id}`);
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
	const showChats = $derived(dayChats.length > 0);

	// Aggregate coverage percentage from W6H data quality (each dimension 1-5, overall is avg/5 → %)
	const coveragePercent = $derived.by<number | null>(() => {
		if (!page.dataQuality) return null;
		return (page.dataQuality.overall / 5) * 100;
	});

	const hasAnyContent = $derived(
		showAutobiography ||
			showTimeline ||
			showMovement ||
			showEntities ||
			showSources ||
			showChats,
	);

	// ─────────────────────────────────────────────────────────────────────────
	// Table of contents headings (derived from visible sections)
	// ─────────────────────────────────────────────────────────────────────────
	const tocHeadings = $derived.by<TocHeading[]>(() => {
		const h: TocHeading[] = [];
		if (showAutobiography) h.push({ id: "summary", text: "The Day", level: 2 });
		h.push({ id: "dayline", text: "The Dayline", level: 2 });
		if (showTimeline) h.push({ id: "timeline", text: "Event Timeline", level: 2 });
		if (showChats) h.push({ id: "chats", text: "AI Chats", level: 2 });
		if (showEntities) h.push({ id: "entities", text: "Entities", level: 2 });
		if (hasAnyContent) h.push({ id: "ontologies", text: "Data Ontologies", level: 2 });
		if (hasAnyContent) h.push({ id: "metadata", text: "Metadata", level: 3 });
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
		{coveragePercent}
	/>

	<div class="day-page-layout">
		<article class="day-article wiki-article" bind:this={scrollContainerEl}>
			<div class="day-content-with-toc">
			<div class="day-content">
				<!-- Header: title-page layout (h1 → meta → rule) -->
				<header class="day-header" bind:this={headerEl}>
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
					<!-- The record's colophon, where a title page would carry one:
					     when this was last written, by whose hand, and how much of
					     the day the record actually saw. The audit trail below
					     keeps the rest. -->
					{#if page.updatedAt || page.dataQuality}
						<p class="day-byline">
							{#if page.updatedAt}
								<span>
									Updated {new Date(page.updatedAt).toLocaleDateString("en-US", {
										month: "short",
										day: "numeric",
										year: "numeric",
									})}{page.lastEditedBy
										? ` · by ${page.lastEditedBy === "ai" ? "the record" : "you"}`
										: ""}
								</span>
							{/if}
							{#if page.updatedAt && page.dataQuality}
								<span class="byline-sep">·</span>
							{/if}
							{#if page.dataQuality}
								<span title={page.dataQuality.note}>
									Coverage {page.dataQuality.overall}/5
								</span>
							{/if}
						</p>
					{/if}
					<div class="day-title-rule" aria-hidden="true"></div>
				</header>

				<!-- Narrative first: the day told in words (unfolds from the epigraph) -->
				{#if showAutobiography}
					<section class="section lead-section" id="summary">
						<h2 class="section-title">
							The Day
							<!-- One pen at a time: the day article is kept by the
							     nightly narration until you edit it, at which point
							     it becomes yours and the record files notes instead. -->
							<button
								type="button"
								class="day-edit"
								title="Editing makes this day's article yours — the nightly narration stops rewriting it. Prefer a note for a line you want to attach to the day."
								onclick={openDayArticle}
							>
								Edit
							</button>
						</h2>
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
						{:else}
							<div class="lead-content">
								<Markdown content={summaryText} refVariant="quiet" />
							</div>
						{/if}
					</section>
				{/if}

				<!-- Dayline chart: visual bridge between narrative and timeline -->
				<section class="section" id="dayline">
					<h2 class="section-title">The Dayline</h2>
					<DaylineChart events={dayEvents} {priorSleepEvents} timezone={page.startTimezone} pageDate={page.date} readinessScore={page.readinessScore} sleepCycles={page.sleepCycles} {movementStops} {movementTrack} {dedupedMarkers} dayDateSlug={currentDateSlug} {hasLocationData} />
				</section>

				{#if hasAnyContent}
					<!-- Event Timeline -->
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

					<!-- Notes: the day's margin — where the examen line lands, and
					     where a machine note about this day would wait. -->
					<NotesRail subjectType="day" subjectId={page.id} />

					<!-- Legacy reflections, read-only; renders nothing when the
					     day has none. The primitive is retired — writing about a
					     day belongs to the day's article or a note on it. -->
					<JournalCard date={currentDateSlug} />

					<!-- Movement is now in the Dayline chart's "Location" pill -->

					<!-- AI Chats: conversations from this day -->
					{#if showChats}
						<section class="section" id="chats">
							<h2 class="section-title">AI Chats</h2>
							<div class="chat-list">
								{#each dayChats as chat (chat.id)}
									{#if chat.source === "virtues"}
										<button
											class="chat-item"
											type="button"
											onclick={() => openChat(chat.id)}
										>
											<span class="chat-icon"><Icon icon="ri:message-3-line" width="14" /></span>
											<div class="chat-item-content">
												<span class="chat-item-title">{chat.title}</span>
												<span class="chat-item-meta">
													<span class="chat-badge chat-badge-virtues">Virtues</span>
													· {chat.message_count} message{chat.message_count === 1 ? "" : "s"}
													· {formatChatTime(chat.started_at)}
												</span>
											</div>
										</button>
									{:else}
										<div class="chat-item chat-item-static">
											<span class="chat-icon"><Icon icon="ri:message-3-line" width="14" /></span>
											<div class="chat-item-content">
												<span class="chat-item-title">{chat.title}</span>
												<span class="chat-item-meta">
													<span class="chat-badge">{providerLabel(chat.provider)}</span>
													· {chat.message_count} message{chat.message_count === 1 ? "" : "s"}
													· {formatChatTime(chat.started_at)}
												</span>
											</div>
										</div>
									{/if}
								{/each}
							</div>
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

					<!-- Ontologies: one chronological table of every data point -->
					<section class="section" id="ontologies">
						<h2 class="section-title">Data Ontologies</h2>
						{#if sourceTypeChips.length > 0}
							<div class="source-filters" role="group" aria-label="Filter data points by ontology">
								{#each sourceTypeChips as chip (chip.name)}
									<button
										type="button"
										class="source-chip"
										class:active={activeSourceTypes.has(chip.type)}
										aria-pressed={activeSourceTypes.has(chip.type)}
										onclick={() => toggleSourceChip(chip)}
									>
										{chip.name}
										<span class="source-chip-count">{chip.count}</span>
									</button>
								{/each}
							</div>
						{/if}
						<div class="sources-table-wrapper">
							<UniversalDataGrid
								items={visibleSourceRows}
								columns={sourceColumns}
								entityType="day-sources"
								loading={sourcesLoading}
								emptyIcon="ri:database-2-line"
								emptyMessage={sourcesEmptyMessage}
								loadingMessage="Loading sources..."
								searchPlaceholder="Filter sources..."
								pageSize={8}
							/>
						</div>
					</section>

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
							<!-- "Last updated" moved to the byline under the title —
							     it is the one line a reader wants before the prose,
							     not after the sources table. -->
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

	/* Header: title-page layout — h1, meta, rule, all centered */
	.day-header {
		margin-bottom: 2.5rem;
		display: flex;
		flex-direction: column;
		align-items: center;
		text-align: center;
		gap: 1rem;
	}

	.day-title-rule {
		width: 3rem;
		height: 1px;
		background: color-mix(in srgb, var(--color-foreground) 15%, transparent);
		margin-top: 0.25rem;
	}

	/* Small-caps register under the title: present but quiet, like a printed
	   colophon. The tooltip on Coverage carries the quality note. */
	.day-byline {
		margin: 0;
		font-family: var(--font-sans, system-ui, sans-serif);
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
	}

	.byline-sep {
		margin: 0 0.375rem;
		opacity: 0.5;
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
		border-radius: var(--radius-full);
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

	/* Quiet until wanted — a small-caps verb beside the heading, not a button
	   competing with the prose. */
	.day-edit {
		margin-left: 0.625rem;
		background: none;
		border: none;
		padding: 0;
		font-family: var(--font-sans, sans-serif);
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
		cursor: pointer;
		vertical-align: middle;
	}

	.day-edit:hover {
		color: var(--color-primary);
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

	/* AI Chat list */
	.chat-list {
		display: flex;
		flex-direction: column;
		gap: 1px;
	}

	.chat-item {
		all: unset;
		display: flex;
		align-items: flex-start;
		gap: 0.625rem;
		padding: 0.5rem 0.625rem;
		border-radius: 6px;
		cursor: pointer;
		transition: background 0.12s ease;
	}

	.chat-item:hover {
		background: color-mix(in srgb, var(--color-foreground) 4%, transparent);
	}

	.chat-item-static {
		cursor: default;
	}

	.chat-item-static:hover {
		background: transparent;
	}

	.chat-badge {
		display: inline-block;
		font-size: 0.6875rem;
		font-weight: 500;
		padding: 1px 6px;
		border-radius: var(--radius-full);
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
		color: var(--color-foreground-muted);
		margin-right: 0.25rem;
	}

	.chat-badge-virtues {
		background: color-mix(in srgb, var(--color-primary) 14%, transparent);
		color: var(--color-primary);
	}

	.chat-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 1.5rem;
		height: 1.5rem;
		border-radius: 5px;
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
		color: var(--color-foreground-muted);
		flex-shrink: 0;
		margin-top: 1px;
	}

	.chat-item-content {
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
		min-width: 0;
	}

	.chat-item-title {
		font-size: 0.875rem;
		font-weight: 450;
		color: var(--color-foreground);
		line-height: 1.35;
	}

	.chat-item-meta {
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
		line-height: 1.3;
	}

	/* Sources table */
	/* Full-bleed: the records table breaks out of the reading column by the
	   width of the desktop gutter. A phone's gutter is narrower than 2rem, so
	   the same bleed hung the table off both edges of the screen — where the
	   page can't scroll to it and the datagrid's own controls sat outside the
	   viewport. The bleed starts at the shell's breakpoint, with the gutter. */
	.sources-table-wrapper {
		margin: 0;
	}

	@media (min-width: 768px) {
		.sources-table-wrapper {
			margin: 0 -2rem;
		}
	}

	/* Ontology filter chips */
	.source-filters {
		display: flex;
		flex-wrap: wrap;
		gap: 0.375rem;
		margin-bottom: 0.875rem;
	}

	.source-chip {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0.25rem 0.625rem;
		font-size: 0.75rem;
		font-weight: 500;
		line-height: 1.4;
		border: 1px solid color-mix(in srgb, var(--color-foreground) 12%, transparent);
		border-radius: var(--radius-full);
		background: transparent;
		color: var(--color-foreground-muted);
		cursor: pointer;
		opacity: 0.6;
		transition:
			opacity 0.12s ease,
			background 0.12s ease,
			border-color 0.12s ease,
			color 0.12s ease;
	}

	.source-chip:hover {
		opacity: 0.9;
	}

	.source-chip.active {
		opacity: 1;
		color: var(--color-primary);
		border-color: color-mix(in srgb, var(--color-primary) 35%, transparent);
		background: color-mix(in srgb, var(--color-primary) 12%, transparent);
	}

	.source-chip-count {
		font-variant-numeric: tabular-nums;
		font-size: 0.6875rem;
		opacity: 0.75;
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
