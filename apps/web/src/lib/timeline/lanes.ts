/**
 * lanes.ts - what the scrubber's lanes draw (dayback/src/main.js:1319-1371),
 * as data the component lays out: the Location ribbon with its held spans,
 * a conversation's waveform silhouette, and the Body lane's step bars.
 */

import { buildInspector, type DerivedWindow, type VoiceWindow } from "./inspector";
import { MIN } from "./scale";

/** The Body, Calendar and Finance lanes over a window, as
 *  `/api/timeline/lanes` serves them. */
export interface LaneWindow {
	steps: { occurred_at: string; step_count: number }[];
	step_scale: number;
	has_calendar: boolean;
	calendar: {
		id: string;
		title: string | null;
		started_at: string;
		ended_at: string;
		calendar_name: string | null;
		location_name: string | null;
		is_unanswered: boolean;
	}[];
	has_finance: boolean;
}

/** A conversation in the Voice lane, with what its hover peek shows. */
export interface ScrubConversation {
	s: number;
	e: number;
	title: string;
	/** The most voices any of its windows heard. */
	speakers: number;
	/** Names mentioned in it, never "present". */
	people: string[];
}

/** A timed calendar event in the Calendar lane. */
export interface ScrubEvent {
	/** Its real span, for the peek. */
	s: number;
	e: number;
	/** The spans the lane draws: where events overlap, each gets its share
	 *  of the shared time, so the lane never stacks them; an event with a
	 *  shorter one inside it resumes after it. */
	pieces: [number, number][];
	title: string;
	where: string | null;
	/** An invitation you haven't answered: drawn faded. */
	unanswered: boolean;
}

/** Events in start order, each with the pieces of time the lane draws it
 *  over. Time only one event covers is its own; time several cover is split
 *  into equal shares in start order, so two overlapping events meet halfway
 *  through what they share, and an event with a shorter one inside it gives
 *  way to it and resumes after. A tiny lane has no room for two rows. */
export function untangle<T extends { s: number; e: number }>(events: T[]): (T & { pieces: [number, number][] })[] {
	const out = [...events].sort((a, b) => a.s - b.s || a.e - b.e).map((ev) => ({ ...ev, pieces: [] as [number, number][] }));
	const cuts = [...new Set(out.flatMap((ev) => [ev.s, ev.e]))].sort((a, b) => a - b);
	for (let k = 1; k < cuts.length; k++) {
		const a = cuts[k - 1];
		const b = cuts[k];
		const over = out.filter((ev) => ev.s < b && ev.e > a);
		over.forEach((ev, j) => {
			const ps = a + ((b - a) * j) / over.length;
			const pe = a + ((b - a) * (j + 1)) / over.length;
			const last = ev.pieces.at(-1);
			if (last && last[1] === ps) last[1] = pe;
			else ev.pieces.push([ps, pe]);
		});
	}
	return out;
}

/** `text` cut to whole words within `maxc` characters, with an ellipsis when
 *  cut; empty when not even the first word fits - never a mid-word stub
 *  (main.js:1328-1330). */
export function fitWords(text: string, maxc: number): string {
	if (text.length <= maxc) return text;
	let acc = "";
	for (const word of text.split(/\s+/)) {
		const next = acc ? `${acc} ${word}` : word;
		if (next.length <= maxc - 1) acc = next;
		else break;
	}
	return acc ? `${acc}…` : "";
}

export type RibbonKind = "place" | "transit" | "gap";

export interface RibbonSpan {
	s: number;
	e: number;
	kind: RibbonKind;
	title: string;
	/** A gap between two stretches, faded in the colour of the one before. */
	held: boolean;
}

/** Location as one ribbon (main.js:1319-1322): every stretch, and across a
 *  gap of over 3 minutes to the next, a faded span that holds the last one. */
export function ribbon(stretches: { s: number; e: number; kind: RibbonKind; title: string }[]): RibbonSpan[] {
	const sorted = [...stretches].sort((a, b) => a.s - b.s);
	const out: RibbonSpan[] = [];
	sorted.forEach((x, i) => {
		out.push({ ...x, held: false });
		const next = sorted[i + 1];
		if (next && next.s - x.e > 3 * MIN) out.push({ s: x.e, e: next.s, kind: x.kind, title: x.title, held: true });
	});
	return out;
}

/** A conversation's waveform (main.js:1342-1347): a filled, rounded
 *  silhouette between x `a` and `b` around `cy`, tapered at the ends. Its
 *  height claims nothing about volume; a phase from the start time keeps
 *  each one a little different. */
export function waveform(a: number, b: number, cy: number, maxA: number, start: number): string {
	const w = b - a;
	const ph = ((start % 97000) / 97000) * 6.283;
	const top: string[] = [];
	const bot: string[] = [];
	for (let x = a; x <= b + 0.01; x += 3) {
		const u = x - a;
		const edge = Math.min(1, Math.min(u, w - u) / 8);
		const s1 = 0.5 + 0.5 * Math.sin(u * 0.16 + ph);
		const s2 = 0.5 + 0.5 * Math.sin(u * 0.37 + ph * 1.6);
		const amp = maxA * (0.14 + 0.86 * (0.62 * s1 + 0.38 * s2)) * (0.4 + 0.6 * edge);
		top.push(`${x.toFixed(1)} ${(cy - amp).toFixed(1)}`);
		bot.push(`${x.toFixed(1)} ${(cy + amp).toFixed(1)}`);
	}
	if (top.length < 2) return "";
	return `M${top.join(" L")} L${bot.reverse().join(" L")} Z`;
}

