<!--
	DayPage.svelte

	One day, two views of it (agents/plan/day-article-plan.md):
	- Article: the weekday and date, the Abstract, the fact strip, and the body
	  with its margin (DayArticleBody), then the days on either side.
	- Record: the evidence — the dayline, the timeline, your chats, and every
	  record of the day. A citation in the article opens Record on its item.
	Notes for the editor open from the toolbar; Edit opens the article page.
-->

<script lang="ts">
	import { subjectHref } from "$lib/wiki/links";
	import { browser } from "$app/environment";
	import { tick } from "svelte";
	import type { DayEvent } from "$lib/wiki/types";
	import {
		getDaySources,
		getDayEvents,
		getDayTimeline,
		getDayChats,
		getDayFacts,
		getDayByDate,
		getSimilarDays,
		type SimilarDayApi,
		getArticle,
		type DayFactsApi,
		type DaySourceApi,
		type DayChatApi,
		type TimelineDayLocationChunk,
		type WikiDayApi,
	} from "$lib/wiki/api";
	import { apiToDayEvent } from "$lib/wiki/converters";
	import { getOntologyName } from "$lib/wiki/ontology";
	import { getLocalDateSlug, parseDateSlug } from "$lib/utils/dateUtils";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import TextAction from "$lib/components/TextAction.svelte";
	import EventTimeline from "./EventTimeline.svelte";
	import DaylineChart from "./DaylineChart.svelte";
	import DayDatePicker from "./DayDatePicker.svelte";
	import NotesRail from "./NotesRail.svelte";
	import UniversalDataGrid, { type Column } from "$lib/components/datagrid/UniversalDataGrid.svelte";
	import DayArticleBody from "./DayArticleBody.svelte";
	import DayFactStrip from "./DayFactStrip.svelte";
	import DayInline from "./DayInline.svelte";
	import { getRecord } from "$lib/api/client";
	import DayGloss from "./DayGloss.svelte";
	import { parseDayArticle, abstractOf, veilMarks } from "$lib/wiki/dayArticle";
	import { veiled } from "$lib/actions/veil";
	import { veil } from "$lib/stores/veil.svelte";
	import { Popover } from "$lib/floating";

	import Icon from "$lib/components/Icon.svelte";
	import Button from "$lib/components/Button.svelte";

	interface Props {
		/** The wire shape. See PersonPage for why the converter is gone. */
		page: WikiDayApi;
	}

	let { page }: Props = $props();

	// `page.date` is an ISO day string. Read it as a LOCAL date — `new Date`
	// on a bare `YYYY-MM-DD` is UTC midnight, which is the previous day for
	// everyone west of Greenwich, and this one feeds the chart's axis.
	const date = $derived(parseDateSlug(page.date));
	const dayOfWeek = $derived(
		["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"][
			date.getDay()
		]
	);
	const sleepCycles = $derived(
		(page.sleep_cycles ?? []).map((c) => ({
			startTime: new Date(c.start_time),
			endTime: new Date(c.end_time),
			dominantStage: c.dominant_stage,
			avgHr: c.avg_hr,
			autonomicZ: c.autonomic_z,
		}))
	);

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

	// Timezone display — fallback to browser timezone for ungenerated days
	function getBrowserTimezone(): string | null {
		if (!browser) return null;
		const tz = Intl.DateTimeFormat().resolvedOptions().timeZone;
		const parts = tz.split("/");
		return parts[parts.length - 1].replace(/_/g, " ");
	}

	const timezoneDisplay = $derived(
		formatTimezoneDisplay(page.start_timezone) ?? getBrowserTimezone(),
	);

	// Render timestamps in the SAME zone the server windowed this day in: the
	// locked per-day start_timezone, else the viewing device's zone (which is
	// also what get_day_sources used for an in-progress today). Keeps the Time
	// column consistent with which records appear. See agents/record/timezone-model.md.
	const rowTz = $derived(page.start_timezone ?? undefined);


	const currentDateSlug = $derived(getLocalDateSlug(date));
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

	let scrollContainerEl = $state<HTMLElement | null>(null);

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
	// READ-ONLY here. The day's prose lives on its article page, and `Edit`
	// (openDayArticle, below) opens that page in the real editor.
	//
	// There used to be an inline contenteditable that saved through
	// `updateDay({ autobiography })` — into the LEGACY column. Since 0083 moved
	// day prose onto article pages and `wiki_day_prose` began preferring the
	// page, that write went somewhere nothing reads: the article shadowed it, so
	// a user's edit vanished the instant they saved it, while
	// `last_edited_by: "user"` still claimed the day and stopped narration. The
	// worst of both.
	//
	// Porting the inline editor to write the page instead was the obvious fix
	// and is wrong: an article page may carry a live Yjs document, and a plain
	// content write under one is silently clobbered — the hazard
	// `save_day_article` already guards against by refusing when
	// `yjs_state IS NOT NULL`. The page editor is CRDT-aware; this was never
	// going to be. One editor, and it is that one.
	let summaryText = $state((page.article ?? "") || "");

	$effect(() => {
		summaryText = (page.article ?? "") || "";
	});

	// The day article IS a page — Edit opens the page editor. Editing it does
	// stop the nightly narration for that day, which is the one rung where that
	// is still true: narration writes a whole first draft and has no way to
	// edit around your sentences, so it stands down once there are any.
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
	// An "Entities" section stood here and could never draw: the converter
	// hardcoded the list empty and the server never sent one on a day at all,
	// so a day full of people rendered none while the Metadata block below
	// reported how many were new. The section is gone rather than left as a
	// stub; listing a day's entities is a thing to BUILD, from `wiki_refs`,
	// not a thing to leave half-wired.
	const showSources = $derived(dataSources.length > 0);
	const showChats = $derived(dayChats.length > 0);

	const hasAnyContent = $derived(
		showAutobiography ||
			showTimeline ||
			showMovement ||
			showSources ||
			showChats,
	);

	// ─────────────────────────────────────────────────────────────────────────
	// Article | Record
	// ─────────────────────────────────────────────────────────────────────────
	let view = $state<"article" | "record">("article");
	/** The record a citation opened Record on, `table:id`. */
	let citedRef = $state<string | null>(null);

	/**
	 * Switch views along the day's clock: the date stays where it is, the fact
	 * strip's coverage bar grows into the dayline, and the rest crossfades.
	 * Reduced motion, or a browser without view transitions, switches at once.
	 */
	type Transition = { finished: Promise<void>; skipTransition: () => void };
	let pendingTransition: Transition | null = null;

	/** Resolves once the new view is on the page. */
	async function switchView(change: () => void): Promise<void> {
		const doc = document as Document & {
			startViewTransition?: (cb: () => Promise<void>) => Transition;
		};
		const still = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
		// A switch while one is still animating lands at once: the first one
		// is skipped, never queued behind.
		if (!doc.startViewTransition || still || pendingTransition) {
			pendingTransition?.skipTransition();
			pendingTransition = null;
			change();
			await tick();
			return;
		}
		let ran = false;
		let landed!: () => void;
		const done = new Promise<void>((resolve) => (landed = resolve));
		const run = async () => {
			if (ran) return;
			ran = true;
			change();
			await tick();
			landed();
		};
		try {
			const t = doc.startViewTransition(run);
			pendingTransition = t;
			void t.finished.finally(() => {
				if (pendingTransition === t) pendingTransition = null;
			});
			// The update waits for a frame; a page that is not drawing (a
			// hidden pane) would never get one, so the switch happens anyway.
			setTimeout(() => void run(), 300);
		} catch {
			// Two day pages side by side share transition names; switch plainly.
			await run();
		}
		return done;
	}

	function openCitation(ref: string) {
		void switchView(() => {
			citedRef = ref;
			view = "record";
		});
		scrollContainerEl?.scrollTo({ top: 0 });
	}

	/** The same day on the map, in the one Timeline tab: brought forward and
	 *  moved to this day when it is open, else opened on it. */
	function openInTimeline() {
		const route = `/timeline/${currentDateSlug}`;
		const open = windowShellStore.findTab((t) => t.type === "timeline");
		if (open) {
			windowShellStore.setActiveTab(open.tab.id);
			windowShellStore.updateTab(open.tab.id, { route });
		} else windowShellStore.openTabFromRoute(route, { label: "Timeline" });
	}

	function showRecord() {
		void switchView(() => (view = "record"));
	}

	/** Back to the article, and to the sentence the citation came from. */
	function backToArticle() {
		const ref = citedRef;
		void switchView(() => {
			view = "article";
			citedRef = null;
		}).then(() => {
			if (!ref) return;
			const sentence = scrollContainerEl?.querySelector<HTMLElement>(`[data-refs~="${CSS.escape(ref)}"]`);
			if (!sentence) return;
			sentence.scrollIntoView({ block: "center" });
			sentence.classList.remove("flash");
			void sentence.offsetWidth;
			sentence.classList.add("flash");
			setTimeout(() => sentence.classList.remove("flash"), 1700);
			sentence.focus({ preventScroll: true });
		});
	}

	/** The cited record's row in the day's sources, when the sources carry it. */
	const citedRow = $derived.by(() => {
		if (!citedRef) return null;
		const id = citedRef.split(":").slice(1).join(":");
		return dataSources.find((s) => s.id === id) ?? null;
	});

	/**
	 * A cited record the day's list doesn't carry (a recording chunk is never
	 * a day source), fetched whole so the card can still show its words.
	 */
	let citedRecord = $state<{ ref: string; time: string; kind: string; text: string } | null>(null);
	$effect(() => {
		const ref = citedRef;
		if (!ref || citedRow) {
			citedRecord = null;
			return;
		}
		const [table, ...rest] = ref.split(":");
		getRecord(table, rest.join(":"))
			.then((r) => {
				if (citedRef !== ref) return;
				const row = r.row as Record<string, unknown>;
				const at = String(row[r.timestamp_column] ?? "");
				const raw = String(row.body ?? row.text ?? "").replace(/\[Speaker(?: \d+)?\]:\s*/g, "").trim();
				citedRecord = {
					ref,
					time: at ? new Date(at).toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit", timeZone: rowTz }) : "",
					kind: r.display_name,
					text: raw.length > 600 ? `${raw.slice(0, 600)}…` : raw,
				};
			})
			.catch(() => {
				if (citedRef === ref) citedRecord = null;
			});
	});

	const parsed = $derived(parseDayArticle(summaryText));
	const abstractMarked = $derived(veilMarks(parsed.abstract));

	/** "With": the people the Abstract links, in its order. */
	const abstractPeople = $derived.by(() => {
		const out: { name: string; href: string }[] = [];
		for (const m of abstractMarked.markdown.matchAll(/\[([^\]]+)\]\((\/person\/[^)]+)\)/g)) {
			if (!out.some((p) => p.href === m[2])) out.push({ name: m[1], href: m[2] });
		}
		return out;
	});

	// ─────────────────────────────────────────────────────────────────────────
	// The fact strip and the days on either side
	// ─────────────────────────────────────────────────────────────────────────
	let facts = $state<DayFactsApi | null>(null);
	const loadFacts = makeLoader((slug) => getDayFacts(slug), (r) => (facts = r));
	$effect(() => {
		if (browser && page?.date) loadFacts(currentDateSlug);
	});

	function shiftSlug(slug: string, days: number): string {
		const d = new Date(`${slug}T12:00:00`);
		d.setDate(d.getDate() + days);
		return getLocalDateSlug(d);
	}

	type Neighbor = { slug: string; date: Date; abstract: string };
	let prevDay = $state<Neighbor | null>(null);
	let nextDay = $state<Neighbor | null>(null);

	async function neighbor(slug: string): Promise<Neighbor> {
		const d = await getDayByDate(slug).catch(() => null);
		return { slug, date: parseDateSlug(slug), abstract: abstractOf(d?.article) };
	}

	const loadNeighbors = makeLoader(
		async (slug) => Promise.all([neighbor(shiftSlug(slug, -1)), neighbor(shiftSlug(slug, 1))]),
		(r) => {
			prevDay = r?.[0] ?? null;
			nextDay = r && r[1].slug <= todaySlug ? r[1] : null;
		},
	);
	$effect(() => {
		if (browser && page?.date) loadNeighbors(currentDateSlug);
	});

	function neighborLabel(n: Neighbor): string {
		return n.date.toLocaleDateString("en-US", { weekday: "long", month: "long", day: "numeric" });
	}

	let similar = $state<SimilarDayApi[]>([]);
	const loadSimilar = makeLoader((slug) => getSimilarDays(slug), (r) => (similar = r ?? []));
	$effect(() => {
		if (browser && page?.date) loadSimilar(currentDateSlug);
	});

	/** The chaos/order mark: each scored part of the day at its midpoint. */
	const noveltyPoints = $derived(
		dayEvents
			.filter((e) => !e.isUnknown && !e.isSleep && !e.userHidden && e.noveltyZ != null)
			.map((e) => ({
				at: new Date((e.startTime.getTime() + e.endTime.getTime()) / 2).toISOString(),
				z: e.noveltyZ as number,
			})),
	);

	let notesOpen = $state(false);
	let noteCount = $state(0);

