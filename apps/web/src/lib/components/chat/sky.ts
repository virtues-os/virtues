/**
 * What the chat sky (ChatSky) and the composer's `@` menu (ContextPanel)
 * share about today's moments: the day itself, how a moment's time reads,
 * which moments a typed query matches, and where each level of the sky puts
 * its dots ("stars").
 */
import {
	getSkyDay,
	type SkyContext,
	type SkyDay,
	type SkyLink,
	type SkyMoment,
	type SkyMomentDetail,
	type SkyThread,
	type SkyWeek
} from '$lib/api/client';
import { getLocalDateSlug } from '$lib/utils/dateUtils';
import { PROJECT_ICON } from '$lib/utils/iconHelpers';

/** The icon for each kind of moment, and of link a moment opens to. */
export const SKY_ICONS: Record<string, string> = {
	stay: 'mynaui:map-pin',
	event: 'mynaui:calendar',
	workout: 'mynaui:activity',
	sleep: 'mynaui:moon',
	place: 'mynaui:map-pin',
	person: 'mynaui:user',
	purchase: 'mynaui:credit-card',
	message: 'mynaui:message',
	voice: 'mynaui:microphone',
	arrive: 'mynaui:map-pin',
	leave: 'mynaui:map-pin',
	project: PROJECT_ICON,
	page: 'mynaui:file-text',
	chat: 'mynaui:message'
};

/**
 * The sky's levels, finest first. Scrolling over the sky moves along them:
 * one moment opened into what happened in it, the day, the day with what it
 * belongs to, and the week.
 */
export const LEVELS = ['moment', 'day', 'context', 'week'] as const;
export type Level = (typeof LEVELS)[number];

export const LEVEL_NAMES: Record<Level, string> = { moment: 'Moment', day: 'Day', context: 'Context', week: 'Week' };

export const SKY_HEIGHT = 140;
export const SKY_PAD = 12;

/** How long one read of today serves the sky and the `@` menu. */
const FRESH_MS = 60_000;
let cached: { date: string; at: number; day: Promise<SkyDay> } | null = null;

/** Today's sky. A new chat and every `@` ask for it, so within a minute
 *  they share one request. A failed request is not kept. */
export function todaySky(): Promise<SkyDay> {
	const date = getLocalDateSlug();
	if (cached && cached.date === date && Date.now() - cached.at < FRESH_MS) return cached.day;
	const day = getSkyDay(date);
	cached = { date, at: Date.now(), day };
	day.catch(() => {
		if (cached?.day === day) cached = null;
	});
	return day;
}

/** Forget the read of today, so the next `todaySky` asks the server, as
 *  the sky does when it reads the day again. */
export function forgetToday() {
	cached = null;
}

/** Hours since local midnight as a wall clock: `12:40 PM`, or `12:40`. */
export function clock(hours: number, suffix = true): string {
	const h = ((Math.floor(hours) % 24) + 24) % 24;
	const m = Math.round((hours - Math.floor(hours)) * 60);
	const hh = h + (m === 60 ? 1 : 0);
	const mm = m === 60 ? 0 : m;
	const text = `${((hh % 24) + 11) % 12 + 1}:${String(mm).padStart(2, '0')}`;
	return suffix ? `${text} ${hh % 24 < 12 ? 'AM' : 'PM'}` : text;
}

/** `12:00–1:00 PM`, naming AM or PM once when both ends share it. */
export function span(start: number, end: number): string {
	const sameHalf = (((Math.floor(start) % 24) + 24) % 24 < 12) === (((Math.floor(end) % 24) + 24) % 24 < 12);
	return `${clock(start, !sameHalf)}–${clock(end)}`;
}

/** Whether a moment answers what was typed after an `@`: its label, or a
 *  link's (a lunch answers "david" through David Okafor). */
