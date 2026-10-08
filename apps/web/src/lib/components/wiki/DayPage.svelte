<!--
	DayPage.svelte

	One day, two views of it (agents/plan/day-article-plan.md):
	- Article: the date, the year and the weather under it, the Abstract and
	  your numbers, and the body with its margin (DayArticleBody), then the
	  days on either side.
	- Data: the evidence: the day line, the places, the event timeline, and
	  every record of the day. A sentence's source opens Data on its record.
	Write a note writes in the margin about the whole day (nothing reads day
	notes yet); Edit opens the article page. The ⋯ menu holds the rest.
	Rewrite this page, there on a past day with a page, asks your server to
	write the page again from the day's record; it keeps the current page in
	History, asks before replacing changes you made, and says how it went in
	a line above the Abstract (lib/wiki/dayRewrite.ts).
-->

<script lang="ts">
	import { subjectHref } from "$lib/wiki/links";
	import { browser } from "$app/environment";
	import { tick, untrack } from "svelte";
	import type { DayEvent } from "$lib/wiki/types";
	import {
		getDaySources,
		getDayEvents,
		getDayTimeline,
		getDayFacts,
		getDayByDate,
		getSimilarDays,
		listNotes,
		createNote,
		resolveNote,
		type WikiNote,
		type SimilarDayApi,
		getArticle,
		getDayRewrite,
		rewriteDay,
		paintDay,
		type DayPictureStyle,
		revertArticle,
		type DayRewriteStatus,
		type RevertOutcome,
		type DayFactsApi,
		type DaySourceApi,
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
	import DayPlaces from "./DayPlaces.svelte";
	import DayDatePicker from "./DayDatePicker.svelte";
	import UniversalDataGrid, { type Column } from "$lib/components/datagrid/UniversalDataGrid.svelte";
	import DayArticleBody from "./DayArticleBody.svelte";
	import DayNumbers from "./DayNumbers.svelte";
	import DayInline from "./DayInline.svelte";
	import { ApiError, getRecord, getAssistantProfile, updateUiPreferences } from "$lib/api/client";
	import { confirmAction } from "$lib/stores/dialog.svelte";
	import { messageBody, nearTurns } from "$lib/wiki/recordWords";
	import type { NoteAnchor } from "$lib/wiki/dayNotes";
	import DayGloss from "./DayGloss.svelte";
	import { parseDayArticle, abstractOf, veilMarks } from "$lib/wiki/dayArticle";
	import {
		IDLE,
		POLL_MS,
		STOPPED,
		UNKNOWN,
		SHOW_FAILED,
		StatusAsks,
		keepsAsking,
		CONFIRM_TITLE,
		CONFIRM_LABEL,
		CANCEL_LABEL,
		readStatus,
		afterUnansweredStart,
		readRefusal,
		failedCopy,
		putBackCopy,
		confirmBody,
		anchoredNoteCount,
		movedNoteCount,
		movedNotesCopy,
		type RewriteLine,
		type StartRefusal,
	} from "$lib/wiki/dayRewrite";
	import { veiled } from "$lib/actions/veil";
	import { veil } from "$lib/stores/veil.svelte";
	import { Popover } from "$lib/floating";

	import Button from "$lib/components/Button.svelte";
	import IconButton from "$lib/components/IconButton.svelte";
	import MenuItem from "$lib/components/MenuItem.svelte";

	interface Props {
		/** The wire shape. See PersonPage for why the converter is gone. */
		page: WikiDayApi;
	}

	let { page }: Props = $props();

	// `page.date` is an ISO day string. Read it as a LOCAL date — `new Date`
	// on a bare `YYYY-MM-DD` is UTC midnight, which is the previous day for
	// everyone west of Greenwich, and this one feeds the chart's axis.
	const date = $derived(parseDateSlug(page.date));
	/** The title: "Saturday, March 14". The year sits on the line under it. */
	const titleLabel = $derived(
		date.toLocaleDateString("en-US", { weekday: "long", month: "long", day: "numeric" }),
	);

	// Shared hover state for chart ↔ timeline sync
	let hoveredEventId = $state<string | null>(null);

	// Timeline component ref for expand/collapse all
	let timelineRef = $state<{ toggleAll: () => void; allExpanded: boolean } | null>(null);

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

	const loadEvents = makeLoader(
		(slug) => getDayEvents(slug),
		(result) => {
			dayEvents = result ? result.map(apiToDayEvent) : [];
		},
	);

	$effect(() => {
		if (browser && page?.date) loadEvents(currentDateSlug);
	});

	// ─────────────────────────────────────────────────────────────────────────
	// The day's prose (read-only here)
	// ─────────────────────────────────────────────────────────────────────────
	// READ-ONLY here. The day's prose lives on its article page, and `Edit`
	// (openDayArticle, below) opens that page in the real editor.
	//
	// Don't add an inline editor that writes the page's `content`. Once a page
	// has been opened, its live Yjs document wins, and a plain content write
	// under it is lost or doubles the page. The nightly writer keeps the same
	// rule: `save_day_article` writes through the pool only while
	// `yjs_state IS NULL` and otherwise leaves the page as it is. Rewrite this
	// page goes through your server's live document instead. The page editor
	// is CRDT-aware; one editor, and it is that one.
	//
	// `summaryText` follows the page you were handed, and Show it / Put the
	// earlier page back set it to what your server has now.
	let summaryText = $state((page.article ?? "") || "");

	$effect(() => {
		summaryText = (page.article ?? "") || "";
	});

	// The day article IS a page: Edit opens the page editor. The nightly writes
	// a day's page once, and never over changes you made or over a page you've
	// opened. After that, Rewrite this page writes it again when you ask, and
	// asks before replacing changes you made.
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
	// No entities section: listing a day's entities is a thing to build from
	// `wiki_refs`, not a stub to draw from a list nothing fills.
	const showSources = $derived(dataSources.length > 0);

	const hasAnyContent = $derived(showAutobiography || showTimeline || hasLocationData || showSources);

	// ─────────────────────────────────────────────────────────────────────────
	// Article | Data
	// ─────────────────────────────────────────────────────────────────────────
	let view = $state<"article" | "record">("article");
	/** The record a citation opened Data on, `table:id`. */
	let citedRef = $state<string | null>(null);

	/** Where the citation came from: the sentence's paragraph block and index. */
	let citedAt = $state<{ block: number; sentence: number } | null>(null);

	function openCitation(ref: string, at: { block: number; sentence: number } | null) {
		citedRef = ref;
		citedAt = at;
		view = "record";
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
		view = "record";
	}

	/** Back to the article, and to the sentence the citation came from. */
	async function backToArticle() {
		const ref = citedRef;
		const at = citedAt;
		view = "article";
		citedRef = null;
		citedAt = null;
		if (!ref) return;
		await tick();
		// The sentence that was clicked, by its place; several sentences in
		// one scene often cite the same recording, so the ref alone is not
		// enough. A table or photo has no place, so it goes by its ref.
		const sentence =
			(at && scrollContainerEl?.querySelector<HTMLElement>(`[data-block="${at.block}"] .s[data-s="${at.sentence}"]`)) ||
			scrollContainerEl?.querySelector<HTMLElement>(`[data-refs~="${CSS.escape(ref)}"]`);
		if (!sentence) return;
		sentence.scrollIntoView({ block: "center" });
		sentence.classList.remove("flash");
		void sentence.offsetWidth;
		sentence.classList.add("flash");
		setTimeout(() => sentence.classList.remove("flash"), 1700);
		// Focus goes to the sentence's keyboard control, the next element.
		(sentence.nextElementSibling as HTMLElement | null)?.focus({ preventScroll: true });
	}

	/** The cited record's row in the day's sources, when the sources carry it. */
	const citedRow = $derived.by(() => {
		if (!citedRef) return null;
		const id = citedRef.split(":").slice(1).join(":");
		return dataSources.find((s) => s.id === id) ?? null;
	});

	/**
	 * A cited record the day's list doesn't carry (a recording chunk is never
	 * a day source), fetched whole so the card can still show its words: the
	 * same words the evidence card showed (lib/wiki/recordWords.ts).
	 */
	let citedRecord = $state<{ ref: string; time: string; kind: string; lines: string[] } | null>(null);
	let citedFailed = $state<string | null>(null);
	$effect(() => {
		const ref = citedRef;
		const at = citedAt;
		citedFailed = null;
		if (!ref || citedRow) {
			citedRecord = null;
			return;
		}
		const [table, ...rest] = ref.split(":");
		getRecord(table, rest.join(":"))
			.then((r) => {
				if (citedRef !== ref) return;
				const row = r.row as Record<string, unknown>;
				const when = String(row[r.timestamp_column] ?? "");
				const sentence = at ? (parsed.blocks[at.block]?.sentences[at.sentence]?.markdown ?? "") : "";
				const lines =
					table === "data_communication_transcription"
						? nearTurns(String(row.text ?? ""), sentence, 6).turns.map((t) => t.text)
						: [messageBody(row, 1200)].filter(Boolean);
				citedRecord = {
					ref,
					time: when ? new Date(when).toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit", timeZone: rowTz }) : "",
					kind: r.display_name,
					lines,
				};
			})
			.catch(() => {
				if (citedRef !== ref) return;
				citedRecord = null;
				citedFailed = ref;
			});
	});

	const parsed = $derived(parseDayArticle(summaryText));

	/**
	 * The Abstract with its first word set apart, for the semibold lead. An
	 * Abstract that opens on anything but a plain word (a link, a veiled
	 * passage, emphasis) keeps its drop cap and has no lead word.
	 */
	const abstractLead = $derived.by(() => {
		const m = /^([\p{L}\p{N}][\p{L}\p{N}'’-]*)(.*)$/su.exec(parsed.abstract);
		return { word: m?.[1] ?? "", ...veilMarks(m ? m[2] : parsed.abstract) };
	});

	// ─────────────────────────────────────────────────────────────────────────
	// The day's facts, your numbers, your notes, and the days on either side
	// ─────────────────────────────────────────────────────────────────────────
	/** The weather and the recorded stretches; the margin marks its silences from the latter. */
	let facts = $state<DayFactsApi | null>(null);
	const loadFacts = makeLoader((slug) => getDayFacts(slug), (r) => (facts = r));
	$effect(() => {
		if (browser && page?.date) loadFacts(currentDateSlug);
	});

	const fahrenheit = (c: number) => Math.round((c * 9) / 5 + 32);
	/** "73° / 51°" under the title; left out, never "no data", when the day has none. */
	const weather = $derived(
		facts?.temperature_high_c != null && facts?.temperature_low_c != null
			? `${fahrenheit(facts.temperature_high_c)}° / ${fahrenheit(facts.temperature_low_c)}°`
			: null,
	);

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

	/** Your pinned numbers (`lane:id`), the same on every day: undefined while
	 *  loading, null before you've chosen any, [] when you chose none. */
	let pins = $state<string[] | null | undefined>(undefined);
	let pinsFailed = $state(false);
	/** How your days' pictures are made, the same on every day: oil until you choose. */
	let pictures = $state<DayPictureStyle | "off">("oil");
	$effect(() => {
		if (!browser) return;
		getAssistantProfile<{ ui_preferences?: Record<string, unknown> }>()
			.then((p) => {
				const saved = p?.ui_preferences?.day_measures;
				pins = Array.isArray(saved) ? saved.filter((k): k is string => typeof k === "string") : null;
				const style = p?.ui_preferences?.day_pictures;
				pictures = PICTURE_CHOICES.some((c) => c.value === style) ? (style as DayPictureStyle | "off") : "oil";
			})
			// Unreadable: show the starters; a later save reads the profile again.
			.catch(() => (pins = null));
	});

	/** Saves run one after another, so the last change you made is the one kept. */
	let pinSave: Promise<void> = Promise.resolve();
	function savePins(next: string[]) {
		const prev = pins;
		pins = next;
		pinsFailed = false;
		pinSave = pinSave
			.then(() => updateUiPreferences({ day_measures: next }))
			.catch(() => {
				pins = prev;
				pinsFailed = true;
			});
	}

	/** Your open notes on this day, drawn in the margin. */
	let dayNotes = $state<WikiNote[]>([]);
	const loadNotes = makeLoader((_slug) => listNotes("day", page.id), (r) => (dayNotes = r ?? []));
	$effect(() => {
		if (browser && page?.id) loadNotes(currentDateSlug);
	});

	async function writeNote(body: string, anchor: NoteAnchor | null): Promise<WikiNote> {
		const n = await createNote("day", page.id, body, { anchor: anchor ?? undefined });
		dayNotes = [n, ...dayNotes];
		return n;
	}

	/** Removing waits a few seconds, with an Undo, before the note is dismissed. */
	const UNDO_MS = 5000;
	let removing = $state<number[]>([]);
	let removeFailed = $state<number[]>([]);
	const removeTimers = new Map<number, ReturnType<typeof setTimeout>>();

	function removeNote(id: number) {
		removeFailed = removeFailed.filter((x) => x !== id);
		removing = [...removing, id];
		removeTimers.set(
			id,
			setTimeout(async () => {
				removeTimers.delete(id);
				try {
					await resolveNote(id, "dismissed");
					dayNotes = dayNotes.filter((n) => n.id !== id);
				} catch {
					removeFailed = [...removeFailed, id];
				} finally {
					removing = removing.filter((x) => x !== id);
				}
			}, UNDO_MS),
		);
	}

	function undoRemove(id: number) {
		clearTimeout(removeTimers.get(id));
		removeTimers.delete(id);
		removing = removing.filter((x) => x !== id);
	}

	let body: { writeAboutDay: () => void } | null = $state(null);

	/** No hover and a coarse pointer: a phone or tablet, where "hold V" means nothing. */
	const touchOnly = browser && window.matchMedia("(hover: none) and (pointer: coarse)").matches;

	// ─────────────────────────────────────────────────────────────────────────
	// Rewrite this page
	// ─────────────────────────────────────────────────────────────────────────
	// Your server writes the page in the background, so this page only reads
	// where that stands: on arrival at a past day with a page, then every few
	// seconds while it runs (or while a start that got no answer leaves it
	// unknown) and the page is on screen, and at once when you come back to
	// it. Rewrite this page appears once your server has answered that the
	// day has a page, so a server without the rewrite never offers it; until
	// an answer comes, coming back to the page asks again. What each answer
	// says is lib/wiki/dayRewrite.ts.

	/** The line above the Abstract. */
	let rewriteLine = $state.raw<RewriteLine>(IDLE);
	const rewriteRunning = $derived(rewriteLine.kind === "running");
	/** From your server on every ask. When true, the confirm says the rewrite replaces your changes. */
	let hasYourEdits = $state(false);
	/** The start request is out. */
	let rewriteSending = $state(false);
	let showingRewrite = $state(false);
	let puttingBack = $state(false);

	/** The confirm is open or the start request is out: one start at a time. */
	let rewriteStarting = false;
	/** This client saw the day's rewrite running, so how it ends is news here. */
	let watching = false;
	/** `has_page` from your server's latest answer about this day; null until one comes. */
	let rewriteHasPage = $state<boolean | null>(null);
	/** The `started_at` of the last status the page read about this day. */
	let lastStartedAt: string | null = null;
	/** Once a start came back here: `lastStartedAt` as it was before that start (`Seen.before`). */
	let startedAfter: string | null | undefined = undefined;
	/** Bumped when the day changes or the page goes: answers for another day are dropped. */
	let rewriteSeq = 0;
	/** One ask out at a time (focus and visibility fire together), and none read from before a start. */
	const statusAsks = new StatusAsks();
	let rewriteTimer: ReturnType<typeof setTimeout> | null = null;

	function stopRewritePolling() {
		if (rewriteTimer) clearTimeout(rewriteTimer);
		rewriteTimer = null;
	}

	function scheduleRewriteAsk(slug: string) {
		stopRewritePolling();
		if (!document.hidden) rewriteTimer = setTimeout(() => void askRewrite(slug), POLL_MS);
	}

	/**
	 * Ask where the day's rewrite stands, and again in a few seconds while the
	 * line waits on your server. Resolves true when your server's answer was
	 * read: it answered, about this day, after the latest start.
	 */
	async function askRewrite(slug: string): Promise<boolean> {
		const ask = statusAsks.send(rewriteSeq);
		if (!ask) return false;
		stopRewritePolling();
		let s: DayRewriteStatus | null = null;
		try {
			s = await getDayRewrite(slug);
		} catch {
			// A failed ask says nothing about the rewrite: keep the line and ask again.
		}
		const { current, again } = statusAsks.landed(ask, rewriteSeq);
		if (ask.day !== rewriteSeq) return false;
		// An answer to an ask sent before the latest start can't say how that
		// start went, so it changes nothing, and the asking goes on.
		if (s && current) {
			hasYourEdits = s.has_your_edits;
			rewriteHasPage = s.has_page === true;
			lastStartedAt = s.started_at ?? null;
			const next = readStatus(s, { watched: watching, before: startedAfter }, Date.now());
			watching = next.kind === "running";
			rewriteLine = next;
		}
		if (keepsAsking(rewriteLine)) {
			// An ask that waited behind a dropped answer still wants one now.
			if (again && !current) void askRewrite(slug);
			else scheduleRewriteAsk(slug);
		}
		return s !== null && current;
	}

	$effect(() => {
		const slug = currentDateSlug;
		const ask = browser && showAutobiography && slug < todaySlug;
		untrack(() => {
			rewriteSeq++;
			stopRewritePolling();
			watching = false;
			rewriteLine = IDLE;
			hasYourEdits = false;
			rewriteHasPage = ask ? null : false;
			lastStartedAt = null;
			startedAfter = undefined;
			showingRewrite = false;
			puttingBack = false;
			if (ask) void askRewrite(slug);
		});
		return () => {
			rewriteSeq++;
			stopRewritePolling();
		};
	});

	$effect(() => {
		if (!browser) return;
		const wake = () => {
			if (!document.hidden && (keepsAsking(rewriteLine) || rewriteHasPage === null)) void askRewrite(currentDateSlug);
		};
		window.addEventListener("focus", wake);
		document.addEventListener("visibilitychange", wake);
		return () => {
			window.removeEventListener("focus", wake);
			document.removeEventListener("visibilitychange", wake);
		};
	});

	/**
	 * Confirm, then ask your server to start. The confirm uses what the page
	 * said on arrival; when your server says the page now holds changes you
	 * made, it asks again with that line, and only a confirm that said so
	 * sends your consent to replace them.
	 */
	async function rewritePage() {
		if (rewriteStarting || rewriteRunning) return;
		const slug = currentDateSlug;
		const seq = rewriteSeq;
		rewriteStarting = true;
		try {
			let withEdits = hasYourEdits;
			for (;;) {
				const ok = await confirmAction({
					title: CONFIRM_TITLE,
					body: confirmBody(withEdits, anchoredNoteCount(parsed.blocks, dayNotes)),
					confirmLabel: CONFIRM_LABEL,
					cancelLabel: CANCEL_LABEL,
				});
				if (!ok || seq !== rewriteSeq) return;
				rewriteSending = true;
				const before = lastStartedAt;
				let refusal: StartRefusal | null = null;
				try {
					await rewriteDay(slug, { replaceEdits: withEdits });
				} catch (e) {
					refusal = e instanceof ApiError ? readRefusal(e.status, e.message) : readRefusal(null, null);
				} finally {
					rewriteSending = false;
				}
				if (seq !== rewriteSeq) return;
				// However it came back, an answer to an ask sent before now can't
				// say how this start went, nor replace the line it sets. From here
				// a finish this page hadn't read before the start is news.
				statusAsks.started();
				startedAfter = before;
				if (!refusal || refusal.kind === "running") {
					watching = true;
					rewriteLine = { kind: "running" };
					scheduleRewriteAsk(slug);
					return;
				}
				if (refusal.kind === "consent" && !withEdits) {
					withEdits = true;
					hasYourEdits = true;
					continue;
				}
				if (refusal.kind === "ask") {
					// No answer came back, so it may have started anyway: ask first.
					// An answer with nothing running and no finish this page hadn't
					// read means it didn't start; a running or new finish is as
					// read. A server that doesn't answer leaves it unknown, and the
					// line says so and keeps asking.
					watching = false;
					const answered = await askRewrite(slug);
					if (seq !== rewriteSeq) return;
					if (!answered) {
						rewriteLine = { kind: "unknown" };
						scheduleRewriteAsk(slug);
					} else {
						rewriteLine = afterUnansweredStart(rewriteLine);
					}
					return;
				}
				rewriteLine = { kind: "failed", code: refusal.kind === "failed" ? refusal.code : "failed" };
				return;
			}
		} finally {
			rewriteStarting = false;
		}
	}

	/** The page as your server has it now, with the day it was asked for. */
	async function fetchArticle(slug: string) {
		const d = await getDayByDate(slug).catch(() => null);
		return { slug, article: d?.article || null };
	}

	/** Show it: the new page goes on screen only when you ask, and only on its own day. */
	const loadRewritten = makeLoader(fetchArticle, (r) => {
		showingRewrite = false;
		const line = rewriteLine;
		if (!r || r.slug !== currentDateSlug || line.kind !== "done") return;
		if (!r.article) {
			rewriteLine = { ...line, loadFailed: true };
			return;
		}
		const moved = movedNoteCount(parsed.blocks, parseDayArticle(r.article).blocks, dayNotes);
		summaryText = r.article;
		rewriteLine = { kind: "shown", beforeVersion: line.beforeVersion, moved };
	});

	function showRewritten() {
		// Off while it loads, so a second failure is a new line the status
		// region reads out again.
		if (rewriteLine.kind === "done" && rewriteLine.loadFailed) rewriteLine = { ...rewriteLine, loadFailed: false };
		showingRewrite = true;
		void loadRewritten(currentDateSlug);
	}

	/** How the put-back went, said once the page your server has now is on screen. */
	let putBackOutcome: RevertOutcome = { changed: true, saved: true, message: "" };
	const loadPutBack = makeLoader(fetchArticle, (r) => {
		puttingBack = false;
		if (!r || r.slug !== currentDateSlug) return;
		if (r.article) summaryText = r.article;
		rewriteLine = { kind: "putBack", message: putBackCopy(putBackOutcome, !!r.article) };
	});

	/** Put the earlier page back: the version your server kept before the rewrite. */
	async function putEarlierBack() {
		const line = rewriteLine;
		if (line.kind !== "shown" || line.beforeVersion === null || puttingBack) return;
		const slug = currentDateSlug;
		const seq = rewriteSeq;
		puttingBack = true;
		let outcome: RevertOutcome;
		try {
			outcome = await revertArticle("day", page.id, line.beforeVersion);
		} catch {
			if (seq !== rewriteSeq) return;
			puttingBack = false;
			rewriteLine = { kind: "putBackFailed" };
			return;
		}
		if (seq !== rewriteSeq) return;
		if (outcome.changed && !outcome.saved) {
			// This page reads what your server has saved, which is still the
			// rewrite, so the screen keeps it and the line says to check back.
			puttingBack = false;
			rewriteLine = { kind: "putBack", message: putBackCopy(outcome, false) };
		} else {
			// Also when nothing changed: the page on screen is the rewrite, and
			// your server's page is already the earlier one.
			putBackOutcome = outcome;
			void loadPutBack(slug);
		}
		// Putting a version back counts as your edit, so the next rewrite's
		// confirm should say so; your server is the one that knows.
		getDayRewrite(slug)
			.then((s) => {
				if (seq === rewriteSeq) hasYourEdits = s.has_your_edits;
			})
			.catch(() => {});
	}

	function openHistory() {
		windowShellStore.openTabFromRoute("/wiki/history");
	}

	// ─────────────────────────────────────────────────────────────────────────
	// The ⋯ menu
	// ─────────────────────────────────────────────────────────────────────────
	let menuOpen = $state(false);
	/** How the last Copy link went, said in its row until the menu closes. */
	let linkCopy = $state<"idle" | "copied" | "failed">("idle");
	$effect(() => {
		if (!menuOpen) linkCopy = "idle";
	});

	const canRewrite = $derived(currentDateSlug < todaySlug && rewriteHasPage === true);

	async function copyLink() {
		try {
			await navigator.clipboard.writeText(`${window.location.origin}${subjectHref(`day_${currentDateSlug}`)}`);
			linkCopy = "copied";
		} catch {
			// No clipboard: an insecure origin (a box over plain http) or a refused permission.
			linkCopy = "failed";
		}
	}

	// Paint this day, and how your days' pictures are made (api/day_picture.rs).
	/** The choices, in the menu's order; "off" is no pictures. */
	const PICTURE_CHOICES: { value: DayPictureStyle | "off"; label: string }[] = [
		{ value: "oil", label: "Oil" },
		{ value: "watercolor", label: "Watercolor" },
		{ value: "pencil", label: "Pencil" },
		{ value: "gouache", label: "Gouache" },
		{ value: "off", label: "Off" },
	];
	const PAINT_COPY: Record<string, string> = {
		nothing_to_paint: "Nothing on this page is a place your server can paint without guessing.",
		busy: "Your server is writing this day's page right now. Try painting it in a few minutes.",
		billing: "Billing stopped your server before it could paint this day, so the page hasn't changed. Check Billing, then try again.",
		failed: "Your server couldn't paint this day, so the page hasn't changed. Try again later.",
		update: "Your server needs an update before it can paint days.",
	};

	/** The day your server is painting, while it is; one at a time. */
	let paintingDay = $state<string | null>(null);
	const painting = $derived(paintingDay === currentDateSlug);
	/** How the last painting that changed nothing ended, on its own day. */
	let paintLine = $state<{ day: string; text: string } | null>(null);
	/** A server older than the picture route answers 404; the item stays gone until the page reloads. */
	let paintMissing = $state(false);
	const canPaint = $derived(canRewrite && pictures !== "off" && !paintMissing);

	/** Paint this day in your style; a picture that lands brings the page as your server has it now. */
	async function paintThisDay() {
		if (paintingDay || pictures === "off") return;
		const slug = currentDateSlug;
		paintingDay = slug;
		paintLine = null;
		try {
			const r = await paintDay(slug, pictures);
			if (r.painted) {
				const a = await fetchArticle(slug);
				if (a.slug === currentDateSlug && a.article) summaryText = a.article;
			} else {
				paintLine = { day: slug, text: PAINT_COPY[r.reason ?? "failed"] ?? PAINT_COPY.failed };
			}
		} catch (e) {
			if (e instanceof ApiError && e.status === 404) {
				paintMissing = true;
				paintLine = { day: slug, text: PAINT_COPY.update };
			} else {
				paintLine = { day: slug, text: PAINT_COPY.failed };
			}
		} finally {
			paintingDay = null;
		}
	}

	/** Saved on the same queue as your numbers, so neither save overwrites the other. */
	function choosePictures(next: DayPictureStyle | "off") {
		const prev = pictures;
		if (next === prev) return;
		pictures = next;
		pinSave = pinSave
			.then(() => updateUiPreferences({ day_pictures: next }))
			.catch(() => {
				pictures = prev;
			});
	}

	/**
	 * The menu's keys: focus starts on the first row, ↑ and ↓ move between
	 * rows, and Escape (which the popover handles) hands focus back to ⋯.
	 */
	function menuKeys(node: HTMLElement) {
		const rows = () => [
			...node.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled), [role="menuitemradio"]:not(:disabled)'),
		];
		const trigger = node.closest(".day-more")?.querySelector<HTMLElement>('[aria-haspopup="menu"]');
		rows()[0]?.focus();
		const onkey = (e: KeyboardEvent) => {
			if (e.key === "Escape") {
				trigger?.focus();
				return;
			}
			if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
			e.preventDefault();
			const list = rows();
			const at = list.indexOf(document.activeElement as HTMLButtonElement);
			const next = e.key === "ArrowDown" ? at + 1 : at - 1;
			list[(next + list.length) % list.length]?.focus();
		};
		node.addEventListener("keydown", onkey);
		return { destroy: () => node.removeEventListener("keydown", onkey) };
	}

</script>

{#snippet personGloss({ name, url }: { name: string; url: string })}<DayGloss {name} {url} date={currentDateSlug} onday={(slug) => navigateToDay(parseDateSlug(slug))} />{/snippet}

<div class="day-page-outer">
	<div class="day-page-layout">
		<article class="day-article wiki-article" bind:this={scrollContainerEl}>
			<div class="day-bar" role="toolbar" aria-label="Day">
				{#if view === "record" && citedRef}
					<button type="button" class="bar-action back" aria-label="Back to the sentence" onclick={backToArticle}>← Back<span class="bar-more">{" to the sentence"}</span></button>
				{/if}
				<span class="bar-gap"></span>
				<div class="segmented" role="group" aria-label="View">
					<button type="button" class="seg" class:active={view === "article"} aria-pressed={view === "article"} onclick={backToArticle}>Article</button>
					<button type="button" class="seg" class:active={view === "record"} aria-pressed={view === "record"} onclick={showRecord}>Data</button>
				</div>
				{#if view === "article" && currentDateSlug <= todaySlug}
					<button type="button" class="bar-action" title="Write in the margin about the whole day" onclick={() => body?.writeAboutDay()}>Write a note</button>
				{/if}
				{#if showAutobiography}
					<button
						type="button"
						class="bar-action"
						disabled={rewriteRunning}
						title={rewriteRunning ? "Your server is writing this page again." : undefined}
						onclick={openDayArticle}
					>Edit</button>
				{/if}
				<div class="day-more">
					<Popover bind:open={menuOpen} placement="bottom-end" offset={4}>
						{#snippet trigger({ toggle })}
							<IconButton icon="ri:more-fill" label="More" haspopup="menu" expanded={menuOpen} onclick={toggle} />
						{/snippet}
						{#snippet children({ close })}
							<div class="day-menu" role="menu" aria-label="More" use:menuKeys>
								{#if canRewrite}
									<MenuItem
										label={rewriteSending || rewriteRunning ? "Writing this page…" : "Rewrite this page…"}
										loading={rewriteSending || rewriteRunning}
										onclick={() => {
											close();
											void rewritePage();
										}}
									/>
								{/if}
								{#if canPaint}
									<MenuItem
										label={painting ? "Painting…" : "Paint this day"}
										loading={painting}
										disabled={(paintingDay !== null && !painting) || rewriteRunning}
										onclick={() => {
											close();
											void paintThisDay();
										}}
									/>
								{/if}
								<MenuItem
									label="History"
									onclick={() => {
										close();
										openHistory();
									}}
								/>
								<MenuItem
									label="Open in Timeline"
									onclick={() => {
										close();
										openInTimeline();
									}}
								/>
								<MenuItem
									label={linkCopy === "copied" ? "Link copied" : linkCopy === "failed" ? "Couldn't copy the link" : "Copy link"}
									onclick={copyLink}
								/>
								<div class="menu-rule" role="separator"></div>
								<MenuItem
									label={veil.on ? "Show names" : "Hide names"}
									shortcut={veil.on && !touchOnly ? "Hold V" : undefined}
									onclick={() => {
										close();
										veil.toggle();
									}}
								/>
								<div class="menu-rule" role="separator"></div>
								<div class="menu-pictures" role="group" aria-label="Pictures">
									<span class="menu-label" aria-hidden="true">Pictures</span>
									<div class="picture-choices">
										{#each PICTURE_CHOICES as choice (choice.value)}
											<button
												type="button"
												role="menuitemradio"
												aria-checked={pictures === choice.value}
												class="picture-choice"
												class:on={pictures === choice.value}
												onclick={() => choosePictures(choice.value)}>{choice.label}</button
											>
										{/each}
									</div>
									<span class="menu-hint">About one a week, of a place from your pages</span>
								</div>
							</div>
						{/snippet}
					</Popover>
				</div>
			</div>

			<div class="day-content">
				<header class="day-header">
					<h1 class="day-title">
						<DayDatePicker
							pageDate={date}
							{currentDateSlug}
							{todaySlug}
							onNavigateDay={navigateToDay}
							title={relativeDateLabel() ?? "Go to another day"}
						>
							{#snippet label()}{titleLabel}{/snippet}
						</DayDatePicker>
					</h1>
					<p class="dateline">
						<span>{date.getFullYear()}</span>
						{#if weather}<span class="dot" aria-hidden="true">·</span><span title="High and low, from a weather model">{weather}</span>{/if}
					</p>
				</header>

				{#if view === "article"}
					{#if showAutobiography}
						{#snippet lead()}
							<div role="status">
								{#if rewriteLine.kind === "running"}
									<p class="rewrite-line">Your server is writing this day's page again. You can keep reading, or leave and come back.</p>
								{:else if rewriteLine.kind === "unknown"}
									<p class="rewrite-line">{UNKNOWN}</p>
								{:else if rewriteLine.kind === "stopped"}
									<p class="rewrite-line">{STOPPED}</p>
								{:else if rewriteLine.kind === "done"}
									<p class="rewrite-line">Your server wrote this day's page again. <TextAction inline loading={showingRewrite} onclick={showRewritten}>Show it</TextAction></p>
									{#if rewriteLine.loadFailed}<p class="rewrite-line">{SHOW_FAILED}</p>{/if}
								{:else if rewriteLine.kind === "shown"}
									{@const moved = movedNotesCopy(rewriteLine.moved)}
									<p class="rewrite-line">
										The earlier page is in <TextAction inline onclick={openHistory}>History</TextAction>.
										{#if rewriteLine.beforeVersion !== null}<TextAction inline loading={puttingBack} onclick={putEarlierBack}>Put the earlier page back</TextAction>{/if}
									</p>
									{#if moved}<p class="rewrite-line">{moved}</p>{/if}
								{:else if rewriteLine.kind === "putBack"}
									<p class="rewrite-line">{rewriteLine.message}</p>
								{:else if rewriteLine.kind === "putBackFailed"}
									<p class="rewrite-line">Your server couldn't put the earlier page back. Try again from <TextAction inline onclick={openHistory}>History</TextAction>.</p>
								{:else if rewriteLine.kind === "failed"}
									<p class="rewrite-line">{failedCopy(rewriteLine.code)}</p>
								{/if}
								{#if painting}
									<p class="rewrite-line">Your server is painting a picture for this page. It takes about twenty seconds.</p>
								{:else if paintLine?.day === currentDateSlug}
									<p class="rewrite-line">{paintLine.text}</p>
								{/if}
							</div>
							{#if parsed.abstract}
								<div class="day-abstract" use:veiled={{ hiding: veil.hiding, phrases: abstractLead.phrases }}>
									<div class="markdown markdown--article"><p class="abstract">{#if abstractLead.word}<strong class="lead-word">{abstractLead.word}</strong>{/if}<DayInline markdown={abstractLead.markdown} person={personGloss} /></p></div>
								</div>
							{/if}
							<DayNumbers date={currentDateSlug} {pins} onchange={savePins} saveFailed={pinsFailed} />
						{/snippet}
						<DayArticleBody
							bind:this={body}
							blocks={parsed.blocks}
							{lead}
							date={currentDateSlug}
							timezone={page.start_timezone}
							coverage={facts?.coverage ?? null}
							notes={dayNotes}
							oncite={openCitation}
							person={personGloss}
							onwrite={writeNote}
							onremove={removeNote}
							onundo={undoRemove}
							{removing}
							{removeFailed}
						/>
					{:else}
						{#if currentDateSlug <= todaySlug}
							<!-- No page yet, but the day still has your numbers and your notes. -->
							{#snippet numbersLead()}<DayNumbers date={currentDateSlug} {pins} onchange={savePins} saveFailed={pinsFailed} />{/snippet}
							<DayArticleBody
								bind:this={body}
								blocks={[]}
								lead={numbersLead}
								date={currentDateSlug}
								timezone={page.start_timezone}
								coverage={facts?.coverage ?? null}
								notes={dayNotes}
								onwrite={writeNote}
								onremove={removeNote}
								onundo={undoRemove}
								{removing}
								{removeFailed}
							/>
						{/if}
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
								<button type="button" class="adjacent-day" onclick={() => navigateToDay(prevDay!.date)}>
									<span class="adjacent-when">← {neighborLabel(prevDay)}</span>
									{#if prevDay.abstract}<span class="adjacent-abstract" use:veiled={{ hiding: veil.hiding, whole: true }}>{prevDay.abstract}</span>{:else}<span class="adjacent-none">No page yet</span>{/if}
								</button>
							{:else}
								<span></span>
							{/if}
							{#if nextDay}
								<button type="button" class="adjacent-day adjacent-next" onclick={() => navigateToDay(nextDay!.date)}>
									<span class="adjacent-when">{neighborLabel(nextDay)} →</span>
									{#if nextDay.abstract}<span class="adjacent-abstract" use:veiled={{ hiding: veil.hiding, whole: true }}>{nextDay.abstract}</span>{:else}<span class="adjacent-none">No page yet</span>{/if}
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
							{:else if citedRecord && citedRecord.ref === citedRef}
								<p class="cited-row">
									<span class="cited-time">{citedRecord.time}</span>
									<span>{citedRecord.kind}</span>
								</p>
								{#each citedRecord.lines as line, i (i)}<p class="cited-preview">{line}</p>{/each}
							{:else if citedFailed === citedRef}
								<p class="cited-preview">Couldn't load this record. Check that your server is running, then go back and open it again.</p>
							{:else}
								<p class="cited-preview">Loading the record…</p>
							{/if}
						</section>
					{/if}

					<section class="section" id="dayline">
						<h2 class="section-title">The day line</h2>
						<DaylineChart events={dayEvents} timezone={page.start_timezone} pageDate={date} dayDateSlug={currentDateSlug} />
					</section>

					{#if hasLocationData}
						<section class="section" id="places">
							<h2 class="section-title">Places</h2>
							<DayPlaces {movementStops} {movementTrack} {dedupedMarkers} {hasLocationData} timezone={page.start_timezone} pageDate={date} />
						</section>
					{/if}

					{#if hasAnyContent}
						{#if showTimeline}
							<section class="section" id="timeline">
								<div class="section-header-row">
									<h2 class="section-title">Event timeline</h2>
									<div class="section-actions">
										<Button variant="ghost" size="sm" onclick={() => timelineRef?.toggleAll()}>
											{timelineRef?.allExpanded ? "Collapse all" : "Expand all"}
										</Button>
									</div>
								</div>
								<EventTimeline bind:this={timelineRef} events={dayEvents} timezone={page.start_timezone} {hoveredEventId} onhover={(id) => (hoveredEventId = id)} pageDate={date} />
							</section>
						{/if}

						<!-- Every record of the day, in one chronological table -->
						<section class="section" id="ontologies">
							<h2 class="section-title">Data ontologies</h2>
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
					{:else}
						<div class="empty-state">
							{#if currentDateSlug > todaySlug}
								<p class="empty-state-text">This day hasn't happened yet.</p>
							{:else if currentDateSlug === todaySlug}
								<p class="empty-state-text">Your day is still in progress.</p>
							{:else}
								<p class="empty-state-text">Your record holds nothing from this day.</p>
							{/if}
						</div>
					{/if}
				{/if}
			</div>
		</article>
	</div>
</div>

<style>
	/* ── the toolbar: Article | Data, Write a note, Edit, and ⋯ for the rest.
	   Four controls fit a phone's width; anything more goes in the menu. ── */
	.day-bar {
		display: flex;
		align-items: center;
		gap: 1.125rem;
		max-width: 53.5rem;
		height: 3.25rem;
		margin: 0 auto;
		font-family: var(--font-sans);
		font-size: 0.8125rem;
	}

	.bar-gap {
		flex: 1;
	}

	.bar-action,
	.seg {
		white-space: nowrap;
	}

	.bar-action {
		border: none;
		background: none;
		padding: 0.25rem 0;
		font: inherit;
		color: var(--color-foreground-muted);
		cursor: pointer;
	}

	.bar-action:hover:not(:disabled) {
		color: var(--color-foreground);
	}

	.bar-action:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.segmented {
		display: flex;
		padding: 2px;
		background: var(--color-surface-elevated);
		border-radius: 6px;
	}

	.seg {
		padding: 0.25rem 0.75rem;
		border: 1px solid transparent;
		background: transparent;
		color: var(--color-foreground-subtle);
		font: inherit;
		border-radius: 6px;
		cursor: pointer;
	}

	.seg:hover {
		color: var(--color-foreground);
	}

	/* White on the page, edged by a hairline: no shadows in the pane. */
	.seg.active {
		background: var(--color-surface);
		color: var(--color-foreground);
		border-color: var(--color-border);
	}

	.day-more {
		display: flex;
	}

	.day-menu {
		display: flex;
		flex-direction: column;
		min-width: 15rem;
		padding: 6px;
	}

	.menu-rule {
		height: 1px;
		margin: 6px;
		background: var(--color-border);
	}

	/* Pictures: one row of choices under its label, the way the rows above read. */
	.menu-pictures {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 4px 8px 8px;
		font-family: var(--font-sans);
	}

	.menu-label,
	.menu-hint {
		font-size: 12px;
		color: var(--color-foreground-muted);
	}

	.picture-choices {
		display: flex;
		flex-wrap: wrap;
		gap: 4px;
	}

	.picture-choice {
		padding: 4px 8px;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: transparent;
		color: var(--color-foreground);
		font: inherit;
		font-size: 13px;
		cursor: pointer;
	}

	.picture-choice:hover {
		background: var(--hover-bg);
	}

	.picture-choice.on {
		border-color: var(--color-foreground-muted);
		background: var(--active-bg);
	}

	.picture-choice:focus-visible {
		outline: 2px solid var(--color-border-focus);
		outline-offset: 2px;
	}

	/* A phone: every action stays, with the words that don't fit cut short. */
	@media (max-width: 420px) {
		.day-bar {
			gap: 0.75rem;
		}

		.bar-more {
			display: none;
		}
	}

	/* ── the page head: the date, then the year and the weather ── */
	.dateline {
		display: flex;
		flex-wrap: wrap;
		gap: 0.45rem;
		margin: 0;
		font-family: var(--font-sans);
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
	}

	.dateline .dot {
		color: var(--color-foreground-disabled);
	}

	/* ── the Abstract: body size, its first letter a two-line drop cap and the
	   rest of its first word the one semibold on the page ── */
	.day-abstract {
		max-width: 40rem;
		margin-bottom: 1.4rem;
	}

	.abstract {
		margin: 0;
	}

	/* No color of its own: the cap inherits from the element holding the
	   letter, so a veiled phrase's transparent ink hides it too. */
	.abstract::first-letter {
		-webkit-initial-letter: 2;
		initial-letter: 2;
		/* A cap sitting tight against the lines it spans reads as crowded;
		   this is a little more than a word space at body size. */
		margin-right: 0.35rem;
		font-family: var(--font-serif);
		font-weight: 400;
	}

	@supports not ((initial-letter: 2) or (-webkit-initial-letter: 2)) {
		.abstract::first-letter {
			float: left;
			font-size: 3.55em;
			line-height: 0.8;
			padding: 0.07em 0.35rem 0 0;
		}
	}

	/* The semibold cut is its own family (app.css), so no other bold serif
	   can reach it. A letter the cut lacks falls to the regular, never a
	   synthesized bold. */
	.lead-word {
		font-family: var(--font-serif-lead);
		font-weight: 600;
		font-synthesis-weight: none;
	}

	/* ── Rewrite this page: its news, above the Abstract ── */
	.rewrite-line {
		max-width: 40rem;
		margin: 0 0 1rem;
		font-family: var(--font-sans);
		font-size: 0.875rem;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	/* ── the days on either side: plain text under a rule ── */
	.adjacent {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: 2rem;
		max-width: 40rem;
		margin-top: 3rem;
		padding-top: 1.4rem;
		border-top: 1px solid var(--color-border);
	}

	.adjacent-day {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		border: none;
		background: none;
		padding: 0;
		text-align: left;
		cursor: pointer;
	}

	.adjacent-next {
		text-align: right;
	}

	.adjacent-when,
	.adjacent-none {
		font-family: var(--font-sans);
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
	}

	.adjacent-abstract {
		font-family: var(--font-serif);
		font-size: 1rem;
		line-height: 1.45;
		color: var(--color-foreground-muted);
		display: -webkit-box;
		-webkit-line-clamp: 3;
		line-clamp: 3;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	.adjacent-day:hover .adjacent-when {
		color: var(--color-foreground);
	}

	.adjacent-day:hover .adjacent-abstract {
		color: var(--color-foreground);
	}

	@media (max-width: 560px) {
		.adjacent {
			grid-template-columns: minmax(0, 1fr);
			gap: 1.25rem;
		}

		.adjacent-next {
			text-align: left;
		}
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

	.day-header {
		max-width: 40rem;
		margin: 0.25rem 0 1.6rem;
	}

	.day-title {
		font-family: var(--font-serif);
		font-size: 2.75rem;
		font-weight: 400;
		color: var(--color-foreground);
		margin: 0 0 0.4rem;
		line-height: 1.1;
		letter-spacing: -0.01em;
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

	/* Responsive */
	@media (max-width: 900px) {
		.day-page-layout {
			flex-direction: column;
		}

		.day-article {
			padding: 1rem;
		}
	}

	@media (max-width: 560px) {
		.day-title {
			font-size: 2.25rem;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.source-chip {
			transition: none;
		}
	}
</style>