</script>

{#snippet personGloss({ name, url }: { name: string; url: string })}<DayGloss {name} {url} date={currentDateSlug} onday={(slug) => navigateToDay(parseDateSlug(slug))} />{/snippet}

<div class="day-page-outer">
	<div class="day-page-layout">
		<article class="day-article wiki-article" bind:this={scrollContainerEl}>
			<div class="day-bar" role="toolbar" aria-label="Day">
				{#if view === "record" && citedRef}
					<button type="button" class="bar-action back" onclick={backToArticle}>← Back to the sentence</button>
				{/if}
				<span class="bar-gap"></span>
				<div class="segmented" role="group" aria-label="View">
					<button type="button" class="seg" class:active={view === "article"} aria-pressed={view === "article"} onclick={backToArticle}>Article</button>
					<button type="button" class="seg" class:active={view === "record"} aria-pressed={view === "record"} onclick={showRecord}>Data</button>
				</div>
				<button
					type="button"
					class="bar-action"
					class:active={veil.on}
					aria-pressed={veil.on}
					title={veil.on ? "You've hidden names and hard passages. Hold V to read them." : "Hide names and hard passages on day pages"}
					onclick={() => veil.toggle()}
				>
					{veil.on ? "Veiled · hold V" : "Veil"}
				</button>
				<Popover bind:open={notesOpen} placement="bottom-end">
					{#snippet trigger({ toggle })}
						<button type="button" class="bar-action" class:active={notesOpen} onclick={toggle}>
							Notes{noteCount ? ` ${noteCount}` : ""}
						</button>
					{/snippet}
					{#snippet children()}
						<div class="notes-popover">
							<p class="notes-title">Notes for the editor</p>
							<NotesRail subjectType="day" subjectId={page.id} bare oncount={(n) => (noteCount = n)} />
							<p class="notes-hint">The editor works through these the next time it revises this page.</p>
						</div>
					{/snippet}
				</Popover>
				{#if showAutobiography}
					<button type="button" class="bar-action" onclick={openDayArticle}>Edit</button>
				{/if}
			</div>

			<div class="day-content">
				<header class="day-header">
					<p class="day-eyebrow">{dayOfWeek}</p>
					<h1 class="day-title">
						<DayDatePicker
							pageDate={date}
							{currentDateSlug}
							{todaySlug}
							onNavigateDay={navigateToDay}
							title={relativeDateLabel() ?? "Go to another day"}
						>
							{#snippet label()}{date.toLocaleDateString("en-US", { month: "long", day: "numeric", year: "numeric" })}{/snippet}
						</DayDatePicker>
					</h1>
				</header>

				{#if view === "article"}
					{#if showAutobiography}
						{#if parsed.abstract}
							<div class="day-abstract" use:veiled={{ hiding: veil.hiding, phrases: abstractMarked.phrases }}>
								<div class="markdown markdown--article"><p><DayInline markdown={abstractMarked.markdown} person={personGloss} /></p></div>
							</div>
						{/if}
						<DayFactStrip {facts} people={abstractPeople} timezone={page.start_timezone} novelty={noveltyPoints} />
						<DayArticleBody blocks={parsed.blocks} oncite={openCitation} person={personGloss} />
					{:else}
						<DayFactStrip {facts} people={[]} timezone={page.start_timezone} novelty={noveltyPoints} />
						<div class="empty-state">
							{#if currentDateSlug > todaySlug}
								<p class="empty-state-text">This day hasn't happened yet.</p>
							{:else if currentDateSlug === todaySlug}
								<p class="empty-state-text">Your day is still in progress. Your server writes its page after it ends.</p>
							{:else}
								<p class="empty-state-text">This day has no page yet.</p>
							{/if}
						</div>
					{/if}

					{#if prevDay || nextDay}
						<nav class="adjacent" aria-label="Adjacent days">
							{#if prevDay}
								<button type="button" class="adjacent-card" onclick={() => navigateToDay(prevDay!.date)}>
									<span class="adjacent-when">← {neighborLabel(prevDay)}</span>
									{#if prevDay.abstract}<span class="adjacent-abstract" use:veiled={{ hiding: veil.hiding, whole: true }}>{prevDay.abstract}</span>{/if}
								</button>
							{:else}
								<span></span>
							{/if}
							{#if nextDay}
								<button type="button" class="adjacent-card adjacent-next" onclick={() => navigateToDay(nextDay!.date)}>
									<span class="adjacent-when">{neighborLabel(nextDay)} →</span>
									{#if nextDay.abstract}<span class="adjacent-abstract" use:veiled={{ hiding: veil.hiding, whole: true }}>{nextDay.abstract}</span>{/if}
								</button>
							{/if}
						</nav>
					{/if}

					{#if similar.length}
						<section class="similar" aria-label="Similar days">
							<h2 class="similar-title">Similar days</h2>
							{#each similar as d (d.date)}
								<button type="button" class="similar-row" onclick={() => navigateToDay(parseDateSlug(d.date))}>
									<span class="similar-date">{parseDateSlug(d.date).toLocaleDateString("en-US", { month: "short", day: "numeric", year: "numeric" })}</span>
									<span class="similar-abstract" use:veiled={{ hiding: veil.hiding, whole: true }}>{abstractOf(d.abstract_md)}</span>
								</button>
							{/each}
						</section>
					{/if}
				{:else}
					{#if citedRef}
						<section class="cited" aria-label="The cited record">
							<p class="cited-key">Cited in the article</p>
							{#if citedRow}
								<p class="cited-row">
									<span class="cited-time">{new Date(citedRow.timestamp).toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit", timeZone: rowTz })}</span>
									<span>{getOntologyName(citedRow.source_type)}</span>
									<span class="cited-label">{citedRow.label}</span>
								</p>
								{#if citedRow.preview}<p class="cited-preview">{citedRow.preview}</p>{/if}
							{:else if citedRecord}
								<p class="cited-row">
									<span class="cited-time">{citedRecord.time}</span>
									<span>{citedRecord.kind}</span>
								</p>
								{#if citedRecord.text}<p class="cited-preview">{citedRecord.text}</p>{/if}
							{:else}
								<p class="cited-preview">Loading the record…</p>
							{/if}
						</section>
					{/if}

<p class="to-timeline">
					<TextAction onclick={openInTimeline}>Open this day in the Timeline</TextAction>
				</p>

				<!-- Dayline chart: visual bridge between narrative and timeline -->
				<section class="section" id="dayline">
					<h2 class="section-title">The Dayline</h2>
					<DaylineChart events={dayEvents} {priorSleepEvents} timezone={page.start_timezone} pageDate={date} sleepCycles={sleepCycles} {movementStops} {movementTrack} {dedupedMarkers} dayDateSlug={currentDateSlug} {hasLocationData} />
				</section>

				{#if hasAnyContent}
					<!-- Event Timeline -->
					{#if showTimeline}
						<section class="section" id="timeline">
							<div class="section-header-row">
								<h2 class="section-title">Event Timeline</h2>
								<div class="section-actions">
								<Button variant="ghost" size="sm" onclick={() => timelineRef?.toggleAll()}>
									{timelineRef?.allExpanded ? 'Collapse all' : 'Expand all'}
								</Button>
							</div>
							</div>
							<EventTimeline bind:this={timelineRef} events={dayEvents} timezone={page.start_timezone} {hoveredEventId} onhover={(id) => hoveredEventId = id} pageDate={date} />
						</section>
					{/if}


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
							{#if page.start_timezone}
								<dt>Timezone</dt>
								<dd>{timezoneDisplay}</dd>
							{/if}
							{#if page.created_at}
								<dt>Created</dt>
								<dd>{new Date(page.created_at).toLocaleString()}</dd>
							{/if}
							<!-- "Last updated" moved to the byline under the title —
							     it is the one line a reader wants before the prose,
							     not after the sources table. -->
							<dt>Events</dt>
							<dd>{dayEvents.length}</dd>
							<dt>Sources</dt>
							<dd>{dataSources.length}</dd>
							<dt>New entities</dt>
							<dd>{page.new_entity_count}</dd>
							<dt>New topics</dt>
							<dd>{page.new_topic_count}</dd>
							<dt>Page ID</dt>
							<dd class="metadata-mono">{page.id}</dd>
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
				{/if}
			</div>
		</article>
	</div>
</div>

<style>
	/* ── the toolbar: Article | Record, Notes, Edit ── */
	.day-bar {
		display: flex;
		align-items: center;
		gap: 1.125rem;
		max-width: 53.5rem;
		height: 2.5rem;
		margin: 0 auto;
	}

	.bar-gap {
		flex: 1;
	}

	.bar-action {
		border: none;
		background: none;
		padding: 0.25rem 0;
		font-family: var(--font-sans);
		font-size: 0.78125rem;
		color: var(--color-foreground-subtle);
		cursor: pointer;
	}

	.bar-action:hover,
	.bar-action.active {
		color: var(--color-foreground);
	}

	.segmented {
		display: flex;
		gap: 2px;
		padding: 2px;
		background: var(--color-surface-elevated);
		border-radius: 6px;
	}

	.seg {
		padding: 0.25rem 0.75rem;
		border: 1px solid transparent;
		background: transparent;
		color: var(--color-foreground-subtle);
		font-family: var(--font-sans);
		font-size: 0.75rem;
		border-radius: 6px;
		cursor: pointer;
	}

	.seg:hover {
		color: var(--color-foreground);
	}

	.seg.active {
		background: var(--color-background);
		color: var(--color-foreground);
		border-color: var(--color-border);
	}

	.notes-popover {
		width: 20rem;
		padding: 0.875rem 1rem;
	}

	.notes-title {
		margin: 0 0 0.5rem;
		font-family: var(--font-serif);
		font-size: 1.0625rem;
	}

	.notes-hint {
		margin: 0.5rem 0 0;
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
	}

	/* ── Article ↔ Record: the same day, along its clock ── */
	.day-title {
		view-transition-name: day-title;
	}

	.day-eyebrow {
		view-transition-name: day-eyebrow;
	}

	#dayline {
		view-transition-name: day-clock;
	}

	:global(::view-transition-group(*)) {
		animation-duration: 350ms;
		animation-timing-function: cubic-bezier(0.22, 1, 0.36, 1);
	}

	/* ── the page head ── */
	.day-eyebrow {
		margin: 0;
		font-family: var(--font-sans);
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
	}

	.day-abstract {
		max-width: 40rem;
		margin-bottom: 1.375rem;
	}

	/* The Abstract is the line read first and most: a size above the body. */
	.day-abstract :global(.markdown p) {
		font-size: 1.375rem;
		line-height: 1.45;
		color: var(--color-foreground);
	}

	/* ── the days on either side ── */
	.adjacent {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: 0.75rem;
		max-width: 40rem;
		margin-top: 2.5rem;
	}

	.adjacent-card {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		border: none;
		text-align: left;
		background: var(--color-surface-elevated);
		border-radius: 12px;
		padding: 0.875rem 1rem;
		cursor: pointer;
	}

	.adjacent-next {
		text-align: right;
	}

	.adjacent-when {
		font-family: var(--font-sans);
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
	}

	.adjacent-abstract {
		font-family: var(--font-serif);
		font-size: 1rem;
		line-height: 1.4;
		color: var(--color-foreground);
		display: -webkit-box;
		-webkit-line-clamp: 3;
		line-clamp: 3;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	/* ── Similar days ── */
	.similar {
		max-width: 40rem;
		margin-top: 2.5rem;
	}

	.similar-title {
		margin: 0 0 0.5rem;
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: 1.5rem;
	}

	.similar-row {
		display: flex;
		gap: 1rem;
		align-items: baseline;
		width: 100%;
		border: none;
		background: none;
		padding: 0.375rem 0;
		text-align: left;
		cursor: pointer;
	}

	.similar-date {
		flex-shrink: 0;
		width: 6.5rem;
		font-family: var(--font-sans);
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
	}

	.similar-abstract {
		font-family: var(--font-serif);
		font-size: 1rem;
		line-height: 1.4;
		color: var(--color-foreground);
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	.similar-row:hover .similar-abstract {
		color: var(--color-primary);
	}

	/* ── Record, arrived at from a citation ── */
	.cited {
		background: var(--color-surface-elevated);
		border-radius: 12px;
		padding: 0.875rem 1rem;
		margin-bottom: 1.5rem;
	}

	.cited-key {
		margin: 0 0 0.375rem;
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
	}

	.cited-row {
		display: flex;
		gap: 0.75rem;
		margin: 0;
		font-size: 0.8125rem;
	}

	.cited-time {
		color: var(--color-foreground-subtle);
	}

	.cited-label {
		color: var(--color-foreground);
	}

	.cited-preview {
		margin: 0.375rem 0 0;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}

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
		padding: 0.5rem 2rem 2rem;
	}

	.day-article::-webkit-scrollbar {
		display: none;
	}


	.day-content {
		max-width: 53.5rem;
		width: 100%;
		margin: 0 auto;
		padding-top: 1rem;
		padding-bottom: 4rem;
	}

	/* Header: title-page layout — h1, meta, rule, all centered */
	.day-header {
		margin-bottom: 1.25rem;
		max-width: 40rem;
	}



	.day-title {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 2.625rem;
		font-weight: 400;
		color: var(--color-foreground);
		margin: 0;
		line-height: 1.2;
		letter-spacing: -0.01em;
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

	/* The day on the map: a quiet verb above the Dayline, since the Timeline
	   is where the same day is read in space. */
	.to-timeline {
		margin: 0 0 24px;
		font-size: 14px;
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
		background: color-mix(in srgb, var(--wash-ink) 4%, transparent);
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
		background: color-mix(in srgb, var(--wash-ink) 8%, transparent);
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
		background: color-mix(in srgb, var(--wash-ink) 6%, transparent);
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