export function momentMatches(m: { label: string; links: SkyLink[] }, query: string): boolean {
	const q = query.trim().toLowerCase();
	return q !== '' && (m.label.toLowerCase().includes(q) || m.links.some((l) => l.label.toLowerCase().includes(q)));
}

/** The second line of a moment's hover: when it was, or is. */
export function momentWhen(m: SkyMoment, now: number | null): string {
	if (m.kind === 'sleep') return `Asleep ${clock(m.start_hour)}, up at ${clock(m.end_hour)}`;
	if (m.planned) return `${span(m.start_hour, m.end_hour)} · on your calendar`;
	if (isOngoing(m, now)) return `Since ${clock(m.start_hour)}`;
	return span(m.start_hour, m.end_hour);
}

/** How long after a stay's last recorded minute it still counts as now:
 *  the entity resolver rewrites the live visit every 15 minutes. */
const ONGOING_SLACK_H = 20 / 60;

function isOngoing(m: SkyMoment, now: number | null): boolean {
	return now !== null && m.kind !== 'sleep' && !m.planned && m.start_hour <= now && now <= m.end_hour + ONGOING_SLACK_H;
}

/** A stable number in [0, 1) for a string, for scatter that does not move
 *  between reads. */
export function hash(s: string): number {
	let h = 2166136261;
	for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 16777619);
	h = Math.imul(h ^ (h >>> 16), 0x85ebca6b);
	h = Math.imul(h ^ (h >>> 13), 0xc2b2ae35);
	h ^= h >>> 16;
	return (h >>> 0) / 4294967296;
}

/**
 * How a dot is drawn. `past` is solid; `ongoing` is solid with a slow pulse;
 * `planned` is a faint ring, so what has not happened is quieter than what
 * has; `night` is the soft band at the left edge; `cap` is a moment opened
 * into a soft capsule at the Moment level.
 */
export type StarLook = 'past' | 'ongoing' | 'planned' | 'night' | 'cap';

/** One dot on any level of the sky, placed and ready to draw. */
export interface Star {
	/** Unique on its level. */
	key: string;
	label: string;
	/** The hover's second line. */
	when: string;
	/** An icon key (`SKY_ICONS`). */
	kind: string;
	/** What the dot mentions, or null when nothing can be. */
	url: string | null;
	links: SkyLink[];
	x: number;
	y: number;
	w: number;
	h: number;
	/** Ink, in percent of its color. */
	tone: number;
	look: StarLook;
	/** Drawn in the theme's accent instead of the text color: what is
	 *  happening now, and what is unusual. */
	accent: boolean;
	/** Its place in the arrival, and its own breathing period in seconds. */
	order: number;
	breathe: number;
	/** The threads it belongs to, by url, at the Context and Week levels. */
	threads?: string[];
	/** The day's moment it is, when it is one. */
	moment?: SkyMoment;
	/** Placed but not drawn: a day's dot folded into its week dot, or pushed
	 *  off the edge when a moment opens. It stays so the next level can move
	 *  it back from there instead of fading it in from nothing. */
	hidden?: boolean;
	/** Where it comes out of when it first appears: a moment's pieces come
	 *  out of its capsule. */
	from?: { x: number; y: number };
	/** How long it waits to appear, in ms: a week's dots wait for the day's
	 *  to fold into them. */
	delay?: number;
}

/** How tall the sky is at each level, as the final-round boards draw it. */
export const LEVEL_HEIGHT: Record<Level, number> = { moment: 210, day: 140, context: 270, week: 300 };

/** A thread's label: above its moments at Context, in the left gutter at Week. */
export interface ThreadLabel {
	thread: SkyThread;
	x: number;
	y: number;
	align: 'center' | 'right';
	/** The hover's second line. */
	why: string;
}

/** A drawn line: a connector at Context, a storyline at Week. */
export interface SkyPath {
	key: string;
	/** The thread it belongs to. */
	url: string;
	d: string;
}

