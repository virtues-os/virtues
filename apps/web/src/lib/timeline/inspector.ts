/**
 * inspector.ts - the day as the inspector reads it: sections for where you were (a
 * stay, a drive, a signal gap, In Bed), and rows for what happened (a
 * conversation, a walk) filed under the section that holds their middle
 * (dayback/src/main.js:1562-1584). The highlight never climbs back up: a
 * section stays lit for its whole span while its rows light and dim inside it.
 */
import { gapVerdict } from "./day";

/** One stretch of the day, as `/api/timeline/derived` serves it. */
export interface DerivedSpan {
	id: string;
	kind: "stay" | "transit" | "sleep" | "unknown";
	started_at: string;
	ended_at: string;
	timeline_place_id: string | null;
	metadata: Record<string, unknown>;
}

export interface DerivedMoment {
	id: string;
	kind: "conversation" | "walk";
	started_at: string;
	ended_at: string;
	title: string | null;
	metadata: Record<string, unknown>;
}

export interface DerivedPlace {
	id: string;
	latitude: number;
	longitude: number;
	/** The person named it; an unnamed place carries the resolver's "Location …". */
	is_named: boolean;
	place_name: string | null;
}

/** One transcription window, as `/api/timeline/voice` serves it. */
export interface VoiceWindow {
	id: string;
	started_at: string;
	ended_at: string;
	/** Two or more is a conversation; none is silence the mic still recorded. */
	speaker_count: number;
	title: string | null;
	text: string | null;
	/** Names spoken in it: mentioned, never known to be present. */
	people: string[];
}

export interface DerivedWindow {
	spans: DerivedSpan[];
	moments: DerivedMoment[];
	/** The last stay that ended before the window opens. */
	last_stay_before: DerivedSpan | null;
	places: DerivedPlace[];
}

export type SectionKind = "place" | "transit" | "gap" | "sleep";

/** What a click on the inspector picked: a section or a row, by kind and span. */
export interface InspectorPick {
	kind: SectionKind | "conversation" | "walk";
	s: number;
	e: number;
}

export interface InspectorRow {
	kind: "conversation" | "walk";
	s: number;
	e: number;
	title: string;
	dur: string;
	/** Conversation only: the transcription windows it joined. */
	windowIds: string[];
	/** Conversation only: the two-or-more-speaker windows over it, in order -
	 *  what its transcript reads (main.js:1575). */
	convs: VoiceWindow[];
}

export interface InspectorSection {
	kind: SectionKind;
	s: number;
	e: number;
	title: string;
	dur: string;
	/** Up to two lines under the title: only a signal gap explains itself. */
	notes: string[];
	placeId: string | null;
	/** A stay only: the person named its place. */
	named: boolean;
	rows: InspectorRow[];
	/** A stay's audio note: "no audio" or "silent", only when clear-cut. */
	atag: "no audio" | "silent" | null;
	/** In Bed only: where the night came from. */
	src: "healthkit" | "quiet_hours" | null;
}

const HOUR = 3_600_000;

/** "45m", "2h", "2h 5m" (main.js:1561). */
export function fmtDur(ms: number): string {
	const m = Math.round(ms / 60_000);
	if (m < 60) return `${m}m`;
	const h = Math.floor(m / 60);
	const r = m % 60;
	return r ? `${h}h ${r}m` : `${h}h`;
}

/** A place's title: the name the person gave it, else "Unnamed place". The
 *  wiki names a place nobody named "Location <lat>, <lon>"; that is no title
 *  to show. */
export function placeTitle(p: DerivedPlace | undefined): string {
	return p?.is_named && p.place_name ? p.place_name : "Unnamed place";
}

/** Faster than anything on the ground: only a flight reaches it. */
const FLIGHT_KMH = 350;
/** A walk keeps a walker's pace on average and never spikes past a jog.
 *  GPS gets that right; it cannot tell a car from a bus, a train or a ride,
 *  so everything between is "Moving" with what was measured. */
const WALK_AVG_KMH = 7;
const WALK_PEAK_KMH = 15;

/** "12 km", "3.4 km", "800 m". */
export function fmtDistance(meters: number): string {
	if (meters < 1000) return `${Math.round(meters / 10) * 10} m`;
	const km = meters / 1000;
	return `${km < 10 ? km.toFixed(1) : Math.round(km)} km`;
}

/** A trip's title: its mode only where speed alone proves it (Flight,
 *  Walk), else "Moving"; then how far the track went. */
export function movementTitle(pathMeters: number, peakKmh: number, ms: number): string {
	const avgKmh = ms > 0 ? pathMeters / 1000 / (ms / HOUR) : 0;
	const mode =
		peakKmh >= FLIGHT_KMH
			? "Flight"
			: pathMeters > 0 && avgKmh < WALK_AVG_KMH && peakKmh < WALK_PEAK_KMH
				? "Walk"
				: "Moving";
	return pathMeters > 0 ? `${mode} · ${fmtDistance(pathMeters)}` : mode;
}