export type Zone = "still" | "walking" | "active";

export interface Bar {
	t: number;
	/** The reading against the record's scale, 0-1. */
	f: number;
	zone: Zone;
}

/** The Body lane's bars (main.js:1354-1370): every step reading while not
 *  in bed, measured against the record's 92nd-percentile bin; the zone by
 *  the same measure the readout would use. */
export function bars(bins: { t: number; v: number }[], nights: { s: number; e: number }[], scale: number): Bar[] {
	return bins
		.filter((b) => !nights.some((n) => n.s <= b.t && b.t < n.e))
		.map((b) => {
			const f = Math.min(1, b.v / scale);
			return { t: b.t, f, zone: f < 0.33 ? "still" : f < 0.66 ? "walking" : "active" };
		});
}

/** A bar's height: the square root lifts low but real movement off the
 *  floor while real exertion still reaches the top; every reading gets at
 *  least a nub, "the sensor was recording". */
export const barHeight = (f: number, amp: number) => Math.max(Math.max(2.5, amp * 0.05), Math.sqrt(f) * amp);

/** Bar width from the median spacing of the readings on screen, with air
 *  between them. */
export function barWidth(xs: number[]): number {
	const gaps = xs
		.slice(1)
		.map((x, i) => x - xs[i])
		.filter((g) => g > 0)
		.sort((a, b) => a - b);
	const median = gaps.length ? gaps[Math.floor(gaps.length / 2)] : 6;
	return Math.max(2, Math.min(5, median * 0.62));
}

/** Everything the scrubber's lanes draw, over the window [a, b) it was
 *  loaded for. `null` for a source means not known (its fetch failed), so the
 *  lane draws nothing rather than claiming it isn't connected. */
export interface Lanes {
	a: number;
	b: number;
	ribbon: RibbonSpan[];
	nights: { s: number; e: number }[];
	/** Every window the mic recorded. */
	voice: { s: number; e: number }[];
	conversations: ScrubConversation[];
	steps: { t: number; v: number }[];
	stepScale: number;
	hasCalendar: boolean | null;
	calendar: ScrubEvent[];
	hasFinance: boolean | null;
}

export const NO_LANES: Lanes = {
	a: 0,
	b: 0,
	ribbon: [],
	nights: [],
	voice: [],
	conversations: [],
	steps: [],
	stepScale: 1,
	hasCalendar: null,
	calendar: [],
	hasFinance: null,
};

/** The lanes over [a, b) from what the server sent: the stretches and
 *  nights as the inspector reads them, the conversations with who the peek
 *  names, the step bins and the calendar. */
export function laneData(derived: DerivedWindow | null, voice: VoiceWindow[], lw: LaneWindow | null, a: number, b: number): Lanes {
	const t = (iso: string) => Date.parse(iso);
	// Whole, never clipped to the window: a night that began before it folds
	// from its real start, as the prototype's nights do (main.js:100-111).
	const sections = derived ? buildInspector(derived, Number.NEGATIVE_INFINITY, Number.POSITIVE_INFINITY, voice) : [];
	const windows = voice.map((v) => ({ v, s: t(v.started_at), e: t(v.ended_at) }));
	const talk = windows.filter((w) => w.v.speaker_count >= 2);
	const conversations = (derived?.moments ?? [])
		.filter((m) => m.kind === "conversation")
		.map((m) => ({ m, s: t(m.started_at), e: t(m.ended_at) }))
		.filter((c) => c.e > a && c.s < b)
		.sort((x, y) => x.s - y.s)
		.map(({ m, s, e }) => {
			const over = talk.filter((w) => w.s < e && w.e > s);
			return {
				s,
				e,
				title: m.title ?? "Conversation",
				speakers: over.length ? Math.max(0, ...over.map((w) => w.v.speaker_count)) : 0,
				people: [...new Set(over.flatMap((w) => w.v.people))],
			};
		});
	return {
		a,
		b,
		ribbon: ribbon(
			sections.flatMap((x) => (x.kind === "sleep" ? [] : [{ s: x.s, e: x.e, kind: x.kind, title: x.title }])),
		),
		nights: sections.filter((x) => x.kind === "sleep").map((x) => ({ s: x.s, e: x.e })),
		voice: windows.map((w) => ({ s: w.s, e: w.e })),
		conversations,
		steps: (lw?.steps ?? []).map((x) => ({ t: t(x.occurred_at), v: x.step_count })),
		stepScale: lw?.step_scale ?? 1,
		hasCalendar: lw ? lw.has_calendar : null,
		calendar: untangle(
			(lw?.calendar ?? []).map((ev) => ({
				s: t(ev.started_at),
				e: t(ev.ended_at),
				// Google Calendar's own words for an event with no title.
				title: ev.title?.trim() || "(No title)",
				where: ev.location_name,
				unanswered: ev.is_unanswered,
			})),
		),
		hasFinance: lw ? lw.has_finance : null,
	};
}