/** Everything a level draws, placed. */
export interface Layout {
	height: number;
	stars: Star[];
	/** The dotted now mark, when the level shows now. */
	now: { x: number; top: number; height: number } | null;
	labels: ThreadLabel[];
	paths: SkyPath[];
	/** The week's day names along the bottom. */
	letters: { x: number; date: string; text: string; today: boolean }[];
	/** The Moment level's pieces, written under the line. */
	pieceLabels: { key: string; x: number; y: number; text: string }[];
	/** The Moment level's title, centered over the moment. */
	head: { x: number; y: number; title: string; why: string } | null;
}

const EMPTY: Omit<Layout, 'height' | 'stars'> = { now: null, labels: [], paths: [], letters: [], pieceLabels: [], head: null };

export const THREAD_KIND: Record<SkyThread['kind'], string> = { project: 'Project', page: 'Page', chat: 'Chat' };

const r1 = (v: number) => Math.round(v * 10) / 10;

/** Ink by kind: what you did reads darker than where you were. */
const TONE: Record<string, number> = { stay: 38, event: 50, workout: 56, sleep: 30 };
/** Ink for a dot in the accent. */
const ACCENT_TONE = 82;
/** At or above this novelty a moment is unusual enough to take the accent. */
const UNUSUAL = 0.7;

/** A dot's diameter for a duration, by area, so three hours is clearly
 *  bigger than fifteen minutes without the long ones swamping the rest. */
export function dotSize(minutes: number, scale = 1): number {
	const d = 3 + 8.5 * Math.sqrt(Math.min(Math.max(minutes, 0), 240) / 240);
	return Math.round(d * scale * 10) / 10;
}

/** How wide the night band is per hour asleep, and its bounds, so a long
 *  night reads longer than a short one. */
const NIGHT_PX_PER_HOUR = 9;
const NIGHT_MIN_W = 30;
const NIGHT_MAX_W = 96;
/** The horizon the day rises from: the night lies on it, and routine
 *  moments sit just above it. */
const HORIZON_Y = SKY_HEIGHT - 18;
const ROUTINE_Y = HORIZON_Y - 16;
const UNUSUAL_Y = 22;

function nightWidth(m: SkyMoment): number {
	return Math.min(NIGHT_MAX_W, Math.max(NIGHT_MIN_W, (m.end_hour - m.start_hour) * NIGHT_PX_PER_HOUR));
}

/** Height by novelty: routine just above the horizon, the unusual near the
 *  top. A moment with nothing to compare sits halfway. */
function noveltyY(m: SkyMoment): number {
	const n = m.novelty ?? 0.5;
	const y = ROUTINE_Y - n * (ROUTINE_Y - UNUSUAL_Y) + (hash(m.id + 'j') - 0.5) * 8;
	return Math.round(Math.min(ROUTINE_Y + 4, Math.max(UNUSUAL_Y - 4, y)) * 10) / 10;
}
/** How much an hour still to come counts against an hour that has passed,
 *  in width: the morning gets the room, the rest of the day folds after now. */
const FUTURE_WEIGHT = 0.25;

/** Where a day's hours go across `left..right`. For today the hours so far
 *  take most of it and the rest of the day folds into a strip after now. */
export function dayFrame(day: SkyDay, left: number, right: number): { x: (h: number) => number; nowX: number | null } {
	const ms = day.moments;
	const night = ms.find((m) => m.kind === 'sleep');
	const awake = ms.filter((m) => m.kind !== 'sleep');
	const now = day.now_hour;
	const firsts = [...awake.map((m) => m.start_hour), ...(now !== null ? [now] : [])];
	const start = night ? night.end_hour : firsts.length > 0 ? Math.max(0, Math.min(...firsts) - 0.3) : 6;
	const lastEnd = Math.max(start + 1, ...awake.map((m) => m.end_hour + 0.3));
	const clampH = (h: number, a: number, b: number) => Math.min(Math.max(h, a), b);
	if (now === null || now <= start) {
		const end = now === null ? lastEnd : Math.max(lastEnd, 24);
		return { x: (h) => left + ((clampH(h, start, end) - start) / (end - start)) * (right - left), nowX: null };
	}
	const end = Math.max(lastEnd, now + 0.5, 24);
	const past = now - start;
	const future = end - now;
	const share = Math.min(0.82, Math.max(0.4, past / (past + future * FUTURE_WEIGHT)));
	const split = left + (right - left) * share;
	const x = (h: number) => {
		const t = clampH(h, start, end);
		return t <= now ? left + ((t - start) / past) * (split - left) : split + ((t - now) / future) * (right - split);
	};
	return { x, nowX: Math.round(split * 10) / 10 };
}