/** A stay of 25 minutes or more with no conversation filed under it gets an
 *  audio note, only when clear-cut (main.js:1577-1579): the mic on under 10 %
 *  of it is "no audio"; over 60 % is "silent"; anything between says
 *  nothing, never over-claiming. A window at all means the mic was on. */
export function audioTag(s: number, e: number, windows: { s: number; e: number }[]): "no audio" | "silent" | null {
	let on = 0;
	for (const v of windows) {
		const lo = Math.max(s, v.s);
		const hi = Math.min(e, v.e);
		if (hi > lo) on += hi - lo;
	}
	const cov = on / (e - s || 1);
	return cov < 0.1 ? "no audio" : cov > 0.6 ? "silent" : null;
}

/** The day's sections and rows, clipped to [start, end); `voice` is every
 *  transcription window over the day. */
export function buildInspector(w: DerivedWindow, start: number, end: number, voice: VoiceWindow[] = []): InspectorSection[] {
	const places = new Map(w.places.map((p) => [p.id, p]));
	const t = (iso: string) => Date.parse(iso);
	const inDay = w.spans
		.map((s) => ({ ...s, s: t(s.started_at), e: t(s.ended_at) }))
		.filter((s) => s.e > start && s.s < end)
		.sort((a, b) => a.s - b.s);
	const built: InspectorSection[] = inDay.map((s) => {
		const cs = Math.max(s.s, start);
		const ce = Math.min(s.e, end);
		const base = {
			s: cs,
			e: ce,
			placeId: s.timeline_place_id,
			named: false,
			rows: [] as InspectorRow[],
			notes: [] as string[],
			atag: null,
			src: null,
		};
		switch (s.kind) {
			case "stay": {
				const place = places.get(s.timeline_place_id ?? "");
				return { ...base, kind: "place", title: placeTitle(place), named: !!place?.is_named, dur: fmtDur(ce - cs) };
			}
			case "transit":
				return {
					...base,
					kind: "transit",
					title: movementTitle(Number(s.metadata.path_meters ?? 0), Number(s.metadata.peak_kmh ?? 0), s.e - s.s),
					dur: fmtDur(ce - cs),
				};
			case "sleep": {
				// A night keeps its full length, though the day only shows its part.
				const src = s.metadata.source === "healthkit" ? "healthkit" : "quiet_hours";
				return { ...base, kind: "sleep", title: "In Bed", dur: fmtDur(s.e - s.s), src };
			}
			case "unknown": {
				// Says only what the record knows: no location, and what the
				// phone did meanwhile. Where you probably were is left to the
				// reader, who can see the stays on either side (the owner's
				// call; the prototype said "likely still at" the stay beside it).
				const verdict = gapVerdict({
					fixes: Number(s.metadata.fix_count ?? 0),
					talk: Number(s.metadata.conversation_count ?? 0) > 0,
					steps: Number(s.metadata.step_bin_count ?? 0) > 0,
				});
				return { ...base, kind: "gap", title: "Signal gap", dur: fmtDur(ce - cs), notes: ["No location recorded", verdict] };
			}
		}
	});
	// In the order the day shows them: by where each starts inside the day,
	// a night before the stay it began in when both open at midnight (the
	// prototype lists the nights first, main.js:1566-1572).
	const sections = built.sort((a, b) => a.s - b.s || (a.kind === "sleep" ? 0 : 1) - (b.kind === "sleep" ? 0 : 1));

	const windows = voice.map((v) => ({ w: v, s: t(v.started_at), e: t(v.ended_at) }));
	const talk = windows.filter((v) => v.w.speaker_count >= 2);
	const rows: InspectorRow[] = w.moments
		.map((m) => ({ ...m, s: t(m.started_at), e: t(m.ended_at) }))
		.filter((m) => m.e > start && m.s < end)
		.sort((a, b) => a.s - b.s)
		.map((m) => ({
			kind: m.kind,
			s: m.s,
			e: m.e,
			title: m.kind === "walk" ? walkTitle(m.metadata) : (m.title ?? "Conversation"),
			dur: fmtDur(m.e - m.s),
			windowIds: (m.metadata.window_ids as string[] | undefined) ?? [],
			convs: m.kind === "conversation" ? talk.filter((c) => c.s < m.e && c.e > m.s).map((c) => c.w) : [],
		}));
	// Filed under the section that had begun most recently at the row's middle:
	// a conversation can start seconds before the stay's arrival stamp.
	for (const r of rows) {
		const mid = (r.s + r.e) / 2;
		let home: InspectorSection | undefined;
		for (const sec of sections) if (sec.s <= mid) home = sec;
		(home ?? sections[0])?.rows.push(r);
	}
	for (const sec of sections) {
		if (sec.kind === "place" && !sec.rows.length && sec.e - sec.s >= 25 * 60_000) sec.atag = audioTag(sec.s, sec.e, windows);
	}
	return sections;
}

function walkTitle(meta: Record<string, unknown>): string {
	const km = Number(meta.path_meters ?? 0) / 1000;
	return `${Number(meta.kmh ?? 0) < 8 ? "Walk" : "Moved"} · ${km.toFixed(1)} km`;
}
