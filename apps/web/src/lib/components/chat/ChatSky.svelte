<!--
	ChatSky.svelte

	What an empty new chat shows above the composer when there is a day to
	draw: a dot for each part of it, at one of four levels (`sky.ts`).

	- Day (`GET /api/sky/day`): the day left to right. The hours so far take
	  most of the width and the rest of the day folds into a strip after the
	  dotted now mark. Last night is the soft band at the left edge. What
	  happened is solid, what is happening pulses, and what is still to come
	  is a faint ring.
	- Moment (`GET /api/sky/moment`): one moment opened into a band, with
	  arriving and leaving on it, messages and voice notes above, purchases
	  below.
	- Context (`GET /api/sky/context`): the day with what it belongs to,
	  projects, pages and chats, labeled above the moments they hold.
	- Week (`GET /api/sky/week`): seven columns, one per day, with each night
	  as a dash along the bottom.

	Scrolling over the sky, pinching it, minus and plus, or the column of zoom
	dots over the composer's right edge move between them; a dot names its
	level while the pointer is on it. Zooming in from the week opens the day under
	the pointer, and zooming in from a day opens the moment under it. The
	Day dot on another day goes back to today.

	A dot mentions what it is in the composer: drag it there, with a mouse or
	a finger, or click it to fan out it and what it links to and pick one.
	While an `@` is open in the composer the dots it matches light up; typing
	anything else quiets the sky. Something already mentioned keeps a ring,
	and hovering its chip in the composer lights its dot.

	Before anything is recorded today, or with the sky switched off in
	Settings › Assistant, it shows the mark and the word instead.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { cubicOut } from 'svelte/easing';
	import { fade } from 'svelte/transition';
	import Icon from '$lib/components/Icon.svelte';
	import NewChatMark from './NewChatMark.svelte';
	import { portal } from '$lib/actions/portal';
	import {
		getAssistantProfile,
		getSkyContext,
		getSkyDay,
		getSkyMoment,
		getSkyWeek,
		type SkyContext,
		type SkyDay,
		type SkyMomentDetail,
		type SkyThread,
		type SkyWeek
	} from '$lib/api/client';
	import { MENTION_DROP_ATTR, MENTION_DROP_EVENT } from '$lib/stores/dndManager.svelte';
	import { createNote, getDayByDate, listNotes, type WikiNote } from '$lib/wiki/api';
	import { getLocalDateSlug } from '$lib/utils/dateUtils';
	import {
		LEVELS,
		LEVEL_HEIGHT,
		LEVEL_NAMES,
		SKY_ICONS,
		clock,
		layoutContext,
		layoutDay,
		layoutMoment,
		layoutWeek,
		forgetToday,
		momentMatches,
		shortDate,
		todaySky,
		weekMonday,
		type Layout,
		type Level,
		type Star
	} from './sky';

	let {
		query = null,
		draft = '',
		onMention = undefined
	}: {
		/** What is typed after an open `@` in the composer, else null. */
		query?: string | null;
		/** The composer's markdown, which holds the mentions made so far. */
		draft?: string;
		/** Mention this in the composer, where its `@` is if one is open. */
		onMention?: (url: string, label: string) => void;
	} = $props();

	/** The day the arrival last played, so it plays once a day and not on
	 *  every new chat. */
	const ARRIVED_KEY = 'virtues.chatSky.arrived';
	/** How far a pressed dot moves before it is being dragged. */
	const DRAG_PX = 6;
	/** Where a clicked dot's links open, as angles in degrees by how many
	 *  there are (the board's petals), above the dot, or mirrored below it
	 *  when the dot sits high; and the ellipse they sit on. */
	const PETAL_ANGLES: Record<number, number[]> = { 1: [-90], 2: [-150, -30], 3: [-160, -90, -20], 4: [-165, -120, -60, -15] };
	const PETAL_RX = 112;
	const PETAL_RY = 60;
	/** Wheel events closer together than this are one gesture, however long a
	 *  trackpad's momentum keeps sending them, and one gesture moves at most
	 *  one level. */
	const GESTURE_GAP_MS = 160;
	/** How far one gesture scrolls to move a level, in pixels. A trackpad
	 *  pinch sends small deltas, so they count `PINCH_GAIN` times over. */
	const WHEEL_STEP = 90;
	const PINCH_GAIN = 4;
	/** How far the sky scales toward the next level while a gesture builds,
	 *  and how far it gives at the first and last level. */
	const LEAN_SCALE = 0.05;
	const END_GIVE = 0.25;
	/** How often an open new chat reads the day again, and the least time
	 *  between two reads. */
	const READ_AGAIN_MS = 5 * 60_000;
	const READ_AGAIN_MIN_MS = 60_000;
	/** How far two fingers pinch, as a ratio of where they started, to move a level. */
	const PINCH_STEP = 1.33;

	/** Today, which moves on at midnight while a new chat is open. */
	let today = $state(getLocalDateSlug());

	let ready = $state(false);
	let width = $state(640);
	let hover = $state<string | null>(null);
	let arrive = $state(false);
	let skyEl = $state<HTMLDivElement | null>(null);
	/** The dot fanned out into its links. */
	let open = $state<string | null>(null);
	/** A dot being dragged toward a composer, drawn as a pill under the pointer. */
	let drag = $state<{ label: string; x: number; y: number; over: boolean } | null>(null);
	/** The route of the mention chip the pointer is over in a composer. */
	let chipRoute = $state<string | null>(null);
	/** The context label the pointer is over, which lights its moments. */
	let threadHover = $state<SkyThread | null>(null);
	/** The press that may become a drag or a click. */
	let press: { key: string; pointerId: number; x: number; y: number; moved: boolean } | null = null;

	// What each level has read, kept for the life of this new chat.
	let days = $state<Record<string, SkyDay>>({});
	let weeks = $state<Record<string, SkyWeek>>({});
	let contexts = $state<Record<string, SkyContext>>({});
	let details = $state<Record<string, SkyMomentDetail>>({});
	/** Your notes on each day, newest first, with the day page's id they hang
	 *  on. A note on a moment is a day note whose anchor names the moment, so
	 *  the day page shows it as well. */
	let notes = $state<Record<string, { day: string; list: WikiNote[] }>>({});

	async function readNotes(date: string) {
		try {
			const day = notes[date]?.day ?? (await getDayByDate(date))?.id;
			if (day) notes[date] = { day, list: await listNotes('day', day) };
		} catch {
			// The sky draws without notes; the next read tries again.
		}
	}

	/** The open dot's note being written: the dot, and the words so far. */
	let writing = $state<{ key: string; text: string } | null>(null);
	let noteSaving = $state(false);
	let noteFailed = $state(false);

	const notesOn = (s: Star): WikiNote[] => {
		const id = s.moment?.id;
		return id ? (notes[viewDate]?.list ?? []).filter((n) => n.anchor?.moment === id) : [];
	};

	async function saveNote(s: Star) {
		const body = writing?.text.trim();
		const day = notes[viewDate];
		if (!body || !s.moment || !day || noteSaving) return;
		noteSaving = true;
		noteFailed = false;
		try {
			const n = await createNote('day', day.day, body, { anchor: { quote: s.moment.label, moment: s.moment.id } });
			day.list = [n, ...day.list];
			writing = null;
		} catch {
			noteFailed = true;
		} finally {
			noteSaving = false;
		}
	}

	const focusOnMount = (el: HTMLElement) => el.focus();

	let level = $state<Level>('day');
	/** The day the Day, Context and Moment levels show: today, to start. */
	let viewDate = $state(getLocalDateSlug());
	/** The moment the Moment level shows, by its id in the day. */
	let momentId = $state<string | null>(null);
	let moving = false;
	let pointerX = 320;

	const day = $derived(days[today] ?? null);

	onMount(() => {
		Promise.all([
			// Unreadable preferences leave the sky on, its default.
			getAssistantProfile<{ ui_preferences?: Record<string, unknown> }>().catch(() => null),
			todaySky()
		])
			.then(([profile, sky]) => {
				if (profile?.ui_preferences?.chat_sky === false) return;
				days[today] = sky;
				void readNotes(today);
				if (sky.moments.length === 0) return;
				try {
					arrive = localStorage.getItem(ARRIVED_KEY) !== today;
					localStorage.setItem(ARRIVED_KEY, today);
				} catch {
					arrive = false;
				}
			})
			// A server without the sky, or one that cannot read the day, shows the mark.
			.catch(() => {})
			.finally(() => (ready = true));

		// A chip in a composer that carries a record route (Ref's
		// `data-cite-route`) lights its dot while the pointer is over it.
		const over = (e: PointerEvent) => {
			const chip = (e.target as Element | null)?.closest?.(`[${MENTION_DROP_ATTR}] [data-cite-route]`);
			chipRoute = chip?.getAttribute('data-cite-route') ?? null;
		};
		document.addEventListener('pointerover', over);
		return () => document.removeEventListener('pointerover', over);
	});

	// An open fan closes on Escape or a press anywhere outside the sky.
	$effect(() => {
		if (!open) return;
		const key = (e: KeyboardEvent) => {
			if (e.key === 'Escape') open = null;
		};
		const outside = (e: PointerEvent) => {
			if (!skyEl?.contains(e.target as Node)) open = null;
		};
		window.addEventListener('keydown', key);
		window.addEventListener('pointerdown', outside, true);
		return () => {
			window.removeEventListener('keydown', key);
			window.removeEventListener('pointerdown', outside, true);
		};
	});

	// The wheel is taken only over the sky, and only to move between levels;
	// so are minus and plus while the sky has the focus.
	$effect(() => {
		const el = skyEl;
		if (!el) return;
		el.addEventListener('wheel', onWheel, { passive: false });
		el.addEventListener('keydown', onKey);
		return () => {
			el.removeEventListener('wheel', onWheel);
			el.removeEventListener('keydown', onKey);
		};
	});

	const view = $derived.by((): Layout => {
		const empty: Layout = {
			height: LEVEL_HEIGHT[level],
			stars: [],
			now: null,
			labels: [],
			paths: [],
			letters: [],
			pieceLabels: [],
			head: null
		};
		const shownDay = days[viewDate];
		if (level === 'week') {
			const week = weeks[weekMonday(viewDate)];
			return week ? layoutWeek(week, width, today, viewDate) : empty;
		}
		if (level === 'moment') {
			const detail = momentId ? details[`${viewDate}|${momentId}`] : undefined;
			return detail ? layoutMoment(detail, shownDay ?? null, width) : empty;
		}
		if (!shownDay) return empty;
		if (level === 'context') {
			const context = contexts[viewDate];
			return context ? layoutContext(shownDay, context, width) : empty;
		}
		return layoutDay(shownDay, width);
	});

	/** The zoom dot under the pointer or the keyboard, whose name shows. */
	let rungHover = $state<Level | null>(null);

	/** What a zoom dot is called: its level, the day when the sky shows
	 *  another one, and where the Day dot goes from another day. */
	function rungName(l: Level): string {
		if (viewDate === today || l === 'week' || l === 'moment') return LEVEL_NAMES[l];
		if (l === 'day' && level === 'day') return 'Back to today';
		return `${LEVEL_NAMES[l]} · ${shortDate(viewDate)}`;
	}

	/** How far in from the sky's right edge the zoom dots sit, so their
	 *  column lines up with the right edge of the composer below the sky. */
	let pickerRight = $state<number | null>(null);

	function composerBelow(): Element | null {
		const s = skyEl?.getBoundingClientRect();
		if (!s) return null;
		let best: { el: Element; top: number } | null = null;
		for (const el of document.querySelectorAll(`[${MENTION_DROP_ATTR}]`)) {
			const r = el.getBoundingClientRect();
			if (r.top < s.top || r.right < s.left || r.left > s.right) continue;
			if (!best || r.top < best.top) best = { el, top: r.top };
		}
		return best?.el ?? null;
	}

	function placePicker() {
		const s = skyEl?.getBoundingClientRect();
		const c = composerBelow()?.getBoundingClientRect();
		pickerRight = s && c ? Math.round(s.right - c.right) : null;
	}

	$effect(() => {
		const el = skyEl;
		if (!el) return;
		placePicker();
		const watch = new ResizeObserver(placePicker);
		watch.observe(el);
		const composer = composerBelow();
		if (composer) watch.observe(composer);
		window.addEventListener('resize', placePicker);
		return () => {
			watch.disconnect();
			window.removeEventListener('resize', placePicker);
		};
	});
	const stars = $derived(view.stars);
	/** The dots drawn on this level; the hidden ones only wait to move. */
	const shown = $derived(stars.filter((s) => !s.hidden));

	const shownDay = $derived(days[viewDate] ?? null);
	const next = $derived(shownDay?.moments.find((m) => m.planned) ?? null);

	const weatherLine = $derived.by(() => {
		const w = shownDay?.weather;
		if (!w) return '';
		const temp = w.temperature_c === null ? '' : `${Math.round((w.temperature_c * 9) / 5 + 32)}°, `;
		const condition = w.condition ? w.condition.charAt(0).toLowerCase() + w.condition.slice(1) : '';
		return `${temp}${condition}`.replace(/, $/, '');
	});

	// The dots an open `@` matches, which its menu also lists. Nothing
	// matched leaves the sky as it was rather than dimming all of it.
	const matched = $derived.by((): Set<string> | null => {
		const keys = shown.filter((s) => momentMatches(s, query ?? '')).map((s) => s.key);
		return keys.length > 0 ? new Set(keys) : null;
	});

	const mentioned = $derived(new Set(shown.filter((s) => s.url && draft.includes(`(${s.url})`)).map((s) => s.key)));

	const inThread = (s: Star) => !!threadHover && (!!s.threads?.includes(threadHover.url) || threadHover.moments.includes(s.key));

	const lit = (s: Star) =>
		!!matched?.has(s.key) || (!!chipRoute && (s.url === chipRoute || s.links.some((l) => l.url === chipRoute))) || inThread(s);

	// What steps back while something else is being looked at: the other dots
	// while one is open, while an `@` matches some, or while a thread is held.
	const dim = (s: Star) =>
		(open !== null && open !== s.key) || (matched !== null && !matched.has(s.key)) || (!!threadHover && !inThread(s));

	// Writing a message, with no `@` open, quiets the sky until you reach for it.
	const quiet = $derived(draft.trim() !== '' && query === null);

	type Tip = { x: number; y: number; title: string; why: string; above?: boolean; note?: string };
	const tip = $derived.by((): Tip | null => {
		if (!hover || drag) return null;
		const clampX = (x: number) => Math.min(width - 90, Math.max(90, x));
		if (hover === 'now' && view.now && shownDay?.now_hour != null) {
			const title = weatherLine ? `${clock(shownDay.now_hour)} · ${weatherLine}` : clock(shownDay.now_hour);
			const why = next ? `Next, ${next.label} at ${clock(next.start_hour)}` : 'Nothing else on your calendar today';
			return { x: clampX(view.now.x), y: view.now.top - 46, title, why };
		}
		if (threadHover && hover === `thread:${threadHover.url}`) {
			const l = view.labels.find((l) => l.thread.url === threadHover?.url);
			if (l) return { x: clampX(l.align === 'right' ? l.x - 60 : l.x), y: l.y + 30, title: l.thread.label, why: l.why };
		}
		if (open) return null;
		const s = shown.find((s) => s.key === hover);
		// A moment's capsule is already named above it.
		if (!s || s.look === 'cap') return null;
		// Over a low dot the tip grows upward from just above it, so a note
		// line never covers the dot.
		const above = s.y > view.height / 2;
		return { x: clampX(s.x), y: above ? s.y - 14 : s.y + 16, above, title: s.label, why: s.when, note: notesOn(s)[0]?.body };
	});

	type Petal = {
		url: string;
		label: string;
		strong: string;
		rest: string;
		icon: string;
		x: number;
		y: number;
		side: 'left' | 'right' | 'mid';
		fx: number;
		fy: number;
		delay: number;
	};

	// A clicked dot opens its links around it, as the board's petals: on an
	// ellipse above the dot, or below it when the dot sits high, each joined
	// to it by a spoke. Above them, its name (which mentions it), when it
	// was, and your notes on it. A capsule opens from where it was pressed.
	const bloom = $derived.by(() => {
		const s = shown.find((s) => s.key === open);
		if (!s) return null;
		// An opened capsule's head takes the level's title place at the top,
		// and its petals sit on a flatter curve above the line, clear of the
		// pieces' labels below it.
		const cap = s.look === 'cap';
		const at = { x: cap ? Math.min(Math.max(pointerX, 40), width - 40) : s.x, y: s.y };
		const up = cap || at.y >= view.height / 2 - 8;
		const ry = cap ? 36 : PETAL_RY;
		const links = s.links.slice(0, 4);
		const angles = PETAL_ANGLES[links.length] ?? [];
		const petals: Petal[] = links.map((l, i) => {
			const a = ((up ? angles[i] : -angles[i]) * Math.PI) / 180;
			const dx = Math.cos(a) * PETAL_RX;
			const dy = Math.sin(a) * ry;
			// A purchase reads as its amount, then where.
			const [strong, rest] = l.kind === 'purchase' && l.label.includes(' at ') ? [l.label.split(' at ')[0], ` at ${l.label.split(' at ').slice(1).join(' at ')}`] : [l.label, ''];
			return {
				url: l.url,
				label: l.label,
				strong,
				rest,
				icon: SKY_ICONS[l.kind],
				x: Math.round((at.x + dx) * 10) / 10,
				y: Math.round((at.y + dy) * 10) / 10,
				side: dx < -8 ? 'left' : dx > 8 ? 'right' : 'mid',
				fx: Math.round(-dx * 10) / 10,
				fy: Math.round(-dy * 10) / 10,
				delay: 40 + i * 45
			};
		});
		// The head's bottom edge sits above the dot and its petals, and it
		// grows upward as notes are added, so it never runs into the composer
		// under the sky.
		const head = cap && view.head
			? { x: view.head.x, y: 54 }
			: { x: Math.min(Math.max(at.x, 110), width - 110), y: Math.min(at.y - 16, ...petals.map((p) => p.y - 18)) };
		return { s, at, petals, head, notes: notesOn(s).slice(0, 3) };
	});

	/** The moment to open when zooming in from the day: the one under the
	 *  pointer or fanned out, else the one going on, else the latest. */
	function focusMoment(d: SkyDay): string | null {
		const inDay = (k: string | null) => (k && d.moments.some((m) => m.id === k) ? k : null);
		const live = d.moments.find((m) => !m.planned && m.kind !== 'sleep' && d.now_hour !== null && m.start_hour <= d.now_hour && d.now_hour <= m.end_hour + 1 / 3);
		const past = d.moments.filter((m) => !m.planned);
		return inDay(hover) ?? inDay(open) ?? live?.id ?? past.at(-1)?.id ?? d.moments[0]?.id ?? null;
	}

	async function load(next: Level, date: string, moment: string | null): Promise<boolean> {
		if (next === 'week') {
			weeks[weekMonday(date)] ??= await getSkyWeek(date);
			return true;
		}
		days[date] ??= date === today ? await todaySky() : await getSkyDay(date);
		if (!(date in notes)) void readNotes(date);
		if (next === 'context') contexts[date] ??= await getSkyContext(date);
		if (next === 'moment') {
			if (!moment) return false;
			details[`${date}|${moment}`] ??= await getSkyMoment(date, moment);
		}
		return true;
	}

	/** Move to a level, reading what it needs first; the sky stays where it
	 *  was if the read fails. */
	async function goTo(next: Level, opts: { date?: string } = {}) {
		if (moving || (next === level && !opts.date)) return;
		moving = true;
		const date = opts.date ?? viewDate;
		try {
			let moment: string | null = null;
			if (next === 'moment') {
				const d = days[date] ?? (date === today ? await todaySky() : await getSkyDay(date));
				moment = focusMoment(d);
			}
			if (!(await load(next, date, moment))) return;
			viewDate = date;
			momentId = moment ?? momentId;
			level = next;
			open = null;
			hover = null;
			arrive = false;
		} catch (e) {
			// The level could not be read; the sky stays on the one it shows.
			console.error('chat sky: could not open the level', next, e);
		} finally {
			moving = false;
			lean = 0;
		}
	}

	/** A zoom dot. The Day dot on another day goes back to today. */
	function pickLevel(l: Level) {
		if (l === 'day' && level === 'day' && viewDate !== today) void goTo('day', { date: today });
		else void goTo(l);
	}

	function onKey(e: KeyboardEvent) {
		if (e.target !== skyEl) return;
		if (e.key === '-' || e.key === '_') step(1);
		else if (e.key === '=' || e.key === '+') step(-1);
		else return;
		e.preventDefault();
	}

	/** One level coarser (1) or finer (-1). Zooming in from the week opens
	 *  the day under `x`. */
	function step(dir: 1 | -1, x?: number) {
		const i = LEVELS.indexOf(level) + dir;
		if (i < 0 || i >= LEVELS.length) return;
		void goTo(LEVELS[i], { date: level === 'week' && x !== undefined ? dayUnder(x) : undefined });
	}

	/** How far the current gesture has gone toward the next level, from -1
	 *  (finer) to 1 (coarser); the sky leans by it and springs back to 0. */
	let lean = $state(0);
	let leanX = $state(320);
	let gesture = { sum: 0, last: 0, done: false };
	/** The level a gesture leans toward, whose zoom dot grows with it. */
	const leanTarget = $derived(lean === 0 ? null : (LEVELS[LEVELS.indexOf(level) + (lean > 0 ? 1 : -1)] ?? null));
	let settle: ReturnType<typeof setTimeout> | undefined;

	function onWheel(e: WheelEvent) {
		e.preventDefault();
		const t = performance.now();
		if (t - gesture.last > GESTURE_GAP_MS) {
			gesture = { sum: 0, last: t, done: false };
			prefetchAround(localX(e.clientX));
		}
		gesture.last = t;
		// When the wheel goes quiet the gesture is over: a lean that did not
		// reach the next level springs back, unless a level is on its way.
		clearTimeout(settle);
		settle = setTimeout(() => {
			if (!moving) lean = 0;
		}, GESTURE_GAP_MS);
		if (gesture.done) return;
		const px = e.deltaMode === 1 ? e.deltaY * 16 : e.deltaMode === 2 ? e.deltaY * 400 : e.deltaY;
		gesture.sum += e.ctrlKey ? px * PINCH_GAIN : px;
		leanX = localX(e.clientX);
		const dir: 1 | -1 = gesture.sum > 0 ? 1 : -1;
		const next = LEVELS.indexOf(level) + dir;
		const reach = gesture.sum / WHEEL_STEP;
		if (next < 0 || next >= LEVELS.length) {
			lean = Math.max(-END_GIVE, Math.min(END_GIVE, reach * END_GIVE));
			return;
		}
		lean = Math.max(-1, Math.min(1, reach));
		if (Math.abs(reach) < 1) return;
		gesture.done = true;
		step(dir, leanX);
	}

	/** Read the levels either side of this one as a gesture starts, so the
	 *  move does not wait on the server. */
	function prefetchAround(x: number) {
		const i = LEVELS.indexOf(level);
		const finer = LEVELS[i - 1];
		const coarser = LEVELS[i + 1];
		const date = level === 'week' ? (dayUnder(x) ?? viewDate) : viewDate;
		const moment = finer === 'moment' && days[date] ? focusMoment(days[date]) : null;
		if (finer) load(finer, date, moment).catch(() => {});
		if (coarser) load(coarser, viewDate, null).catch(() => {});
	}

	/** The week's day nearest `x`, by its letter's place. */
	function dayUnder(x: number): string | undefined {
		if (view.letters.length === 0) return undefined;
		return view.letters.reduce((a, l) => (Math.abs(l.x - x) < Math.abs(a.x - x) ? l : a)).date;
	}

	// Read the day again while a new chat stays open: when the window comes
	// back, and every few minutes, so now moves on and what was planned
	// becomes what happened. Past midnight, today becomes the new day.
	let lastRead = Date.now();
	async function readAgain() {
		if (Date.now() - lastRead < READ_AGAIN_MIN_MS) return;
		lastRead = Date.now();
		forgetToday();
		const now = getLocalDateSlug();
		try {
			const fresh = await todaySky();
			if (now !== today) {
				if (viewDate === today) viewDate = now;
				today = now;
			}
			days[now] = fresh;
			void readNotes(now);
			const monday = weekMonday(now);
			if (level === 'context' && viewDate === now) contexts[now] = await getSkyContext(now);
			else delete contexts[now];
			if (level === 'week' && weekMonday(viewDate) === monday) weeks[monday] = await getSkyWeek(now);
			else delete weeks[monday];
		} catch {
			// The sky keeps what it drew; the next read tries again.
		}
	}

	$effect(() => {
		const visible = () => {
			if (document.visibilityState === 'visible') void readAgain();
		};
		const every = setInterval(() => {
			if (document.visibilityState === 'visible') void readAgain();
		}, READ_AGAIN_MS);
		window.addEventListener('focus', visible);
		document.addEventListener('visibilitychange', visible);
		return () => {
			clearInterval(every);
			clearTimeout(settle);
			window.removeEventListener('focus', visible);
			document.removeEventListener('visibilitychange', visible);
		};
	});

	function localX(clientX: number): number {
		return clientX - (skyEl?.getBoundingClientRect().left ?? 0);
	}

	// Two fingers on the sky pinch between levels.
	const fingers = new Map<number, { x: number; y: number }>();
	let pinchFrom = 0;
	const spread = () => {
		const [a, b] = [...fingers.values()];
		return Math.hypot(a.x - b.x, a.y - b.y);
	};
	function skyDown(e: PointerEvent) {
		pointerX = localX(e.clientX);
		if (e.pointerType !== 'touch') return;
		fingers.set(e.pointerId, { x: e.clientX, y: e.clientY });
		if (fingers.size === 2) {
			pinchFrom = spread();
			cancel();
		}
	}
	function skyMove(e: PointerEvent) {
		pointerX = localX(e.clientX);
		if (!fingers.has(e.pointerId)) return;
		fingers.set(e.pointerId, { x: e.clientX, y: e.clientY });
		if (fingers.size !== 2 || pinchFrom === 0) return;
		const ratio = spread() / pinchFrom;
		if (ratio < 1 / PINCH_STEP || ratio > PINCH_STEP) {
			const mid = [...fingers.values()].reduce((a, f) => a + f.x, 0) / 2;
			step(ratio < 1 ? 1 : -1, localX(mid));
			pinchFrom = spread();
		}
	}
	function skyUp(e: PointerEvent) {
		fingers.delete(e.pointerId);
		if (fingers.size < 2) pinchFrom = 0;
	}

	// A mouse names a dot while it rests on it. A finger, which also fires
	// enter, gets the fan on a tap instead.
	function enter(e: PointerEvent, key: string) {
		if (e.pointerType === 'mouse') hover = key;
	}
	function leave(e: PointerEvent) {
		if (e.pointerType === 'mouse') hover = null;
	}
	function tapNow(e: PointerEvent) {
		if (e.pointerType !== 'mouse') hover = hover === 'now' ? null : 'now';
	}

	function composerAt(x: number, y: number): Element | null {
		for (const el of document.elementsFromPoint(x, y)) {
			const composer = el.closest(`[${MENTION_DROP_ATTR}]`);
			if (composer) return composer;
		}
		return null;
	}

	// A press on a dot is a drag once it moves, and a click to fan it out if
	// it does not. The default is stopped so the composer keeps its focus and
	// any open `@`.
	function down(e: PointerEvent, key: string) {
		if (e.button !== 0 || fingers.size > 1) return;
		e.preventDefault();
		press = { key, pointerId: e.pointerId, x: e.clientX, y: e.clientY, moved: false };
		(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
	}

	function move(e: PointerEvent, s: Star) {
		if (!press || press.pointerId !== e.pointerId || !s.url) return;
		if (!press.moved && Math.hypot(e.clientX - press.x, e.clientY - press.y) < DRAG_PX) return;
		press.moved = true;
		hover = null;
		open = null;
		drag = { label: s.label, x: e.clientX, y: e.clientY, over: composerAt(e.clientX, e.clientY) !== null };
	}

	function up(e: PointerEvent, s: Star) {
		if (!press || press.pointerId !== e.pointerId) return;
		const dragged = press.moved;
		press = null;
		drag = null;
		if (dragged) {
			const at = { x: e.clientX, y: e.clientY };
			composerAt(at.x, at.y)?.dispatchEvent(
				new CustomEvent(MENTION_DROP_EVENT, { detail: { url: s.url, label: s.label, at } })
			);
			return;
		}
		hover = null;
		open = open === s.key ? null : s.key;
	}

	function cancel() {
		press = null;
		drag = null;
	}

	function pick(url: string, label: string) {
		onMention?.(url, label);
		open = null;
	}

	// Moving between levels moves the marks themselves (CSS transitions on
	// their place and size); only a mark the last level did not have appears,
	// out of where `from` says, after its `delay`.
	function appear(node: Element, { from, delay = 0 }: { from?: { dx: number; dy: number }; delay?: number }) {
		const still = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
		const dx = from?.dx ?? 0;
		const dy = from?.dy ?? 0;
		return {
			delay: still ? 0 : delay,
			duration: still ? 0 : 420,
			easing: cubicOut,
			css: (t: number) =>
				`opacity: ${t}; transform: translate(${dx * (1 - t)}px, ${dy * (1 - t)}px) scale(${0.4 + 0.6 * t})`
		};
	}
</script>

{#if ready && day && day.moments.length > 0}
	<!-- The sky takes focus only so minus and plus zoom it, and its pointer
	     handlers track pinches. The zoom dots are the buttons, and the
	     composer's @ reaches every moment. -->
	<!-- svelte-ignore a11y_no_noninteractive_tabindex a11y_no_static_element_interactions -->
	<div
		bind:this={skyEl}
		class="sky"
		class:arrive
		class:quiet
		class:engaged={open !== null || drag !== null}
		style:height="{view.height}px"
		bind:clientWidth={width}
		role="group"
		tabindex="0"
		aria-label="Your chat sky, zoomed to {LEVEL_NAMES[level].toLowerCase()}. Scroll, or press minus and plus, to zoom between one moment, the day, what it belongs to, and the week."
		in:fade={{ duration: 200 }}
		onpointerdown={skyDown}
		onpointermove={skyMove}
		onpointerup={skyUp}
		onpointercancel={skyUp}
	>
		<div class="zoom" role="group" aria-label="Zoom" style="right: {pickerRight ?? 0}px">
			{#each [...LEVELS].reverse() as l (l)}
				<div class="rung-row">
					<button
						type="button"
						class="rung"
						aria-label={rungName(l)}
						aria-pressed={l === level}
						onpointerenter={() => (rungHover = l)}
						onpointerleave={() => (rungHover = null)}
						onfocus={() => (rungHover = l)}
						onblur={() => (rungHover = null)}
						onpointerdown={(e) => e.preventDefault()}
						onclick={() => pickLevel(l)}
						><span style:transform={l === leanTarget ? `scale(${1 + 0.8 * Math.abs(lean)})` : undefined}></span></button
					>
					{#if rungHover === l}<span class="rung-name" aria-hidden="true">{rungName(l)}</span>{/if}
				</div>
			{/each}
		</div>

		<div
			class="layer"
			style="transform: scale({1 + (lean > 0 ? -LEAN_SCALE : LEAN_SCALE) * Math.abs(lean)}); transform-origin: {leanX}px 60%"
		>
			{#if view.now}
				<!-- svelte-ignore a11y_no_static_element_interactions -- hover only adds what the list below already says to screen readers -->
				<span
					class="now"
					style="left: {view.now.x}px; top: {view.now.top}px; height: {view.now.height}px"
					aria-hidden="true"
					onpointerenter={(e) => enter(e, 'now')}
					onpointerleave={leave}
					onpointerdown={tapNow}
				></span>
			{/if}
			{#if view.paths.length > 0}
				<svg class="paths" class:week={level === 'week'} width={width} height={view.height} aria-hidden="true">
					{#each view.paths as path, i (path.key)}
						<path
							in:fade={{ duration: 500, delay: 150 + i * 50 }}
							out:fade={{ duration: 160 }}
							d={path.d}
							class:hot={threadHover?.url === path.url}
							class:faded={!!threadHover && threadHover.url !== path.url}
						/>
					{/each}
				</svg>
			{/if}
			{#each view.labels as l (l.thread.url)}
				<button
					type="button"
					class="thread"
					in:fade={{ duration: 450, delay: 120 }}
					out:fade={{ duration: 160 }}
					class:right={l.align === 'right'}
					class:hot={threadHover?.url === l.thread.url}
					class:faded={!!threadHover && threadHover.url !== l.thread.url}
					style="left: {l.x}px; top: {l.y}px"
					aria-label="{l.thread.label}, {l.why}. Mention it."
					onpointerenter={() => {
						threadHover = l.thread;
						hover = `thread:${l.thread.url}`;
					}}
					onpointerleave={() => {
						threadHover = null;
						hover = null;
					}}
					onpointerdown={(e) => e.preventDefault()}
					onclick={() => onMention?.(l.thread.url, l.thread.label)}
				>
					<Icon icon={SKY_ICONS[l.thread.kind]} width="12" class="thread-icon" />
					<span>{l.thread.label}</span>
				</button>
			{:else}
				{#if level === 'context'}
					<span class="note" in:fade={{ duration: 450, delay: 250 }} aria-hidden="true">Nothing today belongs to a project, page or chat yet.</span>
				{/if}
			{/each}
			{#each view.letters as l (l.date)}
				<span class="letter" in:fade={{ duration: 450, delay: 250 }} out:fade={{ duration: 160 }} class:today={l.today} style="left: {l.x}px; top: {view.height - 18}px" aria-hidden="true">{l.text}</span>
			{/each}
			<!-- An opened capsule's head names it already. -->
			{#if view.head && bloom?.s.look !== 'cap'}
				<div class="moment-head" in:fade={{ duration: 450, delay: 250 }} out:fade={{ duration: 160 }} style="left: {view.head.x}px; top: {view.head.y}px" aria-hidden="true">
					<div>{view.head.title}</div>
					<div class="tip-why">{view.head.why}</div>
				</div>
			{/if}
			{#each view.pieceLabels as p (p.key)}
				<span class="piece-label" in:fade={{ duration: 450, delay: 300 }} out:fade={{ duration: 160 }} style="left: {p.x}px; top: {p.y}px" aria-hidden="true">{p.text}</span>
			{/each}
			{#each stars as s (s.key)}
				{@const hit = Math.max(26, s.w + 10)}
				<!-- svelte-ignore a11y_no_static_element_interactions -- the petals' buttons and the composer's @ are the ways in without a pointer -->
				<span
					class="dot"
					class:hidden={s.hidden}
					in:appear={{ from: s.from ? { dx: s.from.x - s.x, dy: s.from.y - s.y } : undefined, delay: s.delay }}
					out:fade={{ duration: 200 }}
					class:mentionable={!!s.url && !s.hidden}
					class:lit={lit(s)}
					class:dim={dim(s)}
					class:open={open === s.key}
					style="left: {s.x}px; top: {s.y}px; width: {hit}px; height: {Math.max(26, s.h + 10)}px; margin-left: {-hit / 2}px; margin-top: {-Math.max(26, s.h + 10) / 2}px; --d: {Math.min(1300, s.order * 70)}ms; --tw: {s.breathe}s"
					aria-hidden="true"
					onpointerenter={(e) => enter(e, s.key)}
					onpointerleave={leave}
					onpointerdown={(e) => down(e, s.key)}
					onpointermove={(e) => move(e, s)}
					onpointerup={(e) => up(e, s)}
					onpointercancel={cancel}
				>
					<span
						class="ink {s.look}"
						class:accent={s.accent}
						class:mentioned={mentioned.has(s.key)}
						style="width: {s.w}px; height: {s.h}px; --tone: {s.tone}%"
					></span>
				</span>
			{/each}
		</div>

		{#if bloom}
			<svg class="spokes" width={width} height={view.height} aria-hidden="true">
				{#each bloom.petals as p (p.url)}
					<line x1={bloom.at.x} y1={bloom.at.y} x2={p.x} y2={p.y} />
				{/each}
			</svg>
			<div class="bloom-head" style="left: {bloom.head.x}px; top: {bloom.head.y}px">
				{#if bloom.s.url}
					{@const s = bloom.s}
					<button type="button" class="bloom-title" title="Mention {s.label}" onpointerdown={(e) => e.preventDefault()} onclick={() => s.url && pick(s.url, s.label)}
						>{s.label}</button
					>
				{:else}
					<div>{bloom.s.label}</div>
				{/if}
				<div class="tip-why">{bloom.s.when}</div>
				{#each bloom.notes as n (n.id)}
					<div class="moment-note">{n.body}</div>
				{/each}
				{#if bloom.s.moment && notes[viewDate]}
					{@const s = bloom.s}
					{#if writing?.key === s.key}
						<input
							class="note-input"
							bind:value={writing.text}
							placeholder="Add a note"
							aria-label="Note on {s.label}"
							disabled={noteSaving}
							use:focusOnMount
							onkeydown={(e) => {
								if (e.key === 'Enter' && !e.isComposing) {
									e.preventDefault();
									void saveNote(s);
								}
							}}
						/>
						{#if noteFailed}<div class="tip-why">Your note wasn't saved. Press Enter to try again.</div>{/if}
					{:else}
						<button
							type="button"
							class="note-add"
							onclick={() => {
								writing = { key: s.key, text: '' };
								noteFailed = false;
							}}>Add a note</button
						>
					{/if}
				{/if}
			</div>
			<div role="group" aria-label="Mention {bloom.s.label} or what it links to">
				{#each bloom.petals as p (p.url)}
					<div class="petal {p.side}" style="left: {p.x}px; top: {p.y}px">
						<button
							type="button"
							style="--fx: {p.fx}px; --fy: {p.fy}px; --pd: {p.delay}ms"
							aria-label="Mention {p.label}"
							onpointerdown={(e) => e.preventDefault()}
							onclick={() => pick(p.url, p.label)}
						>
							<Icon icon={p.icon} width="12" class="thread-icon" />
							<span><b>{p.strong}</b>{p.rest}</span>
						</button>
					</div>
				{/each}
			</div>
		{/if}
		{#if tip}
			<div class="tip" class:above={tip.above} style="left: {tip.x}px; top: {tip.y}px" aria-hidden="true">
				<div>{tip.title}</div>
				<div class="tip-why">{tip.why}</div>
				{#if tip.note}<div class="moment-note">{tip.note}</div>{/if}
			</div>
		{/if}
		<ul class="sr-only">
			{#each shown as s (s.key)}
				<li>{s.label}, {s.when}</li>
			{/each}
		</ul>
	</div>
	{#if drag}
		<div class="ghost" class:over={drag.over} style="left: {drag.x}px; top: {drag.y}px" use:portal aria-hidden="true">
			{drag.label}
		</div>
	{/if}
{:else if ready}
	<NewChatMark />
{/if}

<style>
	.sky {
		position: relative;
		width: min(640px, calc(100% - 32px));
		height: 140px;
		pointer-events: auto;
		color: var(--color-foreground);
		/* Two fingers pinch the sky between levels instead of zooming the page. */
		touch-action: none;
		transition:
			opacity 240ms var(--ease-premium),
			height 650ms cubic-bezier(0.22, 1, 0.36, 1);
	}

	/* Writing a message quiets the sky; reaching for it brings it back. */
	.sky.quiet:not(:hover):not(.engaged) {
		opacity: 0.4;
	}

	.layer {
		position: absolute;
		inset: 0;
	}

	/* The zoom: a column of dots at the foot of the sky, the week at the top
	   and the moment at the bottom, lined up over the composer's right edge.
	   The current one is larger; a dot names its level only while the
	   pointer or the keyboard is on it. */
	.zoom {
		position: absolute;
		bottom: 6px;
		display: flex;
		flex-direction: column;
		gap: 2px;
		z-index: 2;
	}

	.rung-row {
		position: relative;
		height: 18px;
	}

	.rung {
		width: 20px;
		height: 18px;
		display: grid;
		place-items: center;
		padding: 0;
		cursor: pointer;
	}

	.rung span {
		display: block;
		width: 5px;
		height: 5px;
		border-radius: var(--radius-full);
		background: color-mix(in srgb, currentColor 22%, transparent);
		transition:
			width 200ms ease,
			height 200ms ease,
			background-color 200ms ease,
			transform 120ms ease-out;
	}

	.rung:hover span {
		background: color-mix(in srgb, currentColor 45%, transparent);
	}

	.rung[aria-pressed='true'] span {
		width: 7px;
		height: 7px;
		background: color-mix(in srgb, currentColor 75%, transparent);
	}

	.rung:focus-visible {
		outline: 2px solid var(--color-foreground);
		outline-offset: 1px;
		border-radius: var(--radius-full);
	}

	.rung-name {
		position: absolute;
		left: 24px;
		top: 50%;
		transform: translateY(-50%);
		font-size: 11px;
		color: var(--color-foreground-subtle);
		white-space: nowrap;
		pointer-events: none;
	}

	.sky:focus-visible {
		outline: 2px solid var(--color-foreground);
		outline-offset: 8px;
		border-radius: var(--radius-md);
	}

	.moment-head,
	.bloom-head {
		position: absolute;
		transform: translateX(-50%);
		text-align: center;
		white-space: nowrap;
		font-size: 14px;
		line-height: 20px;
	}

	/* The level's own title only names; the open dot's head holds buttons. */
	.moment-head {
		pointer-events: none;
	}

	.dot {
		position: absolute;
		height: 26px;
		margin-top: -13px;
		display: grid;
		place-items: center;
		border-radius: var(--radius-full);
		cursor: default;
		/* A finger drags the dot instead of scrolling the page. */
		touch-action: none;
		/* Between levels a dot moves to its new place rather than fading. */
		transition:
			left 650ms cubic-bezier(0.22, 1, 0.36, 1),
			top 650ms cubic-bezier(0.22, 1, 0.36, 1),
			opacity 400ms ease;
	}

	.dot.mentionable {
		cursor: grab;
	}

	.dot.dim {
		opacity: 0.22;
	}

	/* Folded into a week dot, or off the edge of an opened moment. */
	.dot.hidden {
		opacity: 0;
		pointer-events: none;
	}

	/* Two tones: the text color, soft, for the ordinary; the theme's accent
	   for what is happening now and what was unusual, so the eye has one
	   place to land. */
	.ink {
		--ink: currentColor;
		position: relative;
		display: block;
		box-sizing: border-box;
		border-radius: var(--radius-full);
		background: color-mix(in srgb, var(--ink) var(--tone), transparent);
		transition:
			width 650ms cubic-bezier(0.22, 1, 0.36, 1),
			height 650ms cubic-bezier(0.22, 1, 0.36, 1),
			background-color 300ms ease,
			transform 220ms var(--ease-premium);
	}

	.ink.accent {
		--ink: var(--color-primary);
	}

	/* Still to come: a faint ring, quieter than anything that happened. */
	.ink.planned {
		background: transparent;
		border: 1px solid color-mix(in srgb, currentColor 24%, transparent);
	}

	/* Happening now: solid, with a ring that slowly leaves it. */
	.ink.ongoing::after {
		content: '';
		position: absolute;
		inset: -3px;
		border-radius: var(--radius-full);
		border: 1px solid color-mix(in srgb, var(--ink) 50%, transparent);
	}

	/* Last night, lying on the horizon and coming in from the left edge. */
	.ink.night {
		background: linear-gradient(to right, transparent, color-mix(in srgb, var(--ink) var(--tone), transparent) 45%);
	}

	/* A moment in the message keeps a ring round its dot. */
	.ink.mentioned {
		outline: 1px solid color-mix(in srgb, var(--color-primary) 60%, transparent);
		outline-offset: 3px;
	}

	.dot:hover .ink,
	.dot.lit .ink,
	.dot.open .ink {
		transform: scale(1.4);
	}

	.dot:hover .ink.night,
	.dot.lit .ink.night,
	.dot.open .ink.night {
		transform: scaleY(1.6);
	}

	/* A moment opened at the Moment level: a soft capsule the line runs
	   through, which does not grow on hover. */
	.ink.cap {
		border-radius: var(--radius-full);
	}

	.dot:hover .ink.cap,
	.dot.lit .ink.cap,
	.dot.open .ink.cap {
		transform: none;
		--tone: 10%;
	}

	.dot.lit .ink:not(.planned):not(.night):not(.cap),
	.dot.open .ink:not(.planned):not(.night):not(.cap) {
		background: color-mix(in srgb, var(--color-primary) 85%, transparent);
	}

	.dot.lit .ink.planned {
		border-color: color-mix(in srgb, var(--color-primary) 78%, transparent);
	}

	.now {
		position: absolute;
		width: 16px;
		margin-left: -8px;
		display: flex;
		justify-content: center;
		cursor: default;
		transition:
			left 650ms cubic-bezier(0.22, 1, 0.36, 1),
			top 650ms cubic-bezier(0.22, 1, 0.36, 1),
			height 650ms cubic-bezier(0.22, 1, 0.36, 1);
	}

	.now::before {
		content: '';
		width: 1px;
		height: 100%;
		background-image: repeating-linear-gradient(
			to bottom,
			color-mix(in srgb, currentColor 34%, transparent) 0 1px,
			transparent 1px 4px
		);
	}

	.letter,
	.piece-label {
		position: absolute;
		transform: translateX(-50%);
		white-space: nowrap;
		pointer-events: none;
		font-size: 11px;
		color: var(--color-foreground-subtle);
	}

	.letter.today {
		color: var(--color-foreground);
	}

	.piece-label {
		font-size: 12px;
	}

	.note {
		position: absolute;
		left: 50%;
		top: 20px;
		transform: translateX(-50%);
		white-space: nowrap;
		font-size: 12px;
		color: var(--color-foreground-subtle);
		pointer-events: none;
	}

	.paths,
	.spokes {
		position: absolute;
		inset: 0;
		overflow: visible;
		pointer-events: none;
	}

	.paths path {
		fill: none;
		stroke: color-mix(in srgb, currentColor 32%, transparent);
		stroke-width: 1;
		stroke-dasharray: 1 3;
		transition: stroke 160ms ease;
	}

	/* A week's storylines are drawn whole, the way the board draws them. */
	.paths.week path {
		stroke: color-mix(in srgb, currentColor 22%, transparent);
		stroke-dasharray: none;
	}

	.paths path.hot {
		stroke: color-mix(in srgb, var(--color-primary) 72%, transparent);
		stroke-width: 1.4;
	}

	.paths path.faded {
		stroke: color-mix(in srgb, currentColor 8%, transparent);
	}

	.spokes line {
		stroke: color-mix(in srgb, currentColor 26%, transparent);
		stroke-width: 1;
		stroke-dasharray: 1 3;
	}

	/* A thread: above its moments at Context, in the gutter at Week. */
	.thread,
	.petal button {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		height: 24px;
		padding: 0 12px;
		border-radius: var(--radius-full);
		border: 1px solid color-mix(in srgb, currentColor 12%, transparent);
		background: color-mix(in srgb, var(--color-background) 94%, transparent);
		color: var(--color-foreground);
		font-size: 12px;
		white-space: nowrap;
		cursor: pointer;
		transition:
			left 650ms cubic-bezier(0.22, 1, 0.36, 1),
			top 650ms cubic-bezier(0.22, 1, 0.36, 1),
			transform 650ms cubic-bezier(0.22, 1, 0.36, 1),
			border-color 160ms ease,
			opacity 160ms ease;
	}

	.thread {
		position: absolute;
		transform: translateX(-50%);
		max-width: 220px;
		z-index: 1;
	}

	.thread span {
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.thread.right {
		transform: translateX(-100%);
		border-color: transparent;
		background: transparent;
	}

	.thread.hot,
	.thread:hover,
	.thread:focus-visible,
	.petal button:hover,
	.petal button:focus-visible {
		border-color: color-mix(in srgb, var(--color-primary) 40%, transparent);
	}

	.thread.faded {
		opacity: 0.35;
	}

	.thread :global(.thread-icon),
	.petal :global(.thread-icon) {
		flex: none;
		color: var(--color-foreground-subtle);
	}

	.thread:focus-visible,
	.petal button:focus-visible,
	.bloom-title:focus-visible {
		outline: 2px solid var(--color-foreground);
		outline-offset: 2px;
	}

	.bloom-head {
		text-align: center;
		white-space: nowrap;
		font-size: 13px;
		line-height: 18px;
		padding: 4px 12px;
		border-radius: var(--radius-md);
		background: color-mix(in srgb, var(--color-background) 94%, transparent);
		z-index: 3;
	}

	.bloom-head,
	.tip.above {
		transform: translate(-50%, -100%);
	}

	/* Your notes on a moment, under its name. */
	.moment-note {
		max-width: 220px;
		margin: 4px auto 0;
		white-space: normal;
		font-size: 12px;
		line-height: 16px;
		color: var(--color-foreground);
	}

	.note-add {
		font-size: 12px;
		color: var(--color-foreground-subtle);
		cursor: pointer;
	}

	.note-add:hover {
		color: var(--color-foreground);
	}

	.note-add:focus-visible {
		outline: 2px solid var(--color-foreground);
		outline-offset: 2px;
	}

	.note-input {
		display: block;
		width: 220px;
		margin: 4px auto 0;
		padding: 4px 8px;
		font-size: 12px;
		line-height: 16px;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		background: var(--color-background);
		color: var(--color-foreground);
	}

	.note-input:focus {
		outline: none;
		border-color: var(--color-border-focus);
	}

	.bloom-title {
		color: var(--color-foreground);
		cursor: pointer;
	}

	.bloom-title:hover {
		text-decoration: underline;
		text-decoration-color: color-mix(in srgb, currentColor 40%, transparent);
	}

	/* The petals: a clicked dot's links, on an ellipse around it. */
	.petal {
		position: absolute;
		z-index: 3;
	}

	.petal.left {
		transform: translate(-100%, -50%);
	}

	.petal.right {
		transform: translate(0, -50%);
	}

	.petal.mid {
		transform: translate(-50%, -50%);
	}

	.petal b {
		font-weight: 500;
	}

	.ghost {
		position: fixed;
		transform: translate(12px, -50%);
		pointer-events: none;
		z-index: 1000;
		max-width: 280px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: 13px;
		line-height: 22px;
		padding: 0 10px;
		border-radius: var(--radius-full);
		border: 1px solid color-mix(in srgb, var(--color-foreground) 14%, transparent);
		background: var(--color-background);
		color: var(--color-foreground);
		opacity: 0.85;
	}

	/* Over a composer: letting go here mentions it. */
	.ghost.over {
		opacity: 1;
		border-color: color-mix(in srgb, var(--color-foreground) 40%, transparent);
	}

	.tip {
		position: absolute;
		transform: translateX(-50%);
		pointer-events: none;
		white-space: nowrap;
		text-align: center;
		font-size: 13px;
		line-height: 18px;
		padding: 4px 12px;
		border-radius: var(--radius-md);
		background: color-mix(in srgb, var(--color-background) 94%, transparent);
		color: var(--color-foreground);
		z-index: 3;
	}

	.tip-why {
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}

	@media (prefers-reduced-motion: no-preference) {
		/* The day arriving in the order it happened, on the first new chat of
		   the day; afterwards each dot breathes on its own clock. */
		.arrive .ink {
			animation:
				sky-arrive 0.7s cubic-bezier(0.22, 1, 0.36, 1) var(--d) backwards,
				sky-breathe var(--tw) ease-in-out calc(var(--d) + 0.7s) infinite;
		}
		.sky:not(.arrive) .ink {
			animation: sky-breathe var(--tw) ease-in-out infinite;
		}
		.ink.ongoing::after {
			animation: sky-pulse 2.8s ease-out infinite;
		}
		.tip,
		.bloom-head,
		.spokes {
			animation: sky-tip 160ms ease-out;
		}
		.petal button {
			animation: sky-petal 260ms cubic-bezier(0.34, 1.56, 0.64, 1) var(--pd) backwards;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.sky,
		.dot,
		.ink,
		.now,
		.thread,
		.rung span {
			transition: none;
		}
	}

	@keyframes sky-arrive {
		from {
			opacity: 0;
			transform: scale(0.2) translateY(4px);
		}
	}
	/* Quick to dim and slow to return, so it breathes rather than blinks. */
	@keyframes sky-breathe {
		0%,
		100% {
			opacity: 1;
		}
		18% {
			opacity: 0.6;
		}
	}
	@keyframes sky-pulse {
		from {
			transform: scale(1);
			opacity: 1;
		}
		to {
			transform: scale(2.2);
			opacity: 0;
		}
	}
	@keyframes sky-petal {
		from {
			opacity: 0;
			transform: translate(var(--fx), var(--fy)) scale(0.4);
		}
	}
	@keyframes sky-tip {
		from {
			opacity: 0;
		}
	}
</style>