/** Heights for a set of moments: a place visited more than once keeps one
 *  height, so going back to it lines up; the rest step through the height by
 *  the golden ratio from `seed`, which reads as scattered, never bunches into
 *  one band, and leaves earlier dots where they were as later ones arrive. */
export function heights(moments: SkyMoment[], seed: string, top: number, bottom: number): Map<string, number> {
	const visits = new Map<string, number>();
	for (const m of moments) if (m.place_id) visits.set(m.place_id, (visits.get(m.place_id) ?? 0) + 1);
	const fixed = [...visits].filter(([, n]) => n > 1).map(([p]) => p).sort((a, b) => hash(a) - hash(b));
	const band = bottom - top;
	const level = new Map(fixed.map((p, i) => [p, top + 6 + (i + 0.3 + hash(p + 'g') * 0.4) * ((band - 6) / fixed.length)]));
	const base = hash(seed);
	let floating = 0;
	const out = new Map<string, number>();
	for (const m of moments) {
		const y = (m.place_id ? level.get(m.place_id) : undefined) ?? top + ((base + floating++ * 0.618034) % 1) * band;
		out.set(m.id, Math.round(Math.min(bottom + 4, Math.max(top - 4, y + (hash(m.id + 'j') - 0.5) * 6)) * 10) / 10);
	}
	return out;
}

/** The Day level: one dot per moment across the day, its height how unusual
 *  it was, the night as a band on the horizon at the left edge, as long as
 *  the night was, ending where the morning starts. Before anything can be
 *  scored (a new box), the dots scatter instead of lying on one line. */
export function layoutDay(day: SkyDay, width: number): Layout {
	const night = day.moments.find((m) => m.kind === 'sleep');
	const nightW = night ? nightWidth(night) : 0;
	const left = SKY_PAD + nightW + (night ? 6 : 0);
	const frame = dayFrame(day, left, width - SKY_PAD);
	const awake = day.moments.filter((m) => m.kind !== 'sleep');
	// `!= null`: a server older than the score sends no `novelty` at all.
	const scored = awake.some((m) => m.novelty != null);
	const scatter = scored ? null : heights(awake, day.date, 22, 104);
	const now = day.now_hour;
	const stars: Star[] = day.moments.map((m, i) => {
		const base = {
			key: m.id,
			label: m.label,
			when: momentWhen(m, now),
			kind: m.kind,
			url: m.url,
			links: m.links,
			moment: m,
			order: i,
			breathe: Math.round((6 + hash(m.id + 'tw') * 7) * 10) / 10
		};
		if (m.kind === 'sleep') {
			const x = Math.round((SKY_PAD + nightW / 2) * 10) / 10;
			return { ...base, x, y: HORIZON_Y, w: nightW, h: 4, tone: TONE.sleep, look: 'night' as const, accent: false };
		}
		const size = dotSize((m.end_hour - m.start_hour) * 60);
		const look: StarLook = m.planned ? 'planned' : isOngoing(m, now) ? 'ongoing' : 'past';
		const accent = look === 'ongoing' || (look === 'past' && (m.novelty ?? 0) >= UNUSUAL);
		return {
			...base,
			x: Math.round(frame.x((m.start_hour + m.end_hour) / 2) * 10) / 10,
			y: scatter ? (scatter.get(m.id) ?? 64) : noveltyY(m),
			w: size,
			h: size,
			tone: accent ? ACCENT_TONE : (TONE[m.kind] ?? TONE.stay),
			look,
			accent
		};
	});
	return {
		...EMPTY,
		height: LEVEL_HEIGHT.day,
		stars,
		now: frame.nowX === null ? null : { x: frame.nowX, top: 41, height: 58 }
	};
}

