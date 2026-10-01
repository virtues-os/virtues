/**
 * rail.ts - the day as the rail reads it: sections for where you were (a
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
	is_home: boolean;
	is_work: boolean;
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
	is_built: boolean;
	spans: DerivedSpan[];
	moments: DerivedMoment[];
	/** The last stay that ended before the window opens. */
	last_stay_before: DerivedSpan | null;
	places: DerivedPlace[];
}

export type SectionKind = "place" | "transit" | "gap" | "sleep";

/** What a click on the rail picked: a section or a row, by kind and span. */
export interface RailPick {
	kind: SectionKind | "conversation" | "walk";
	s: number;
	e: number;
}

export interface RailRow {
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

export interface RailSection {
	kind: SectionKind;
	s: number;
	e: number;
	title: string;
	dur: string;
	/** Up to two lines under the title: only a signal gap explains itself. */
	notes: string[];
	placeId: string | null;
	rows: RailRow[];
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

/** A place's title: Home; else the wiki's name for it (the user's own, as the
 *  prototype's corrections win over its guesses); else "Work / frequent" for
 *  the most-dwelt place (dayback/build.py:209-210). The wiki names a place it
 *  cannot name "Location <lat>, <lon>"; that is no title to show. */
export function placeTitle(p: DerivedPlace | undefined): string {
	if (!p) return "Unnamed place";
	if (p.is_home) return "Home";
	if (p.place_name && !p.place_name.startsWith("Location ")) return p.place_name;
	if (p.is_work) return "Work / frequent";
	return "Unnamed place";
}

/** A drive's title by its fastest hop: Flying from 350 km/h, Driving from 45,
 *  else In transit; hours on the ground with no stay between say so, rather
 *  than calling it one long drive (main.js:1571, 2547). */
export function transitTitle(peakKmh: number, ms: number): string {
	const mode = peakKmh >= 350 ? "Flying" : peakKmh >= 45 ? "Driving" : "In transit";
	return mode !== "Flying" && ms > 2 * HOUR ? "Out · no stay recorded" : mode;
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
export function buildRail(w: DerivedWindow, start: number, end: number, voice: VoiceWindow[] = []): RailSection[] {
	const places = new Map(w.places.map((p) => [p.id, p]));
	const t = (iso: string) => Date.parse(iso);
	const inDay = w.spans
		.map((s) => ({ ...s, s: t(s.started_at), e: t(s.ended_at) }))
		.filter((s) => s.e > start && s.s < end)
		.sort((a, b) => a.s - b.s);
	// The located stretches in order, for "likely still at" beside a gap.
	const located = inDay.filter((s) => s.kind !== "sleep");

	const built: RailSection[] = inDay.map((s) => {
		const cs = Math.max(s.s, start);
		const ce = Math.min(s.e, end);
		const base = { s: cs, e: ce, placeId: s.timeline_place_id, rows: [] as RailRow[], notes: [] as string[], atag: null, src: null };
		switch (s.kind) {
			case "stay":
				return { ...base, kind: "place", title: placeTitle(places.get(s.timeline_place_id ?? "")), dur: fmtDur(ce - cs) };
			case "transit":
				return { ...base, kind: "transit", title: transitTitle(Number(s.metadata.peak_kmh ?? 0), ce - cs), dur: fmtDur(ce - cs) };
			case "sleep": {
				// A night keeps its full length, though the day only shows its part.
				const src = s.metadata.source === "healthkit" ? "healthkit" : "quiet_hours";
				return { ...base, kind: "sleep", title: "In Bed", dur: fmtDur(s.e - s.s), src };
			}
			case "unknown": {
				const i = located.indexOf(s);
				const beside = [located[i - 1], located[i + 1]].find((n) => n?.kind === "stay");
				const likely = beside ? `likely still at ${placeTitle(places.get(beside.timeline_place_id ?? ""))}` : "GPS silent";
				const verdict = gapVerdict({
					fixes: Number(s.metadata.fix_count ?? 0),
					talk: Number(s.metadata.conversation_count ?? 0) > 0,
					steps: Number(s.metadata.step_bin_count ?? 0) > 0,
				});
				return { ...base, kind: "gap", title: "Signal gap", dur: fmtDur(ce - cs), notes: [likely, verdict] };
			}
		}
	});
	// In the order the day shows them: by where each starts inside the day,
	// a night before the stay it began in when both open at midnight (the
	// prototype lists the nights first, main.js:1566-1572).
	const sections = built.sort((a, b) => a.s - b.s || (a.kind === "sleep" ? 0 : 1) - (b.kind === "sleep" ? 0 : 1));

	const windows = voice.map((v) => ({ w: v, s: t(v.started_at), e: t(v.ended_at) }));
	const talk = windows.filter((v) => v.w.speaker_count >= 2);
	const rows: RailRow[] = w.moments
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
		let home: RailSection | undefined;
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
