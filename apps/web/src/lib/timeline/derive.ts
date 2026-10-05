/**
 * derive.ts - the day the inspector and the lanes read, from the day the
 * server sends.
 *
 * The server sends only what the record holds: visits, fixes, nights, audio
 * sessions, steps, the calendar. Everything named here is read off those and
 * nothing is stored: a stay is a visit; what lies between two stays is a drive
 * when the track shows travel and a signal gap when it does not; a night is
 * a joined HealthKit night; a conversation is an audio session with two or
 * more voices in it.
 */
import type { TimelineDay } from "./day";
import type { DerivedMoment, DerivedPlace, DerivedSpan, DerivedWindow, VoiceWindow } from "./inspector";
import type { LaneWindow } from "./lanes";
import { dropSpikes, metres, toFixes, type Fix } from "./track";

const MIN = 60_000;
/** A seam between two stays shorter than this is the edge of a visit, not a
 *  stretch of the day. */
const SEAM_MS = 3 * MIN;
/** A stretch whose fastest hop is slower than a walk never showed travel:
 *  the phone went quiet in one place and came back in another. */
const NO_TRAVEL_KMH = 4;
/** Hops closer in time than this are GPS jitter, not speed. */
const MIN_HOP_MS = 10_000;
/** A long stretch is travel only when the track covers it: at least one fix
 *  per this long, and at least LONG_MIN_PATH_M of path. */
const LONG_MS = 30 * MIN;
const LONG_FIX_EVERY_MS = 15 * MIN;
const LONG_MIN_PATH_M = 600;
/** A conversation's title is the start of what was said, cut at a word. */
const TITLE_CHARS = 60;

type Span = { s: number; e: number };

/** What the track did over [s, e): its fixes, its length, its fastest hop. */
export function trackOver(fixes: Fix[], s: number, e: number): { fixes: number; pathM: number; peakKmh: number } {
	const inside = fixes.filter((f) => f.t >= s && f.t < e);
	// The hop into the stretch counts: leaving a place starts at its last fix.
	const before = fixes.filter((f) => f.t < s).at(-1);
	const after = fixes.find((f) => f.t >= e);
	const run = [before, ...inside, after].filter((f): f is Fix => f !== undefined);
	let pathM = 0;
	let peakKmh = 0;
	for (let i = 1; i < run.length; i++) {
		const m = metres(run[i - 1], run[i]);
		const dt = run[i].t - run[i - 1].t;
		pathM += m;
		if (dt >= MIN_HOP_MS) peakKmh = Math.max(peakKmh, m / 1000 / (dt / 3_600_000));
	}
	return { fixes: inside.length, pathM, peakKmh };
}

/** Whether a stretch between stays was travel the track saw. */
export function isTravel(s: number, e: number, t: { fixes: number; pathM: number; peakKmh: number }): boolean {
	if (t.peakKmh < NO_TRAVEL_KMH) return false;
	const long = e - s > LONG_MS;
	return !(long && (t.fixes < (e - s) / LONG_FIX_EVERY_MS || t.pathM < LONG_MIN_PATH_M));
}

/** [lo, hi) less every span in `cover`, as the pieces left over. */
export function uncovered(lo: number, hi: number, cover: Span[]): Span[] {
	const sorted = [...cover].sort((a, b) => a.s - b.s);
	const out: Span[] = [];
	let cursor = lo;
	for (const c of sorted) {
		if (c.e <= cursor) continue;
		if (c.s >= hi) break;
		if (c.s > cursor) out.push({ s: cursor, e: Math.min(c.s, hi) });
		cursor = Math.max(cursor, c.e);
	}
	if (cursor < hi) out.push({ s: cursor, e: hi });
	return out;
}

/** The first sentence of what was said, cut at a word. */
export function conversationTitle(content: string | null): string | null {
	const first = content?.trim().match(/^[^.!?\n]+/)?.[0]?.trim();
	if (!first) return null;
	if (first.length <= TITLE_CHARS) return first;
	const cut = first.slice(0, TITLE_CHARS);
	return `${cut.slice(0, cut.lastIndexOf(" ") > 20 ? cut.lastIndexOf(" ") : TITLE_CHARS).trim()}…`;
}

export interface Sources {
	/** A calendar has ever synced; null when not known yet. */
	calendar: boolean | null;
	/** A financial account has ever synced; null when not known yet. */
	finance: boolean | null;
}

export interface DerivedDay {
	derived: DerivedWindow;
	voice: VoiceWindow[];
	lanes: LaneWindow;
}