/** A local date (`YYYY-MM-DD`) read as a calendar day, whatever the zone. */
function calendarDay(date: string): Date {
	return new Date(`${date}T12:00:00`);
}

/** The Monday of the week holding a local date, as the server's week does. */
export function weekMonday(date: string): string {
	const d = calendarDay(date);
	d.setDate(d.getDate() - ((d.getDay() + 6) % 7));
	const pad = (n: number) => String(n).padStart(2, '0');
	return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** `Wed 22`, as a week's columns name their days. */
export function shortDate(date: string): string {
	const d = calendarDay(date);
	return `${d.toLocaleDateString(undefined, { weekday: 'short' })} ${d.getDate()}`;
}

/** At the Week level a moment counts at least this long, so a short one
 *  still has a size. */
const WEEK_MIN_MINUTES = 20;

/** The Week level, as the board draws it: each thread a line across the
 *  seven days at its own height, named in the left gutter, bending down or up
 *  to meet the day's dot where that day's moments belong to it. A day's
 *  moments gather into one dot per thread, sized by their minutes; what
 *  belongs to nothing gathers on the bottom line. */
export function layoutWeek(week: SkyWeek, width: number, today: string, focus: string = today): Layout {
	const H = LEVEL_HEIGHT.week;
	const G = Math.min(136, Math.round(width * 0.22));
	const colW = (width - G) / 7;
	const xw = (d: number, h: number) => G + d * colW + 6 + ((Math.min(Math.max(h, 6), 24) - 6) / 18) * (colW - 12);
	// A server older than week threads sends none.
	const threads = week.threads ?? [];
	const bandY = (i: number | null) => (i === null ? 250 : 24 + i * 37);
	type Member = { m: SkyMoment; key: string; mid: number; ongoing: boolean };
	type Group = { day: number; date: string; primary: number | null; members: Member[] };
	const groups = new Map<string, Group>();
	week.days.forEach((day, d) => {
		for (const m of day.moments.filter((m) => m.kind !== 'sleep')) {
			const key = `${day.date}|${m.id}`;
			const primary = threads.findIndex((t) => t.moments.includes(key));
			const p = primary < 0 ? null : primary;
			const gk = `${day.date}|${p ?? 'other'}`;
			const g = groups.get(gk) ?? { day: d, date: day.date, primary: p, members: [] };
			g.members.push({ m, key, mid: Math.min((m.start_hour + m.end_hour) / 2, 23.9), ongoing: isOngoing(m, day.now_hour) });
			groups.set(gk, g);
		}
	});
	const weekday = (date: string) => calendarDay(date).toLocaleDateString(undefined, { weekday: 'long' });
	const placed = [...groups.entries()].map(([gk, g]) => {
		const x = g.members.reduce((a, e) => a + xw(g.day, e.mid), 0) / g.members.length;
		const minutes = g.members.reduce((a, e) => a + Math.max((e.m.end_hour - e.m.start_hour) * 60, WEEK_MIN_MINUTES), 0);
		const union = threads.filter((t) => g.members.some((e) => t.moments.includes(e.key))).map((t) => t.url);
		return { gk, g, x, minutes, union };
	});
	const stars: Star[] = placed.map(({ gk, g, x, minutes, union }, i) => {
		const thread = g.primary === null ? null : threads[g.primary];
		const allPlanned = g.members.every((e) => e.m.planned);
		const ongoing = g.members.some((e) => e.ongoing);
		const unusual = g.members.some((e) => !e.m.planned && (e.m.novelty ?? 0) >= UNUSUAL);
		let why = g.members.map((e) => e.m.label).join(' · ');
		if (why.length > 72) why = `${why.slice(0, 70)}…`;
		const size = Math.max(4, r1(dotSize(minutes) * 0.8));
		const only = g.members.length === 1 ? g.members[0].m : null;
		return {
			key: gk,
			label: `${thread?.label ?? 'Everything else'}, ${weekday(g.date)}`,
			when: why,
			kind: thread?.kind ?? only?.kind ?? 'stay',
			url: thread?.url ?? only?.url ?? null,
			links: [],
			x: r1(x),
			y: bandY(g.primary),
			w: size,
			h: size,
			tone: ongoing || unusual ? ACCENT_TONE : 46,
			look: allPlanned ? 'planned' : ongoing ? 'ongoing' : 'past',
			accent: ongoing || unusual,
			order: i,
			breathe: r1(6 + hash(gk + 'tw') * 7),
			threads: union,
			delay: 260
		};
	});
	// The focused day's own dots, folded into the week dot they gathered
	// into, so zooming out draws them together and zooming in spreads them.
	for (const { gk, g, x } of placed.filter((p) => p.g.date === focus)) {
		for (const e of g.members) {
			stars.push({
				key: e.m.id,
				label: e.m.label,
				when: '',
				kind: e.m.kind,
				url: e.m.url,
				links: [],
				x: r1(x),
				y: bandY(g.primary),
				w: 3,
				h: 3,
				tone: TONE[e.m.kind] ?? TONE.stay,
				look: 'past',
				accent: false,
				order: 0,
				breathe: r1(6 + hash(gk + e.m.id) * 7),
				hidden: true
			});
		}
	}
	const paths: SkyPath[] = [];
	threads.forEach((t, i) => {
		const B = bandY(i);
		const perDay = new Map<number, (typeof placed)[number]>();
		for (const p of placed.filter((p) => p.union.includes(t.url))) {
			const cur = perDay.get(p.g.day);
			if (!cur || (p.g.primary === i && cur.g.primary !== i)) perDay.set(p.g.day, p);
		}
		const pts = [...perDay.values()].sort((a, b) => a.x - b.x);
		if (pts.length === 0) return;
		const y = (p: (typeof pts)[number]) => bandY(p.g.primary);
		let d = `M ${r1(pts[0].x - 14)} ${B}`;
		for (const p of pts) {
			const px = r1(p.x);
			d += ` L ${r1(px - 12)} ${B} C ${r1(px - 5)} ${B} ${r1(px - 5)} ${y(p)} ${px} ${y(p)} C ${r1(px + 5)} ${y(p)} ${r1(px + 5)} ${B} ${r1(px + 12)} ${B}`;
		}
		d += ` L ${r1(pts[pts.length - 1].x + 14)} ${B}`;
		paths.push({ key: t.url, url: t.url, d });
	});
	const labels: ThreadLabel[] = threads.map((t, i) => ({
		thread: t,
		x: G - 12,
		y: bandY(i) - 12,
		align: 'right',
		why: `${THREAD_KIND[t.kind]} · ${t.moments.length} this week`
	}));
	const letters = week.days.map((day, d) => ({
		x: r1(G + d * colW + colW / 2),
		date: day.date,
		text: calendarDay(day.date).toLocaleDateString(undefined, { weekday: 'short' }),
		today: day.date === today
	}));
	const d = week.days.findIndex((day) => day.date === today);
	const nowHour = d >= 0 ? week.days[d].now_hour : null;
	return {
		...EMPTY,
		height: H,
		stars,
		labels,
		paths,
		letters,
		now: nowHour === null ? null : { x: r1(xw(d, Math.min(nowHour, 23.9))), top: 8, height: 262 }
	};
}

/** The line the Moment level draws along. */
const MOMENT_LINE_Y = 112;
/** How many characters of a piece's label are written under the line, the
 *  rows they go in, and how wide a character is taken to be. */
const PIECE_LABEL_CHARS = 34;
const PIECE_ROWS = [140, 157, 174];
const PIECE_CHAR_PX = 6.4;

/** Write each piece's label under the line in the first row where it does
 *  not run into the one before it, kept inside the sky; with no room in any
 *  row, in the row that ends soonest. */
function placePieceLabels(pieces: Star[], width: number): Layout['pieceLabels'] {
	const cut = (s: string) => (s.length > PIECE_LABEL_CHARS ? `${s.slice(0, PIECE_LABEL_CHARS - 1).trimEnd()}…` : s);
	const ends = PIECE_ROWS.map(() => -Infinity);
	return [...pieces]
		.sort((a, b) => a.x - b.x)
		.map((p) => {
			const text = cut(p.label);
			const w = text.length * PIECE_CHAR_PX + 8;
			const x = Math.min(Math.max(p.x, w / 2 + 4), width - w / 2 - 4);
			let row = ends.findIndex((end) => x - w / 2 >= end + 8);
			if (row < 0) row = ends.indexOf(Math.min(...ends));
			ends[row] = x + w / 2;
			return { key: p.key, x: r1(x), y: PIECE_ROWS[row], text };
		});
}

/** The Moment level, as the board draws it: the moment opened into a soft
 *  capsule on a line through the minutes around it, what happened inside it
 *  as small dots on the line with their labels written under it, the
 *  moments before and after where they fall, and its name above. */
export function layoutMoment(detail: SkyMomentDetail, day: SkyDay | null, width: number): Layout {
	const m = detail.moment;
	const hours = detail.pieces.map((p) => p.hour);
	const m0 = Math.min(m.start_hour, ...hours) - 0.35;
	const m1 = Math.max(m.end_hour, ...hours) + 0.35;
	const xm = (h: number) => 24 + ((h - m0) / (m1 - m0)) * (width - 48);
	const a = xm(m.start_hour);
	const b = Math.max(xm(m.end_hour), a + 12);
	const now = detail.now_hour;
	const cap: Star = {
		key: m.id,
		label: m.label,
		when: momentWhen(m, now),
		kind: m.kind,
		url: m.url,
		links: m.links,
		x: r1((a + b) / 2),
		y: MOMENT_LINE_Y,
		w: r1(b - a),
		h: 34,
		tone: 6,
		look: 'cap',
		accent: false,
		order: 0,
		breathe: 9,
		moment: m
	};
	// The rest of the day on the same line: the moments just around it in
	// view, the others pushed off the edges, where opening the moment sent
	// them.
	const neighbors: Star[] = (day?.moments ?? [])
		.filter((n) => n.id !== m.id)
		.map((n) => ({ n, mid: (n.start_hour + n.end_hour) / 2 }))
		.map(({ n, mid }, i) => {
			const size = dotSize((n.end_hour - n.start_hour) * 60);
			const look: StarLook = n.planned ? 'planned' : isOngoing(n, now) ? 'ongoing' : 'past';
			return {
				key: n.id,
				label: n.label,
				when: momentWhen(n, now),
				kind: n.kind,
				url: n.url,
				links: n.links,
				x: r1(xm(mid)),
				y: MOMENT_LINE_Y,
				w: size,
				h: size,
				tone: TONE[n.kind] ?? TONE.stay,
				look,
				accent: look === 'ongoing',
				order: i + 1,
				breathe: r1(6 + hash(n.id + 'tw') * 7),
				moment: n,
				hidden: n.kind === 'sleep' || mid <= m0 || mid >= m1
			};
		});
	const pieces: Star[] = detail.pieces.map((p, i) => ({
		from: { x: cap.x, y: cap.y },
		delay: 120 + i * 40,
		key: p.id,
		label: p.label,
		when: p.detail ? `${clock(p.hour)} · ${p.detail}` : clock(p.hour),
		kind: p.kind,
		url: p.url,
		links: [],
		x: r1(xm(p.hour)),
		y: MOMENT_LINE_Y,
		w: 6,
		h: 6,
		tone: 62,
		look: 'past' as const,
		accent: false,
		order: i + 1,
		breathe: r1(6 + hash(p.id + 'tw') * 7)
	}));
	return {
		...EMPTY,
		height: LEVEL_HEIGHT.moment,
		stars: [cap, ...neighbors, ...pieces],
		pieceLabels: placePieceLabels(pieces, width),
		head: { x: r1(Math.min(Math.max((a + b) / 2, 140), width - 140)), y: 40, title: m.label, why: momentWhen(m, now) }
	};
}

/** Label widths are estimated, not measured: close enough to keep them apart. */
const LABEL_CHAR_PX = 6.8;
const LABEL_EXTRA_PX = 36;
const LABEL_MAX_PX = 220;

/** The Context level, as the board draws it: the day's dots lowered into
 *  the bottom of a taller sky, and above them what they belong to, in two
 *  rows, each label as near the middle of its moments as the labels beside
 *  it allow, joined to each of them by a curve. */
export function layoutContext(day: SkyDay, context: SkyContext, width: number): Layout {
	const base = layoutDay(day, width);
	const ty = (y: number) => r1(156 + ((y - 16) / 108) * 96);
	const threadsOf = (key: string) => context.threads.filter((t) => t.moments.includes(key)).map((t) => t.url);
	const stars: Star[] = base.stars.map((s) => ({ ...s, y: ty(s.y), threads: threadsOf(s.key) }));
	const byKey = new Map(stars.map((s) => [s.key, s]));
	const items = context.threads
		.map((thread) => {
			const joined = thread.moments.map((id) => byKey.get(id)).filter((s): s is Star => !!s);
			const mx = joined.length > 0 ? joined.reduce((a, s) => a + s.x, 0) / joined.length : width / 2;
			const w = Math.min(LABEL_MAX_PX, thread.label.length * LABEL_CHAR_PX + LABEL_EXTRA_PX);
			return { thread, joined, mx, w, tx: 0, ty: 0 };
		})
		.filter((it) => it.joined.length > 0)
		.sort((a, b) => a.mx - b.mx);
	const rowEnd = [-1e9, -1e9];
	const rowY = [20, 66];
	for (const it of items) {
		const want = it.mx - it.w / 2;
		const r = want >= rowEnd[0] + 10 ? 0 : want >= rowEnd[1] + 10 ? 1 : rowEnd[0] <= rowEnd[1] ? 0 : 1;
		const left = Math.max(4, Math.min(width - it.w - 4, Math.max(rowEnd[r] + 10, want)));
		rowEnd[r] = left + it.w;
		it.tx = r1(left + it.w / 2);
		it.ty = rowY[r];
	}
	const paths: SkyPath[] = items.flatMap((it) =>
		it.joined.map((s) => {
			const y0 = it.ty + 25;
			const y1 = r1(s.y - Math.max(s.w, s.h) / 2 - 4);
			const my = r1((y0 + y1) / 2);
			return { key: `${it.thread.url}|${s.key}`, url: it.thread.url, d: `M ${it.tx} ${y0} C ${it.tx} ${my} ${s.x} ${my} ${s.x} ${y1}` };
		})
	);
	const labels: ThreadLabel[] = items.map((it) => ({
		thread: it.thread,
		x: it.tx,
		y: it.ty,
		align: 'center',
		why: `${THREAD_KIND[it.thread.kind]} · ${it.joined.length} today`
	}));
	return {
		...EMPTY,
		height: LEVEL_HEIGHT.context,
		stars,
		labels,
		paths,
		now: base.now ? { x: base.now.x, top: 170, height: 60 } : null
	};
}
