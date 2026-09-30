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

export interface DerivedWindow {
	is_built: boolean;
	spans: DerivedSpan[];
	moments: DerivedMoment[];
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

/** A place's title: Home, or the wiki's name for it. The wiki names a place
 *  it cannot name "Location <lat>, <lon>"; that is no title to show. */
export function placeTitle(p: DerivedPlace | undefined): string {
	if (!p) return "Unnamed place";
	if (p.is_home) return "Home";
	if (p.place_name && !p.place_name.startsWith("Location ")) return p.place_name;
	return "Unnamed place";
}

/** A drive's title by its fastest hop: Flying from 350 km/h, Driving from 45,
 *  else In transit; hours on the ground with no stay between say so, rather
 *  than calling it one long drive (main.js:1571, 2547). */
export function transitTitle(peakKmh: number, ms: number): string {
	const mode = peakKmh >= 350 ? "Flying" : peakKmh >= 45 ? "Driving" : "In transit";
	return mode !== "Flying" && ms > 2 * HOUR ? "Out · no stay recorded" : mode;
}

/** The day's sections and rows, clipped to [start, end). */
export function buildRail(w: DerivedWindow, start: number, end: number): RailSection[] {
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
		const base = { s: cs, e: ce, placeId: s.timeline_place_id, rows: [] as RailRow[], notes: [] as string[] };
		switch (s.kind) {
			case "stay":
				return { ...base, kind: "place", title: placeTitle(places.get(s.timeline_place_id ?? "")), dur: fmtDur(ce - cs) };
			case "transit":
				return { ...base, kind: "transit", title: transitTitle(Number(s.metadata.peak_kmh ?? 0), ce - cs), dur: fmtDur(ce - cs) };
			case "sleep":
				// A night keeps its full length, though the day only shows its part.
				return { ...base, kind: "sleep", title: "In Bed", dur: fmtDur(s.e - s.s) };
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
		}));
	// Filed under the section that had begun most recently at the row's middle:
	// a conversation can start seconds before the stay's arrival stamp.
	for (const r of rows) {
		const mid = (r.s + r.e) / 2;
		let home: RailSection | undefined;
		for (const sec of sections) if (sec.s <= mid) home = sec;
		(home ?? sections[0])?.rows.push(r);
	}
	return sections;
}

function walkTitle(meta: Record<string, unknown>): string {
	const km = Number(meta.path_meters ?? 0) / 1000;
	return `${Number(meta.kmh ?? 0) < 8 ? "Walk" : "Moved"} · ${km.toFixed(1)} km`;
}