export function deriveDay(day: TimelineDay, sources: Sources): DerivedDay {
	const t = (iso: string) => Date.parse(iso);
	const start = t(day.started_at);
	const end = t(day.ended_at);
	const fixes = dropSpikes(toFixes([...(day.last_point_before ? [day.last_point_before] : []), ...day.points]));

	const places = new Map<string, DerivedPlace>();
	const stays: DerivedSpan[] = day.stays.map((v) => {
		const placeId = v.place_id ?? `visit:${v.id}`;
		if (!places.has(placeId))
			places.set(placeId, {
				id: placeId,
				latitude: v.latitude,
				longitude: v.longitude,
				is_home: false,
				is_work: false,
				place_name: v.place_name,
			});
		return { id: v.id, kind: "stay", started_at: v.started_at, ended_at: v.ended_at, timeline_place_id: placeId, metadata: {} };
	});

	const nights: DerivedSpan[] = day.nights.map((n, i) => ({
		id: `night:${i}:${n.started_at}`,
		kind: "sleep",
		started_at: n.started_at,
		ended_at: n.ended_at,
		timeline_place_id: null,
		metadata: { source: "healthkit", asleep_minutes: n.asleep_minutes },
	}));

	const talk = day.sessions.filter((x) => x.speaker_mode >= 2).map((x) => ({ s: t(x.started_at), e: t(x.ended_at) }));
	const stepTimes = day.steps.map((b) => t(b.at));

	// What lies between the stays and the nights: a drive, or a signal gap.
	const covered = [...stays, ...nights].map((x) => ({ s: t(x.started_at), e: t(x.ended_at) }));
	const between: DerivedSpan[] = uncovered(start, end, covered)
		.filter((g) => g.e - g.s >= SEAM_MS)
		.map((g, i) => {
			const track = trackOver(fixes, g.s, g.e);
			const iso = { started_at: new Date(g.s).toISOString(), ended_at: new Date(g.e).toISOString() };
			if (isTravel(g.s, g.e, track))
				return {
					id: `transit:${i}`,
					kind: "transit",
					...iso,
					timeline_place_id: null,
					metadata: { path_meters: Math.round(track.pathM), peak_kmh: Math.round(track.peakKmh) },
				};
			return {
				id: `gap:${i}`,
				kind: "unknown",
				...iso,
				timeline_place_id: null,
				metadata: {
					fix_count: track.fixes,
					conversation_count: talk.filter((c) => c.e > g.s && c.s < g.e).length,
					step_bin_count: stepTimes.filter((x) => g.s <= x && x < g.e).length,
				},
			};
		});

	const moments: DerivedMoment[] = day.sessions
		.filter((x) => x.speaker_mode >= 2)
		.map((x) => ({
			id: x.id,
			kind: "conversation",
			started_at: x.started_at,
			ended_at: x.ended_at,
			title: conversationTitle(x.content),
			metadata: { window_ids: [x.id], speaker_count: x.speaker_mode },
		}));

	const voice: VoiceWindow[] = day.sessions.map((x) => ({
		id: x.id,
		started_at: x.started_at,
		ended_at: x.ended_at,
		speaker_count: x.speaker_mode,
		title: conversationTitle(x.content),
		text: x.content,
		people: [],
	}));

	const lanes: LaneWindow = {
		steps: day.steps.map((b) => ({ occurred_at: b.at, step_count: b.steps })),
		step_scale: day.step_scale,
		has_calendar: sources.calendar ?? true,
		// An all-day event has no place on a timeline of hours, and neither
		// does an invitation declined or an event cancelled.
		calendar: day.calendar
			.filter((ev) => !ev.is_all_day && ev.response_status !== "declined" && ev.status !== "cancelled")
			.map((ev) => ({
				id: ev.id,
				title: ev.title,
				started_at: ev.started_at,
				ended_at: ev.ended_at,
				calendar_name: ev.calendar_name,
				location_name: ev.location_name,
				is_unanswered: ev.response_status === "needsAction",
			})),
		has_finance: sources.finance ?? true,
	};

	return {
		derived: {
			spans: [...stays, ...nights, ...between].sort((a, b) => t(a.started_at) - t(b.started_at)),
			moments,
			last_stay_before: null,
			places: [...places.values()],
		},
		voice,
		lanes,
	};
}

/** Several days as one window: the Week span's lanes. */
export function joinDays(days: DerivedDay[]): DerivedDay {
	const placeIds = new Set<string>();
	// A stay or night that crosses midnight is served with both days.
	const once = <T>(key: (x: T) => string) => {
		const seen = new Set<string>();
		return (x: T) => !seen.has(key(x)) && !!seen.add(key(x));
	};
	return {
		derived: {
			spans: days.flatMap((d) => d.derived.spans).filter(once((s: DerivedSpan) => `${s.kind}:${s.started_at}`)),
			moments: days.flatMap((d) => d.derived.moments).filter(once((m: DerivedMoment) => m.id)),
			last_stay_before: null,
			places: days
				.flatMap((d) => d.derived.places)
				.filter((p) => !placeIds.has(p.id) && placeIds.add(p.id)),
		},
		voice: days.flatMap((d) => d.voice).filter(once((v: VoiceWindow) => v.id)),
		lanes: {
			steps: days.flatMap((d) => d.lanes.steps),
			step_scale: Math.max(1, ...days.map((d) => d.lanes.step_scale)),
			has_calendar: days.some((d) => d.lanes.has_calendar),
			calendar: days.flatMap((d) => d.lanes.calendar).filter((ev, i, all) => all.findIndex((x) => x.id === ev.id) === i),
			has_finance: days.some((d) => d.lanes.has_finance),
		},
	};
}
