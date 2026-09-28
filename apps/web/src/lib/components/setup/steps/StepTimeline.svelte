<!--
	Setup: your chapters.

	THE STEP'S ONE JOB is the structure of a life: its chapters, named, with
	rough edges. What each one was and what ended it is the interview's job,
	the next step, which starts from these and writes the person's words onto
	them (narrative_draft.rs, `words_for_drawn`). So this step asks for names
	and years and nothing else, and says rough is fine.

	The framing is McAdams' (the Life Story Interview opens the same way):
	your life as a book, and its contents page. About two to seven chapters
	is what people give; ten is the ceiling here, two the floor.

	The intro is about five seconds, three beats, one object:

	  a  "If your life were a book / What would its chapters be?"
	     the line draws itself, birth to now
	  b  "Where did your life turn?"  an example's chapters settle onto it,
	     a tick at each turn
	  c  "Now write yours"  the example's later chapters fold into one blank
	     stretch; what is left is exactly their starting line: Childhood, to
	     13, then the chapter that came next, unnamed

	and then the editor draws the same line in the same place, so the example
	visibly becomes theirs. It plays once per device; a return opens on the
	editor. Reduced motion shows the example still, with a button on.

	AGES, NOT YEARS, ARE THE TRUTH. Each boundary is stored as the age it
	falls at, so the birth year can be mistyped and fixed without moving or
	deleting a chapter (it once deleted every chapter before a typo'd year).
	People remember chapters by age; the line shows both.

	On a phone the line is a preview and the chapters are a list: a fingertip
	on a 300px line is three years wide, and names on it collide.

	WHAT A SAVE WRITES. The whole list, in one PUT: the first band starts on
	the birth date; every later band starts on January 1 of the year it
	begins (precision 'year'); the last runs to now. The box refuses the
	replace once the chapters have pages of their own, and says so.
-->
<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { fade } from 'svelte/transition';
	import {
		ApiError,
		getProfile,
		listLifeChapters,
		replaceLifeChapters,
		updateProfile,
		type DatePrecision,
		type LifeChapterInput,
	} from '$lib/api/client';
	import { M, rise, sink } from '../motion';
	import { readNextChapter, writeNextChapter } from '../nextChapter';
	import { setup } from '../setup.svelte';

	// `onskip` sets the step aside (Setup records it); without one, skipping
	// just moves on.
	let { onnext, onskip }: { onnext?: () => void; onskip?: () => void } = $props();

	const MIN = 2;
	const MAX = 10;
	const SEED_AGE = 13;
	const INTRO_KEY = 'virtues-timeline-intro';

	type Beat = 'a' | 'b' | 'c' | 'e';

	// ------------------------------------------------------------------
	// Motion
	// ------------------------------------------------------------------

	let reduced = $state(false);
	const touch = typeof window !== 'undefined' && !!window.matchMedia?.('(pointer: coarse)').matches;

	// ------------------------------------------------------------------
	// The example (beats a-c). Ages, 0 = birth. Fictional, and personal in
	// the way theirs will be: a school, a place, a person, a home. The last
	// one is named, never "Now", which is the line's end, not a chapter.
	// ------------------------------------------------------------------

	const SAMPLE_SPAN = 38;
	const SAMPLE: { title: string; from: number; to: number }[] = [
		{ title: 'Childhood', from: 0, to: SEED_AGE },
		{ title: 'The band years', from: SEED_AGE, to: 19 },
		{ title: 'Chicago', from: 19, to: 26 },
		{ title: 'Married', from: 26, to: 33 },
		{ title: 'The farm', from: 33, to: SAMPLE_SPAN },
	];

	let beat = $state<Beat | null>(null);
	let lineDrawn = $state(false);
	let shownBands = $state(0);
	let folded = $state(false);
	let editorShown = $state(false);
	/** Opened on chapters they saved before: no intro, and a different head. */
	let returning = $state(false);

	let timers: ReturnType<typeof setTimeout>[] = [];
	function later(fn: () => void, delay: number) {
		timers.push(setTimeout(fn, delay));
	}
	function clearTimers() {
		timers.forEach(clearTimeout);
		timers = [];
	}

	function enter(next: Beat) {
		clearTimers();
		beat = next;
		switch (next) {
			case 'a':
				later(() => (lineDrawn = true), 160);
				later(() => enter('b'), 1900);
				break;
			case 'b':
				lineDrawn = true;
				SAMPLE.forEach((_, i) => later(() => (shownBands = i + 1), 120 + i * 240));
				later(() => enter('c'), 3300);
				break;
			case 'c':
				lineDrawn = true;
				shownBands = SAMPLE.length;
				folded = true;
				later(() => enter('e'), 900);
				break;
			case 'e':
				lineDrawn = true;
				shownBands = SAMPLE.length;
				folded = true;
				editorShown = true;
				try {
					localStorage.setItem(INTRO_KEY, '1');
				} catch {
					// seen again next time; nothing lost
				}
				later(() => focusFirst(), M.base + 60);
				break;
		}
	}

	/** A click on the example moves it along one beat. */
	function advance() {
		if (reduced) enter('e');
		else if (beat === 'a') enter('b');
		else if (beat === 'b') enter('c');
		else if (beat === 'c') enter('e');
	}

	/** A tap anywhere during the intro moves it along (a click, so a scroll
	 *  that starts on the page doesn't), as a click on the
	 *  example does: on a phone the intro held the editor back about eight
	 *  seconds with nothing for a thumb to do but find "Skip intro". */
	function onIntroPointer(e: MouseEvent) {
		if (!beat || beat === 'e') return;
		if (e.target instanceof Element && e.target.closest('button, input, select, textarea, a')) return;
		advance();
	}

	$effect(() => () => clearTimers());

	// ------------------------------------------------------------------
	// Geometry
	// ------------------------------------------------------------------

	let width = $state(720);
	const PAD_L = 28;
	const PAD_R = 36;
	const BAND_H = 30;
	const inner = $derived(Math.max(1, width - PAD_L - PAD_R));
	/** A phone: the line is a preview, the chapters a list. */
	const listMode = $derived(width < 520);
	// The line starts high enough that the intro's first beat isn't a band
	// of empty paper; stacked names above it grow into the margin.
	const LINE_Y = $derived(listMode ? 64 : 108);
	const H = $derived(listMode ? 124 : 172);
	const labelPx = $derived(listMode ? 14 : 17);

	const sx = (age: number) => PAD_L + (age / SAMPLE_SPAN) * inner;
	/** The example's names alternate above and below the line, so neighbors
	 *  never meet; one that would run off the end is set against it. */
	const sampleLabel = $derived(
		SAMPLE.map((b, i) => {
			const x = sx(b.from) + 4;
			const est = b.title.length * labelPx * 0.5;
			const end = x + est > PAD_L + inner;
			return {
				x: end ? PAD_L + inner - 2 : x,
				anchor: end ? 'end' : 'start',
				y: i % 2 === 0 ? LINE_Y - BAND_H / 2 - 10 : LINE_Y + BAND_H / 2 + labelPx + 6,
			};
		}),
	);

	// ------------------------------------------------------------------
	// Their chapters (beat e)
	// ------------------------------------------------------------------

	const now = new Date();
	const thisYear = now.getFullYear();
	const nowFrac =
		thisYear +
		(now.getTime() - new Date(thisYear, 0, 1).getTime()) /
			(new Date(thisYear + 1, 0, 1).getTime() - new Date(thisYear, 0, 1).getTime());

	const MONTHS = [
		'January', 'February', 'March', 'April', 'May', 'June',
		'July', 'August', 'September', 'October', 'November', 'December',
	];

	let birthYearText = $state('');
	let birthMonth = $state(1);
	/** The profile's own date, kept verbatim when they leave it alone (it may
	 *  carry a real day, which a month picker would round off). */
	let storedBirth = $state<string | null>(null);

	const birthYear = $derived.by(() => {
		const y = Number(birthYearText);
		return /^\d{4}$/.test(birthYearText) && y >= 1900 && y <= thisYear ? y : null;
	});
	const birthKnown = $derived(birthYear !== null);
	/** A full four digits that isn't a birth year: said, not ignored. */
	const birthWrong = $derived(/^\d{4}$/.test(birthYearText) && birthYear === null);
	/** Until they give a year, the line is the example's own span, so the
	 *  hand-off from the example is exact. */
	const base = $derived(birthYear ?? Math.floor(nowFrac - SAMPLE_SPAN));
	const t0 = $derived(birthYear !== null ? birthYear + (birthMonth - 1) / 12 : nowFrac - SAMPLE_SPAN);
	const t1 = $derived(Math.max(nowFrac, t0 + 1));
	/** Their age this year: the last age a chapter can begin at. */
	const ageNow = $derived(thisYear - base);
	const tx = (t: number) => PAD_L + ((t - t0) / (t1 - t0)) * inner;
	const tAt = (x: number) => t0 + ((x - PAD_L) / inner) * (t1 - t0);

	interface Band {
		key: number;
		title: string;
	}
	let nextKey = 2;
	let bands = $state<Band[]>([
		{ key: 0, title: 'Childhood' },
		{ key: 1, title: '' },
	]);
	/** ages[k] = the age chapter k+1 begins at. Strictly increasing. */
	let ages = $state<number[]>([SEED_AGE]);
	let touched = $state(false);

	/** They all fit before now. A birth year typo can break this; it never
	 *  moves or deletes a chapter, it only stops the save and says why. */
	const fits = $derived(ages.every((a) => a >= 1 && a <= ageNow));
	/** Where each boundary is drawn: squeezed inside the line when they don't
	 *  fit, so nothing leaves the page while the year is wrong. */
	const shown = $derived(
		ages.map((a, k) => Math.max(k + 1, Math.min(a, ageNow - (ages.length - 1 - k)))),
	);
	const yearOf = (k: number) => base + shown[k];
	const bandStart = (i: number) => (i === 0 ? t0 : yearOf(i - 1));
	const bandEnd = (i: number) => (i === bands.length - 1 ? t1 : yearOf(i));

	function lo(k: number) {
		return k === 0 ? 1 : ages[k - 1] + 1;
	}
	function hi(k: number) {
		return k === ages.length - 1 ? ageNow : ages[k + 1] - 1;
	}
	/** Band i can take a new boundary at age a. */
	function canSplit(i: number, a: number) {
		if (bands.length >= MAX || !fits) return false;
		const l = i === 0 ? 1 : ages[i - 1] + 1;
		const h = i === bands.length - 1 ? ageNow : ages[i] - 1;
		return a >= l && a <= h;
	}
	const canAdd = $derived(
		bands.length < MAX && fits && bands.some((_, i) => {
			const l = i === 0 ? 1 : ages[i - 1] + 1;
			const h = i === bands.length - 1 ? ageNow : ages[i] - 1;
			return h >= l;
		}),
	);

	function split(i: number, a: number, then: 'name' | 'year' = 'name') {
		if (!canSplit(i, a)) return;
		const band: Band = { key: nextKey++, title: '' };
		bands.splice(i + 1, 0, band);
		ages.splice(i, 0, a);
		touched = true;
		saveError = null;
		tick().then(() => {
			if (then === 'year') openYear(i);
			else nameEls.get(band.key)?.focus();
		});
	}

	/** "What came next": a new chapter after the last, placed at a guess the
	 *  person corrects straight away (its year opens for them). */
	function addNext() {
		const last = bands.length - 1;
		const from = last === 0 ? 1 : ages[last - 1] + 1;
		const guess = Math.max(from, Math.round((from - 1 + ageNow) / 2));
		if (canSplit(last, guess)) {
			split(last, guess, 'year');
			return;
		}
		// The last chapter is a single year: the widest one gives way.
		let best = -1;
		let span = 0;
		bands.forEach((_, i) => {
			const l = i === 0 ? 1 : ages[i - 1] + 1;
			const h = i === bands.length - 1 ? ageNow : ages[i] - 1;
			if (h - l + 1 > span) {
				span = h - l + 1;
				best = i;
			}
		});
		if (best < 0) return;
		const l = best === 0 ? 1 : ages[best - 1] + 1;
		const h = best === bands.length - 1 ? ageNow : ages[best] - 1;
		split(best, Math.round((l + h) / 2), 'year');
	}

	// Undo: one step, for the two things that take away (a remove and a join).
	let undo = $state<{ bands: Band[]; ages: number[]; said: string; key: number } | null>(null);
	let undoTimer: ReturnType<typeof setTimeout> | null = null;
	function keep(said: string, key: number) {
		undo = { bands: bands.map((b) => ({ ...b })), ages: [...ages], said, key };
		if (undoTimer) clearTimeout(undoTimer);
		undoTimer = setTimeout(() => (undo = null), 8000);
	}
	function restore() {
		if (!undo) return;
		const key = undo.key;
		bands = undo.bands;
		ages = undo.ages;
		undo = null;
		tick().then(() => nameEls.get(key)?.focus());
	}
	$effect(() => () => {
		if (undoTimer) clearTimeout(undoTimer);
	});

	const titleOf = (b: Band | undefined) => (b?.title.trim() ? `“${b.title.trim()}”` : 'the unnamed chapter');

	/** Remove band i; its years go to the chapter before it (the first
	 *  chapter's go to the one after, since birth stays put). */
	function remove(i: number) {
		if (bands.length <= 1) return;
		const heir = bands[i === 0 ? 1 : i - 1];
		const gone = bands[i];
		const said = gone.title.trim()
			? `Removed ${titleOf(gone)}. Its years went to ${titleOf(heir)}.`
			: `Removed a chapter. Its years went to ${titleOf(heir)}.`;
		keep(said, gone.key);
		bands.splice(i, 1);
		ages.splice(i === 0 ? 0 : i - 1, 1);
		nameEls.delete(gone.key);
		touched = true;
		tick().then(() => nameEls.get(heir.key)?.focus());
	}

	/** Join band k+1 into band k (a boundary removed). */
	function join(k: number) {
		const left = bands[k];
		const right = bands[k + 1];
		keep(`Joined ${titleOf(right)} into ${titleOf(left)}.`, right.key);
		if (!left.title.trim()) left.title = right.title;
		bands.splice(k + 1, 1);
		ages.splice(k, 1);
		touched = true;
		tick().then(() => bandEls.get(bands[k].key)?.focus());
	}

	function clampAge(k: number, a: number) {
		return Math.min(hi(k), Math.max(lo(k), a));
	}

	// Drag a boundary
	let stageEl = $state<HTMLDivElement | null>(null);
	let dragging = $state<number | null>(null);
	let hoverTick = $state<number | null>(null);
	function xOf(e: MouseEvent) {
		const r = stageEl?.getBoundingClientRect();
		return r ? e.clientX - r.left : 0;
	}
	function onHandleDown(e: PointerEvent, k: number) {
		(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
		(e.currentTarget as HTMLElement).focus();
		dragging = k;
		e.preventDefault();
	}
	function onHandleMove(e: PointerEvent, k: number) {
		if (dragging !== k || !fits) return;
		const a = clampAge(k, Math.round(tAt(xOf(e))) - base);
		if (a !== ages[k]) {
			ages[k] = a;
			touched = true;
		}
	}
	function onHandleKey(e: KeyboardEvent, k: number) {
		const step = e.shiftKey ? 5 : 1;
		if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') ages[k] = clampAge(k, ages[k] - step);
		else if (e.key === 'ArrowRight' || e.key === 'ArrowUp') ages[k] = clampAge(k, ages[k] + step);
		else if (e.key === 'Home') ages[k] = clampAge(k, -Infinity);
		else if (e.key === 'End') ages[k] = clampAge(k, Infinity);
		else if (e.key === 'Enter') openYear(k);
		else if (e.key === 'Delete' || e.key === 'Backspace') join(k);
		else return;
		touched = true;
		e.preventDefault();
	}

	// A boundary's year, entered rather than dragged: a four-digit year, or an
	// age ("18").
	let yearOpen = $state<number | null>(null);
	let yearText = $state('');
	let yearEl = $state<HTMLInputElement | null>(null);
	async function openYear(k: number) {
		if (!birthKnown) {
			nudgeBirth();
			return;
		}
		yearOpen = k;
		yearText = String(yearOf(k));
		await tick();
		yearEl?.focus();
		yearEl?.select();
	}
	function commitYear(k: number, then?: 'next') {
		const n = Number(yearText.trim());
		if (Number.isInteger(n) && yearText.trim()) {
			const a = n >= 1000 ? n - base : n;
			ages[k] = clampAge(k, a);
			touched = true;
		}
		yearOpen = null;
		if (then === 'next') tick().then(() => nameEls.get(bands[k + 1]?.key)?.focus());
	}
	function onYearKey(e: KeyboardEvent, k: number) {
		if (e.key === 'Enter') {
			commitYear(k, 'next');
			e.preventDefault();
		} else if (e.key === 'Escape') {
			yearOpen = null;
			tick().then(() => handleEls.get(k)?.focus());
			e.preventDefault();
		}
	}

	// Clicking a band adds a boundary where you clicked
	let ghost = $state<{ i: number; a: number } | null>(null);
	function onBandMove(e: PointerEvent, i: number) {
		if (!birthKnown) return;
		const a = Math.round(tAt(xOf(e))) - base;
		ghost = canSplit(i, a) ? { i, a } : null;
	}
	function onBandClick(e: MouseEvent, i: number) {
		// A keyboard "click" (Enter/Space) has no position; rename instead.
		if (e.detail === 0) return;
		if (!birthKnown) {
			nudgeBirth();
			return;
		}
		const a = Math.round(tAt(xOf(e))) - base;
		if (canSplit(i, a)) {
			split(i, a);
			ghost = null;
		} else {
			nameEls.get(bands[i].key)?.focus();
			if (bands.length >= MAX) hint = 'Ten chapters is the most. Join two to make room for another.';
		}
	}
	function onBandKey(e: KeyboardEvent, i: number) {
		if (e.key === 'Enter' || e.key === ' ') {
			nameEls.get(bands[i].key)?.focus();
			nameEls.get(bands[i].key)?.select();
			e.preventDefault();
		} else if (e.key === '+' || e.key === '=') {
			const l = i === 0 ? 1 : ages[i - 1] + 1;
			const h = i === bands.length - 1 ? ageNow : ages[i] - 1;
			split(i, Math.round((l + h) / 2), 'year');
			e.preventDefault();
		} else if ((e.key === 'Delete' || e.key === 'Backspace') && bands.length > 1) {
			remove(i);
			e.preventDefault();
		}
	}
	/** Enter in a name moves on: to the next chapter, or, on the last one,
	 *  to a new one ("what came next"). */
	function onNameKey(e: KeyboardEvent, i: number) {
		if (e.key === 'Enter') {
			e.preventDefault();
			if (!bands[i].title.trim()) return;
			if (i < bands.length - 1) nameEls.get(bands[i + 1].key)?.focus();
			else if (canAdd) addNext();
			else (e.currentTarget as HTMLInputElement).blur();
		} else if (e.key === 'Escape') {
			(e.currentTarget as HTMLInputElement).blur();
			e.preventDefault();
		}
	}

	const nameEls = new Map<number, HTMLInputElement>();
	const bandEls = new Map<number, HTMLElement>();
	const handleEls = new Map<number, HTMLElement>();
	function registerName(el: HTMLInputElement, key: number) {
		nameEls.set(key, el);
		return { destroy: () => { if (nameEls.get(key) === el) nameEls.delete(key); } };
	}
	function registerBand(el: HTMLElement, key: number) {
		bandEls.set(key, el);
		return { destroy: () => { if (bandEls.get(key) === el) bandEls.delete(key); } };
	}
	function registerHandle(el: HTMLElement, k: number) {
		handleEls.set(k, el);
		return {
			update: (nk: number) => { handleEls.set(nk, el); },
			destroy: () => { if (handleEls.get(k) === el) handleEls.delete(k); },
		};
	}
	let birthYearEl = $state<HTMLInputElement | null>(null);
	function focusFirst() {
		if (birthYear === null) {
			birthYearEl?.focus({ preventScroll: true });
			return;
		}
		const empty = bands.find((b) => !b.title.trim());
		if (empty) nameEls.get(empty.key)?.focus({ preventScroll: true });
	}

	/** Anything that needs the year, pressed before there is one, points at
	 *  the year instead of doing nothing. */
	let nudged = $state(false);
	function nudgeBirth() {
		nudged = false;
		tick().then(() => (nudged = true));
		birthYearEl?.focus();
		hint = null;
	}

	/** Names stand above the line, stacking up a row when a chapter is too
	 *  narrow to hold its own. */
	const NARROW = 104;
	const placeholderOf = (i: number) =>
		i === 0 ? 'Your first chapter' : i === 1 ? 'What came next?' : 'Name this chapter';
	const layout = $derived.by(() => {
		let narrowRun = 0;
		return bands.map((b, i) => {
			const x0 = tx(bandStart(i));
			const x1 = tx(bandEnd(i));
			const w = Math.max(0, x1 - x0);
			let row = 0;
			if (w < NARROW) {
				row = 1 + (narrowRun % 2);
				narrowRun++;
			} else {
				narrowRun = 0;
			}
			// A narrow band hard against the right edge writes its name leftward,
			// so a focused name never runs off the pane.
			const alignRight = w < 150 && x1 > width - 150;
			const chars = (b.title.trim() || placeholderOf(i)).length;
			return { x0, x1, w, row, alignRight, chars };
		});
	});

	/** Which boundary years have room to be printed: one that would overprint
	 *  its neighbor, or the end of the line, stays quiet until it is held. */
	const YEAR_GAP = 64;
	const yearRoom = $derived.by(() => {
		let last = -Infinity;
		const endX = PAD_L + inner;
		return shown.map((a) => {
			const x = tx(base + a);
			const ok = x - last >= YEAR_GAP && endX - x >= YEAR_GAP;
			if (ok) last = x;
			return ok;
		});
	});

	/** The last chapter, empty, after every other is named: what Enter on the
	 *  last name adds, so a person who pressed Enter to finish left one
	 *  behind. It doesn't hold up saving, and saving drops it (its years go
	 *  back to the chapter before). */
	const dangling = $derived(
		bands.length > MIN &&
			!bands[bands.length - 1].title.trim() &&
			bands.slice(0, -1).every((b) => b.title.trim()),
	);
	const unnamed = $derived(bands.filter((b) => !b.title.trim()).length - (dangling ? 1 : 0));
	const canSave = $derived(birthKnown && unnamed === 0 && bands.length >= MIN && fits);

	/** A passing word from the editor: a refused split, a limit. */
	let hint = $state<string | null>(null);

	/** One instruction at a time, under the headline. */
	const instruction = $derived.by(() => {
		if (!birthKnown) return 'Start with the year you were born.';
		if (bands.length < 3 && !returning)
			return `${touch && !listMode ? 'Tap' : listMode ? 'Add' : 'Click'} ${listMode ? 'the chapters that followed' : 'the line where your life turned'}. Three to seven chapters is plenty, and rough years are fine.`;
		return 'Three to seven chapters is plenty, and rough years are fine.';
	});

	/** The line under the buttons, most pressing first. */
	const status = $derived.by(() => {
		if (saveError) return { error: true, text: saveError };
		if (birthWrong) return { error: true, text: `Enter the year you were born, 1900 to ${thisYear}.` };
		if (birthKnown && !fits)
			return { error: true, text: 'These chapters run past this year. Check the year you were born.' };
		if (confirmSkip) return { error: false, text: 'Skipping leaves out the chapters you drew here.' };
		if (hint) return { error: false, text: hint };
		// Before a year, the line under the headline already asks for it.
		if (!birthKnown) return null;
		if (bands.length < MIN) return { error: false, text: 'Add one more chapter to save.' };
		if (unnamed > 0)
			return {
				error: false,
				text: `${unnamed === 1 ? 'One chapter needs' : `${unnamed} chapters need`} a name before you can save.`,
			};
		// Ready: say what they are for, next. The interview starts from them
		// and asks this (prompt.rs, drawn chapters are not played back).
		if (canSave)
			return {
				error: false,
				text: `Next, ${setup.assistantName} asks what each one was, and what ended it.`,
			};
		return null;
	});
	$effect(() => {
		// A hint is for the moment it answers; the next edit clears it.
		void bands.length;
		void ages.length;
		hint = null;
	});

	// ------------------------------------------------------------------
	// Load and save
	// ------------------------------------------------------------------

	let nextChapter = $state('');

	onMount(() => {
		const mq = window.matchMedia('(prefers-reduced-motion: reduce)');
		reduced = mq.matches;
		nextChapter = readNextChapter();
		let seen = false;
		try {
			seen = localStorage.getItem(INTRO_KEY) === '1';
		} catch {
			// no storage: the intro plays, which is the safe default
		}
		let cancelled = false;
		(async () => {
			const [profile, chapters] = await Promise.all([
				getProfile().catch(() => null),
				listLifeChapters().catch(() => []),
			]);
			if (cancelled) return;
			const bd = profile?.birth_date ?? null;
			if (bd && /^\d{4}-\d{2}-\d{2}$/.test(bd)) {
				storedBirth = bd;
				birthYearText = bd.slice(0, 4);
				birthMonth = Number(bd.slice(5, 7)) || 1;
			}
			if (chapters.length) {
				// They have drawn this before: open on theirs, not the example.
				const start = Number(chapters[0].started_at.slice(0, 4));
				const from = birthYear ?? start;
				if (birthYear === null) birthYearText = String(start);
				bands = chapters.map((c) => ({ key: nextKey++, title: c.title ?? '' }));
				ages = chapters.slice(1).map((c) => Number(c.started_at.slice(0, 4)) - from);
				returning = chapters.some((c) => c.title);
			}
			if (returning || seen) enter('e');
			else if (reduced) {
				// Still: the whole example at once, and a button on.
				beat = 'a';
				lineDrawn = true;
				shownBands = SAMPLE.length;
			} else enter('a');
		})();
		return () => {
			cancelled = true;
		};
	});

	let saving = $state(false);
	let saveError = $state<string | null>(null);
	let confirmSkip = $state(false);

	function sentence(msg: string) {
		const s = msg.trim();
		if (!s) return s;
		const cap = s[0].toUpperCase() + s.slice(1);
		return /[.?]$/.test(cap) ? cap : `${cap}.`;
	}

	async function save() {
		if (!canSave || birthYear === null || saving) return;
		saving = true;
		saveError = null;
		try {
			const mm = String(birthMonth).padStart(2, '0');
			const keepStored = storedBirth?.startsWith(`${birthYear}-${mm}-`) ?? false;
			const birthDate = keepStored ? (storedBirth as string) : `${birthYear}-${mm}-01`;
			const birthPrecision: DatePrecision = keepStored ? 'day' : 'month';
			if (!keepStored) {
				await updateProfile({ birth_date: birthDate });
				storedBirth = birthDate;
			}
			if (dangling) {
				bands.pop();
				ages.pop();
			}
			const last = bands.length - 1;
			const payload: LifeChapterInput[] = bands.map((b, i) => ({
				title: b.title.trim(),
				started_at: i === 0 ? birthDate : `${birthYear + ages[i - 1]}-01-01`,
				started_precision: i === 0 ? birthPrecision : 'year',
				ended_at: i === last ? null : `${birthYear + ages[i]}-01-01`,
				ended_precision: i === last ? null : 'year',
			}));
			await replaceLifeChapters(payload);
			writeNextChapter(nextChapter);
			onnext?.();
		} catch (e) {
			saveError =
				e instanceof ApiError && e.status === 400
					? sentence(e.message)
					: "Your server couldn't save your chapters. Check your connection and try again.";
		} finally {
			saving = false;
		}
	}

	function skip() {
		if (touched && !confirmSkip) {
			confirmSkip = true;
			return;
		}
		(onskip ?? onnext)?.();
	}

	const spanLabel = (i: number) => {
		const from = i === 0 ? (birthYear ?? 'birth') : yearOf(i - 1);
		const to = i === bands.length - 1 ? 'now' : yearOf(i);
		return `${from} to ${to}`;
	};
	/** "2004 · 13" on the line (people place chapters by age), "2004 · age
	 *  13" when held, just the age before there is a year. */
	const tickLabel = (k: number, full: boolean) =>
		birthKnown ? `${yearOf(k)} · ${full ? 'age ' : ''}${shown[k]}` : `age ${shown[k]}`;

	function onWindowKey(e: KeyboardEvent) {
		if (beat && beat !== 'e' && e.key === 'Escape') {
			enter('e');
			return;
		}
		// ⌘Z/Ctrl+Z undoes a remove, unless a field has its own undo to do.
		const el = document.activeElement;
		const field = el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement;
		if (undo && !field && (e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'z' && !e.shiftKey) {
			e.preventDefault();
			restore();
		}
	}

	const heads: Record<Beat, { h: string; s: string }> = {
		a: { h: 'If your life were a book', s: 'What would its chapters be?' },
		b: { h: 'Where did your life turn?', s: 'A move, a school, a person, a loss. Each turn begins a chapter.' },
		c: { h: 'Now write yours', s: '' },
		e: { h: 'Now write yours', s: '' },
	};
	/** Two chapters named: the instruction is followed, and the heading
	 *  becomes the thing made rather than the ask. */
	const settled = $derived(touched && bands.filter((b) => b.title.trim()).length >= 2);
	const head = $derived(
		beat === 'e' && (returning || settled)
			? { h: 'Your chapters', s: '' }
			: beat
				? heads[beat]
				: { h: '', s: '' },
	);
	const sub = $derived(beat === 'e' ? instruction : head.s);
</script>

<svelte:window onkeydown={onWindowKey} onclick={onIntroPointer} />

<section class="timeline-step" class:editing={beat === 'e'} class:list={listMode}>
	<header class="head">
		<div class="slot headline-slot" aria-live="polite">
			{#key head.h}
				<h1 class="headline" in:rise={{ delay: M.quick }} out:sink>{head.h}</h1>
			{/key}
		</div>
		<div class="slot sub-slot">
			{#key sub}
				<p class="sub" in:rise={{ delay: M.quick + 80, y: 4 }} out:sink>{sub}</p>
			{/key}
		</div>
	</header>

	<div class="stage" bind:this={stageEl} bind:clientWidth={width} style:height="{H}px">
		{#if beat && beat !== 'e'}
			<button type="button" class="advance" aria-label="Continue" onclick={advance}></button>
		{/if}

		<!-- The example, beats a-c -->
		<svg class="intro" class:gone={editorShown} {width} height={H} viewBox="0 0 {width} {H}" aria-hidden="true">
			<text class="eyebrow" class:show={shownBands > 0 && !folded} x={width / 2} y={Math.max(12, LINE_Y - 76)} text-anchor="middle">
				An example
			</text>
			{#each SAMPLE as band, i (band.title)}
				<g class="sband" class:show={i < shownBands} class:folded={folded && i > 0} style:--i={i}>
					<rect
						x={sx(band.from) + 1}
						y={LINE_Y - BAND_H / 2}
						width={Math.max(0, sx(band.to) - sx(band.from) - 2)}
						height={BAND_H}
						rx="3"
						class:alt={i % 2 === 1}
					/>
					{#if i > 0}
						<line class="sturn" x1={sx(band.from)} x2={sx(band.from)} y1={LINE_Y - BAND_H / 2 - 6} y2={LINE_Y + BAND_H / 2 + 6} />
					{/if}
					<text
						class="slabel"
						style:font-size="{labelPx}px"
						x={sampleLabel[i].x}
						y={sampleLabel[i].y}
						text-anchor={sampleLabel[i].anchor}
					>
						{band.title}
					</text>
				</g>
			{/each}
			<!-- What the example folds into: their first chapter and the blank one after -->
			<rect
				class="seed-rest"
				class:show={folded}
				x={sx(SEED_AGE) + 1}
				y={LINE_Y - BAND_H / 2}
				width={Math.max(0, sx(SAMPLE_SPAN) - sx(SEED_AGE) - 2)}
				height={BAND_H}
				rx="3"
			/>
			<line class="sturn keep" class:show={shownBands > 1} x1={sx(SEED_AGE)} x2={sx(SEED_AGE)} y1={LINE_Y - BAND_H / 2 - 6} y2={LINE_Y + BAND_H / 2 + 6} />

			<rect class="line" class:drawn={lineDrawn} x={PAD_L} y={LINE_Y - 0.75} width={inner} height="1.5" />
			<circle class="origin" cx={PAD_L} cy={LINE_Y} r="5" />
			<path
				class="arrow"
				class:show={lineDrawn}
				d="M {PAD_L + inner - 1} {LINE_Y - 6} L {PAD_L + inner + 9} {LINE_Y} L {PAD_L + inner - 1} {LINE_Y + 6}"
			/>
			<text class="end" class:show={lineDrawn} x={PAD_L} y={LINE_Y + 48}>Birth</text>
			<text class="end" class:show={lineDrawn} x={PAD_L + inner + 8} y={LINE_Y + 48} text-anchor="end">Now</text>
		</svg>

		<!-- Theirs, beat e -->
		{#if editorShown}
			<div class="editor" class:unborn={!birthKnown} in:fade={{ duration: reduced ? 0 : M.base }}>
				<svg class="axis" {width} height={H} viewBox="0 0 {width} {H}" aria-hidden="true">
					{#each bands as band, i (band.key)}
						{@const l = layout[i]}
						<rect
							class="band"
							class:alt={i % 2 === 1}
							class:empty={!band.title.trim()}
							x={l.x0 + 1}
							y={LINE_Y - BAND_H / 2}
							width={Math.max(0, l.w - 2)}
							height={BAND_H}
							rx="3"
						/>
						{#if l.row > 0 && !listMode}
							<line
								class="leader"
								x1={l.alignRight ? l.x1 - 4 : l.x0 + 4}
								x2={l.alignRight ? l.x1 - 4 : l.x0 + 4}
								y1={LINE_Y - BAND_H / 2 - 4}
								y2={LINE_Y - BAND_H / 2 - 12 - l.row * 30 + 6}
							/>
						{/if}
						{#if listMode}
							<text class="band-n" x={l.x0 + l.w / 2} y={LINE_Y - 4} text-anchor="middle">{l.w > 14 ? i + 1 : ''}</text>
						{/if}
					{/each}
					<rect class="line drawn" x={PAD_L} y={LINE_Y - 0.75} width={inner} height="1.5" />
					<circle class="origin" cx={PAD_L} cy={LINE_Y} r="5" />
					<path
						class="arrow show"
						d="M {PAD_L + inner - 1} {LINE_Y - 6} L {PAD_L + inner + 9} {LINE_Y} L {PAD_L + inner - 1} {LINE_Y + 6}"
					/>
					{#if ghost && dragging === null}
						<line class="ghost" x1={tx(base + ghost.a)} x2={tx(base + ghost.a)} y1={LINE_Y - BAND_H / 2 - 6} y2={LINE_Y + BAND_H / 2 + 6} />
						<text class="year ghost-year" x={tx(base + ghost.a)} y={LINE_Y + 48} text-anchor="middle">
							+ {base + ghost.a} · age {ghost.a}
						</text>
					{/if}
					<text class="year end-year" x={PAD_L + inner + 8} y={LINE_Y + 48} text-anchor="end">Now</text>
				</svg>

				{#if !listMode}

					{#each bands as band, i (band.key)}
						{@const l = layout[i]}
						<button
							type="button"
							class="band-hit"
							use:registerBand={band.key}
							style:left="{l.x0}px"
							style:width="{Math.max(0, l.w)}px"
							style:top="{LINE_Y - BAND_H / 2 - 4}px"
							style:height="{BAND_H + 8}px"
							aria-label="{band.title.trim() || 'Unnamed chapter'}, {spanLabel(i)}"
							aria-keyshortcuts="Enter Plus Delete"
							onpointermove={(e) => onBandMove(e, i)}
							onpointerleave={() => (ghost = null)}
							onclick={(e) => onBandClick(e, i)}
							onkeydown={(e) => onBandKey(e, i)}
						></button>

						<div
							class="label"
							class:right={l.alignRight}
							style:left={l.alignRight ? undefined : `${l.x0 + 3}px`}
							style:right={l.alignRight ? `${width - l.x1 + 3}px` : undefined}
							style:top="{LINE_Y - BAND_H / 2 - 40 - l.row * 30}px"
							style:--w="{l.row > 0 ? Math.max(l.w - 8, Math.min(220, 24 + l.chars * 9)) : Math.max(64, l.w - 32)}px"
						>
							<input
								type="text"
								maxlength="120"
								placeholder={placeholderOf(i)}
								aria-label="Name of the chapter from {spanLabel(i)}"
								bind:value={band.title}
								use:registerName={band.key}
								onkeydown={(e) => onNameKey(e, i)}
								oninput={() => {
									saveError = null;
									touched = true;
									confirmSkip = false;
								}}
							/>
							{#if bands.length > 1}
								<button
									type="button"
									class="remove"
									aria-label="Remove {band.title.trim() || 'this chapter'}"
									onclick={() => remove(i)}
								>
									<svg viewBox="0 0 10 10" width="9" height="9" aria-hidden="true">
										<path d="M1.5 1.5 L8.5 8.5 M8.5 1.5 L1.5 8.5" />
									</svg>
								</button>
							{/if}
						</div>
					{/each}

					{#each ages as _, k (bands[k + 1]?.key ?? k)}
						{@const x = tx(yearOf(k))}
						<div
							class="handle"
							class:active={dragging === k}
							role="slider"
							tabindex="0"
							use:registerHandle={k}
							aria-label="Start of {bands[k + 1]?.title.trim() || 'the next chapter'}. Enter to type a year."
							aria-valuenow={yearOf(k)}
							aria-valuemin={base + lo(k)}
							aria-valuemax={base + hi(k)}
							aria-valuetext={tickLabel(k, true)}
							style:left="{x}px"
							style:top="{LINE_Y - BAND_H / 2 - 8}px"
							style:height="{BAND_H + 16}px"
							onpointerdown={(e) => onHandleDown(e, k)}
							onpointermove={(e) => onHandleMove(e, k)}
							onpointerup={() => (dragging = null)}
							onpointercancel={() => (dragging = null)}
							onpointerenter={() => (hoverTick = k)}
							onpointerleave={() => (hoverTick = null)}
							onkeydown={(e) => onHandleKey(e, k)}
						>
							<span class="tick" aria-hidden="true"></span>
						</div>
						{#if yearOpen === k}
							<input
								class="year-input"
								style:left="{x}px"
								style:top="{LINE_Y + 30}px"
								type="text"
								inputmode="numeric"
								maxlength="4"
								aria-label="Year {bands[k + 1]?.title.trim() || 'the next chapter'} began, or your age then"
								bind:this={yearEl}
								bind:value={yearText}
								onkeydown={(e) => onYearKey(e, k)}
								onblur={() => yearOpen === k && commitYear(k)}
							/>
						{:else}
							{@const full = dragging === k || hoverTick === k}
							<button
								type="button"
								class="year-btn"
								class:active={full}
								class:quiet-year={!full && !yearRoom[k] && ghost === null}
								class:hidden={ghost !== null && dragging === null && Math.abs(tx(base + ghost.a) - x) < 60}
								style:left="{x}px"
								style:top="{LINE_Y + 32}px"
								tabindex="-1"
								onclick={() => openYear(k)}
							>
								{tickLabel(k, full)}
							</button>
						{/if}
					{/each}
				{/if}
			</div>
		{/if}
	</div>

	{#if editorShown && !listMode}
		<!-- ONE ROW, ON THE CENTER LINE. Born sat left under the line, the add
		     button right, and the next chapter centered: three alignments on
		     one screen of a flow that has one (2026-09-28). -->
		<div class="under" in:rise={{ delay: M.quick }}>
			<fieldset class="birth inline" class:nudged>
				<legend>Born</legend>
				<span class="month"><select bind:value={birthMonth} aria-label="Birth month">
					{#each MONTHS as m, i}
						<option value={i + 1}>{m}</option>
					{/each}
				</select></span>
				<input
					class="birth-year"
					type="text"
					inputmode="numeric"
					maxlength="4"
					placeholder="Year"
					aria-label="Birth year"
					aria-invalid={birthWrong}
					bind:this={birthYearEl}
					bind:value={birthYearText}
					oninput={() => {
						saveError = null;
						nudged = false;
					}}
				/>
			</fieldset>
			<span class="sep" aria-hidden="true">·</span>
			<button
				type="button"
				class="quiet add-inline"
				onclick={() => (birthKnown ? addNext() : nudgeBirth())}
				disabled={birthKnown && !canAdd}
			>
				+ Add a chapter
			</button>
		</div>
	{/if}

	{#if editorShown && listMode}
		<!-- The phone's editor: one row per chapter, the line above as its map -->
		<div class="rows" in:rise={{ delay: M.quick }}>
			<fieldset class="birth birth-row" class:nudged>
				<legend>Born</legend>
				<span class="month"><select bind:value={birthMonth} aria-label="Birth month">
					{#each MONTHS as m, i}
						<option value={i + 1}>{m}</option>
					{/each}
				</select></span>
				<input
					class="birth-year"
					type="text"
					inputmode="numeric"
					maxlength="4"
					placeholder="Year"
					aria-label="Birth year"
					aria-invalid={birthWrong}
					bind:this={birthYearEl}
					bind:value={birthYearText}
					oninput={() => {
						saveError = null;
						nudged = false;
					}}
				/>
			</fieldset>
			<ol class="chapter-list">
				{#each bands as band, i (band.key)}
					<li class="row" class:empty={!band.title.trim()}>
						<span class="n" aria-hidden="true">{i + 1}</span>
						<input
							class="row-name"
							type="text"
							maxlength="120"
							placeholder={placeholderOf(i)}
							aria-label="Name of chapter {i + 1}, {spanLabel(i)}"
							bind:value={band.title}
							use:registerName={band.key}
							onkeydown={(e) => onNameKey(e, i)}
							oninput={() => {
								saveError = null;
								touched = true;
								confirmSkip = false;
							}}
						/>
						{#if i === 0}
							<span class="row-from">{birthKnown ? `from ${birthYear}` : 'from birth'}</span>
						{:else if yearOpen === i - 1}
							<input
								class="row-year"
								type="text"
								inputmode="numeric"
								maxlength="4"
								aria-label="Year chapter {i + 1} began, or your age then"
								bind:this={yearEl}
								bind:value={yearText}
								onkeydown={(e) => onYearKey(e, i - 1)}
								onblur={() => yearOpen === i - 1 && commitYear(i - 1)}
							/>
						{:else}
							<button type="button" class="row-from row-year-btn" onclick={() => openYear(i - 1)}>
								from {birthKnown ? yearOf(i - 1) : `age ${shown[i - 1]}`}
							</button>
						{/if}
						{#if bands.length > 1}
							<button
								type="button"
								class="row-remove"
								aria-label="Remove {band.title.trim() || `chapter ${i + 1}`}"
								onclick={() => remove(i)}
							>
								<svg viewBox="0 0 10 10" width="10" height="10" aria-hidden="true">
									<path d="M1.5 1.5 L8.5 8.5 M8.5 1.5 L1.5 8.5" />
								</svg>
							</button>
						{/if}
					</li>
				{/each}
			</ol>
			<button
				type="button"
				class="quiet add-row"
				onclick={() => (birthKnown ? addNext() : nudgeBirth())}
				disabled={birthKnown && !canAdd}
			>
				+ Add a chapter
			</button>
		</div>
	{/if}

	{#if editorShown && birthKnown && bands.length >= MIN && bands.every((b) => b.title.trim())}
		<!-- The one line that looks forward. Not until the drawn chapters are
		     named: beside an empty one it read as the place to name it. -->
		<label class="next" in:rise={{ delay: M.base }}>
			<span>What do you hope the next chapter is?</span>
			<input
				type="text"
				maxlength="120"
				placeholder="Optional"
				bind:value={nextChapter}
				onkeydown={(e) => e.key === 'Enter' && canSave && save()}
			/>
		</label>
	{/if}

	<footer class="foot">
		{#if beat && beat !== 'e'}
			{#if reduced}
				<button type="button" class="primary" onclick={() => enter('e')}>Write yours</button>
			{:else}
				<button type="button" class="quiet" onclick={() => enter('e')}>Skip intro</button>
			{/if}
		{:else if editorShown}
			<div class="actions" in:rise={{ delay: M.base + 80 }}>
				<button type="button" class="primary" onclick={save} disabled={!canSave || saving}>
					{saving ? 'Saving…' : 'Save my chapters'}
				</button>
				<button type="button" class="quiet" onclick={skip}>
					{confirmSkip ? 'Skip anyway' : 'Skip for now'}
				</button>
			</div>
			<p class="status" role={status?.error ? 'alert' : 'status'}>
				{#if undo}
					<span>{undo.said}</span>
					<button type="button" class="link" onclick={restore}>Undo</button>
				{:else if status}
					<span class:error={status.error}>{status.text}</span>
				{:else}
					&nbsp;
				{/if}
			</p>
		{/if}
	</footer>
</section>

<style>
	.timeline-step {
		display: flex;
		flex-direction: column;
		min-height: 100%;
		width: 100%;
		max-width: 44rem;
		margin: 0 auto;
		padding: clamp(40px, 8vh, 88px) 16px 64px;
		box-sizing: border-box;
		color: var(--color-foreground);
	}

	/* ---------- head ---------- */
	/* On the center line, like every step (setup.css, one alignment). Both
	   lines hold their height, so a change of words never moves the line. */
	.head {
		text-align: center;
	}
	.slot {
		display: grid;
	}
	.slot > :global(*) {
		grid-area: 1 / 1;
	}
	.headline {
		margin: 0;
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: clamp(32px, 4vw, 44px);
		line-height: 1.08;
		letter-spacing: -0.015em;
		color: var(--color-foreground);
		text-wrap: balance;
	}
	.headline-slot {
		min-height: 1.1em;
		font-size: clamp(32px, 4vw, 44px);
	}
	.sub-slot {
		min-height: 48px;
		margin-top: 12px;
	}
	.sub {
		margin: 0 auto;
		max-width: 32em;
		font-family: var(--font-sans);
		font-size: 15px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
		text-wrap: balance;
	}

	/* ---------- stage ---------- */
	.stage {
		position: relative;
		width: 100%;
		margin-top: clamp(24px, 6vh, 56px);
	}
	.list .stage {
		margin-top: 16px;
	}
	.stage {
		overflow-x: clip;
		user-select: none;
		-webkit-user-select: none;
	}
	.advance {
		position: absolute;
		inset: 0;
		z-index: 3;
		margin: 0;
		padding: 0;
		border: 0;
		background: transparent;
		cursor: pointer;
	}
	.advance:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 8px;
		border-radius: 6px;
	}
	.stage > svg {
		position: absolute;
		inset: 0;
		overflow: visible;
	}

	/* the example */
	.intro {
		transition: opacity var(--m-base) var(--m-ease);
	}
	.intro.gone {
		opacity: 0;
	}
	.line {
		fill: var(--color-primary);
		transform-box: fill-box;
		transform-origin: left center;
		transform: scaleX(0);
		transition: transform 900ms var(--m-ease);
	}
	.line.drawn {
		transform: scaleX(1);
	}
	.origin {
		fill: var(--color-primary);
	}
	.arrow {
		fill: none;
		stroke: var(--color-primary);
		stroke-width: 1.5;
		stroke-linecap: round;
		stroke-linejoin: round;
		opacity: 0;
		transition: opacity var(--m-base) var(--m-ease) 700ms;
	}
	.arrow.show {
		opacity: 1;
	}
	.editor .arrow {
		transition: none;
	}

	.sband rect,
	.band {
		fill: color-mix(in srgb, var(--color-primary) 13%, transparent);
	}
	.sband rect.alt,
	.band.alt {
		fill: color-mix(in srgb, var(--color-primary) 22%, transparent);
	}
	.sband {
		opacity: 0;
		transition: opacity var(--m-base) var(--m-ease);
	}
	.sband.show {
		opacity: 1;
	}
	.sband rect {
		transform-box: fill-box;
		transform-origin: left center;
		transform: scaleX(0.3);
		transition: transform var(--m-slow) var(--m-spring);
	}
	.sband.show rect {
		transform: scaleX(1);
	}
	.slabel {
		font-family: var(--font-serif-ui, var(--font-serif));
		font-weight: 400;
		fill: var(--color-foreground);
		opacity: 0;
		transform: translateY(6px);
		transition:
			opacity var(--m-slow) var(--m-ease) 80ms,
			transform var(--m-slow) var(--m-ease) 80ms;
	}
	.sband.show .slabel {
		opacity: 1;
		transform: none;
	}
	.sturn {
		stroke: var(--color-primary);
		stroke-width: 1.5;
	}
	/* The later chapters fold away into one blank stretch: names first,
	   then fills, left to right. */
	.sband.folded .slabel {
		opacity: 0;
		transform: translateY(-4px);
		transition:
			opacity var(--m-quick) linear calc(var(--i) * 40ms),
			transform var(--m-quick) linear calc(var(--i) * 40ms);
	}
	.sband.folded rect,
	.sband.folded .sturn {
		opacity: 0;
		transition: opacity var(--m-base) var(--m-ease) calc(120ms + var(--i) * 60ms);
	}
	.sturn.keep {
		opacity: 0;
		transition: opacity var(--m-base) var(--m-ease);
	}
	.sturn.keep.show {
		opacity: 1;
	}
	.seed-rest,
	.band.empty {
		fill: color-mix(in srgb, var(--color-primary) 6%, transparent);
		stroke: color-mix(in srgb, var(--color-primary) 40%, transparent);
		stroke-width: 1;
		stroke-dasharray: 3 3;
	}
	.seed-rest {
		opacity: 0;
		transition: opacity var(--m-slow) var(--m-ease) 200ms;
	}
	.seed-rest.show {
		opacity: 1;
	}
	.eyebrow,
	.end {
		font-family: var(--font-sans);
		font-size: 12px;
		letter-spacing: 0.04em;
		fill: var(--color-foreground-muted);
		opacity: 0;
		transition: opacity var(--m-base) var(--m-ease);
	}
	.end {
		transition-delay: 500ms;
	}
	.eyebrow.show,
	.end.show {
		opacity: 1;
	}

	/* ---------- their chapters ---------- */
	.editor {
		position: absolute;
		inset: 0;
	}
	.band {
		transition: fill var(--m-base) ease;
	}
	.band-n {
		font-family: var(--font-sans);
		font-size: 11px;
		font-variant-numeric: tabular-nums;
		fill: var(--color-foreground-muted);
	}
	.leader {
		stroke: color-mix(in srgb, var(--color-primary) 40%, transparent);
		stroke-width: 1;
	}
	.ghost {
		stroke: var(--color-primary);
		stroke-width: 1;
		stroke-dasharray: 2 3;
		opacity: 0.8;
	}
	.year {
		font-family: var(--font-sans);
		font-size: 12px;
		font-variant-numeric: tabular-nums;
		fill: var(--color-foreground-muted);
	}
	.ghost-year {
		fill: var(--color-primary);
	}

	.band-hit {
		position: absolute;
		margin: 0;
		padding: 0;
		border: 0;
		background: transparent;
		cursor: copy;
		border-radius: 6px;
	}
	.editor.unborn .band-hit {
		cursor: pointer;
	}
	.band-hit:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 1px;
	}

	.label {
		position: absolute;
		display: flex;
		align-items: center;
		gap: 0;
	}
	.label.right {
		flex-direction: row-reverse;
	}
	.label.right input {
		text-align: right;
	}
	.label input {
		width: var(--w);
		max-width: 22ch;
		min-width: 0;
		margin: 0;
		padding: 0 0 4px;
		border: 0;
		border-bottom: 1px solid transparent;
		border-radius: 0;
		/* Clear: the stage's grain runs under a name, so any fill reads as a
		   box. A stacked name's leader rises beside the names, not through. */
		background: transparent;
		font-family: var(--font-serif-ui, var(--font-serif));
		font-weight: 400;
		font-size: 17px;
		line-height: 1.2;
		color: var(--color-foreground);
		text-overflow: ellipsis;
		outline: none;
		transition:
			border-color var(--m-quick) ease,
			width var(--m-quick) ease;
	}
	.label input::placeholder,
	.row-name::placeholder,
	.next input::placeholder {
		color: var(--color-foreground-subtle, var(--color-foreground-muted));
	}
	.label input:hover {
		border-bottom-color: var(--color-border);
	}
	.label input:focus {
		width: max(var(--w), 16ch);
		border-bottom-color: var(--color-primary);
		position: relative;
		z-index: 2;
	}
	.remove {
		display: grid;
		place-items: center;
		flex: none;
		width: 24px;
		height: 24px;
		padding: 0;
		border: 0;
		border-radius: 50%;
		background: transparent;
		color: var(--color-foreground-muted);
		cursor: pointer;
		opacity: 0;
		transition: opacity var(--m-quick) ease;
	}
	.remove path,
	.row-remove path {
		stroke: currentColor;
		stroke-width: 1.3;
		stroke-linecap: round;
		fill: none;
	}
	.label:hover .remove,
	.label:focus-within .remove,
	.remove:focus-visible {
		opacity: 1;
	}
	.remove:hover {
		color: var(--color-foreground);
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
	}

	.handle {
		position: absolute;
		width: 24px;
		margin: 0 0 0 -12px;
		padding: 0;
		border: 0;
		background: transparent;
		cursor: ew-resize;
		touch-action: none;
		z-index: 1;
	}
	.tick {
		position: absolute;
		left: 50%;
		top: 0;
		bottom: 0;
		width: 1.5px;
		background: var(--color-primary);
	}
	.tick::before {
		content: '';
		position: absolute;
		left: 50%;
		top: -3px;
		width: 8px;
		height: 8px;
		margin-left: -4px;
		border-radius: 50%;
		background: var(--color-surface);
		border: 1.5px solid var(--color-primary);
		box-sizing: border-box;
		transition:
			transform var(--m-quick) ease,
			background var(--m-quick) ease;
	}
	.handle:hover .tick::before,
	.handle.active .tick::before,
	.handle:focus-visible .tick::before {
		transform: scale(1.35);
		background: var(--color-primary);
	}
	.handle:focus-visible {
		outline: none;
	}
	.handle:focus-visible .tick {
		outline: 3px solid color-mix(in srgb, var(--color-primary) 25%, transparent);
	}

	.year-btn,
	.year-input {
		position: absolute;
		transform: translateX(-50%);
		font-family: var(--font-sans);
		font-size: 12px;
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}
	.year-btn {
		padding: 4px 6px;
		border: 0;
		border-radius: 6px;
		background: transparent;
		color: var(--color-foreground-muted);
		cursor: text;
		transition:
			opacity var(--m-quick) ease,
			color var(--m-quick) ease;
	}
	.year-btn:hover,
	.year-btn.active {
		color: var(--color-primary);
	}
	.year-btn.quiet-year,
	.year-btn.hidden {
		opacity: 0;
	}
	.year-btn.quiet-year:hover {
		opacity: 1;
	}
	.year-input {
		width: 6ch;
		padding: 4px 0;
		border: 0;
		border-bottom: 1px solid var(--color-primary);
		border-radius: 0;
		background: var(--color-surface);
		color: var(--color-foreground);
		text-align: center;
		outline: none;
		z-index: 3;
	}

	.birth {
		position: absolute;
		display: flex;
		align-items: baseline;
		gap: 6px;
		margin: 0;
		padding: 0;
		border: 0;
	}
	.birth legend {
		float: left;
		padding: 0;
		margin-right: 4px;
		font-family: var(--font-sans);
		font-size: 12px;
		letter-spacing: 0.04em;
		color: var(--color-foreground-muted);
	}
	.birth select,
	.birth input {
		margin: 0;
		padding: 0 0 4px;
		border: 0;
		border-bottom: 1px solid var(--color-border);
		border-radius: 0;
		background: transparent;
		font-family: var(--font-serif-ui, var(--font-serif));
		font-weight: 400;
		font-size: 17px;
		color: var(--color-foreground);
		outline: none;
		appearance: none;
		-webkit-appearance: none;
	}
	.birth select {
		padding-right: 16px;
		cursor: pointer;
	}
	/* A menu should look like one: a small chevron in the ink of the field */
	.month {
		position: relative;
	}
	.month::after {
		content: '';
		position: absolute;
		right: 4px;
		top: 50%;
		width: 5px;
		height: 5px;
		margin-top: -6px;
		border-right: 1.5px solid var(--color-foreground-muted);
		border-bottom: 1.5px solid var(--color-foreground-muted);
		transform: rotate(45deg);
		pointer-events: none;
	}
	.birth-year {
		width: 4.2ch;
		font-variant-numeric: tabular-nums;
	}
	.birth select:focus,
	.birth input:focus,
	.birth-year:placeholder-shown {
		border-bottom-color: var(--color-primary);
	}
	.birth-year[aria-invalid='true'] {
		border-bottom-color: var(--color-error, var(--color-primary));
	}
	/* Pointed at, when something needed the year first */
	.birth.nudged .birth-year {
		animation: nudge var(--m-slow) var(--m-ease);
	}
	@keyframes nudge {
		20% {
			transform: translateX(-4px);
		}
		40% {
			transform: translateX(4px);
		}
		60% {
			transform: translateX(-2px);
		}
		80% {
			transform: translateX(2px);
		}
	}

	/* ---------- the phone's rows ---------- */
	.rows {
		margin-top: 8px;
	}
	.birth-row {
		position: static;
		justify-content: center;
		margin-bottom: 16px;
	}
	.chapter-list {
		margin: 0;
		padding: 0;
		list-style: none;
		border-top: 1px solid var(--color-border);
	}
	.row {
		display: flex;
		align-items: center;
		gap: 8px;
		min-height: 48px;
		border-bottom: 1px solid var(--color-border);
	}
	.n {
		width: 16px;
		flex: none;
		font-family: var(--font-sans);
		font-size: 12px;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground-muted);
	}
	.row-name {
		flex: 1;
		min-width: 0;
		margin: 0;
		padding: 12px 0;
		border: 0;
		background: transparent;
		font-family: var(--font-serif-ui, var(--font-serif));
		font-size: 17px;
		color: var(--color-foreground);
		outline: none;
	}
	.row-from {
		flex: none;
		font-family: var(--font-sans);
		font-size: 13px;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground-muted);
	}
	.row-year-btn {
		min-height: 44px;
		padding: 0 4px;
		border: 0;
		background: transparent;
		color: var(--color-primary);
		cursor: pointer;
	}
	.row-year {
		width: 6ch;
		padding: 8px 0;
		border: 0;
		border-bottom: 1px solid var(--color-primary);
		background: transparent;
		font-family: var(--font-sans);
		font-size: 16px;
		text-align: center;
		color: var(--color-foreground);
		outline: none;
	}
	.row-remove {
		display: grid;
		place-items: center;
		flex: none;
		width: 44px;
		height: 44px;
		margin-right: -12px;
		padding: 0;
		border: 0;
		background: transparent;
		color: var(--color-foreground-muted);
		cursor: pointer;
	}
	.add-row {
		display: block;
		min-height: 44px;
		margin: 4px auto 0;
		color: var(--color-primary);
	}

	/* ---------- the next chapter ---------- */
	.next {
		display: flex;
		align-items: baseline;
		justify-content: center;
		gap: 12px;
		margin: 24px auto 0;
		font-family: var(--font-sans);
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	.next input {
		width: min(16em, 50vw);
		padding: 0 0 4px;
		border: 0;
		border-bottom: 1px solid var(--color-border);
		border-radius: 0;
		background: transparent;
		font-family: var(--font-serif-ui, var(--font-serif));
		font-size: 17px;
		color: var(--color-foreground);
		outline: none;
	}
	.next input:focus {
		border-bottom-color: var(--color-primary);
	}
	.list .next {
		flex-direction: column;
		align-items: center;
		gap: 4px;
	}
	.list .next input {
		width: 100%;
		text-align: center;
	}

	/* ---------- footer ---------- */
	.foot {
		display: flex;
		flex-direction: column;
		align-items: center;
		margin-top: auto;
		padding-top: 28px;
		min-height: 88px;
	}
	.actions {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 16px;
		flex-wrap: wrap;
		width: 100%;
	}
	/* Quiet controls keep a 44-point hit area without taking the room. */
	.quiet,
	.link {
		position: relative;
	}
	.quiet:not(.add-row)::after,
	.link::after {
		content: "";
		position: absolute;
		inset: -8px -4px;
	}
	.quiet {
		padding: 6px 0;
		border: 0;
		background: transparent;
		font-family: var(--font-sans);
		font-size: 14px;
		color: var(--color-foreground-muted);
		cursor: pointer;
		transition: color var(--m-quick) ease;
	}
	.quiet:hover:not(:disabled) {
		color: var(--color-foreground);
	}
	.quiet:disabled {
		opacity: 0.45;
		cursor: default;
	}
	.quiet:focus-visible,
	.primary:focus-visible,
	.link:focus-visible,
	.year-btn:focus-visible,
	.row-year-btn:focus-visible,
	.row-remove:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 3px;
		border-radius: 6px;
	}
	.under {
		display: flex;
		align-items: baseline;
		justify-content: center;
		flex-wrap: wrap;
		gap: 12px;
		margin-top: 12px;
	}
	.birth.inline {
		position: static;
	}
	.sep {
		color: var(--color-foreground-subtle, var(--color-foreground-muted));
	}
	.add-inline {
		color: var(--color-primary);
	}
	.add-inline:hover:not(:disabled) {
		color: var(--color-primary);
		text-decoration: underline;
		text-underline-offset: 4px;
	}
	.primary {
		padding: 8px 20px;
		border: 0;
		border-radius: 999px;
		background: var(--color-primary);
		color: var(--color-background);
		font-family: var(--font-sans);
		font-size: 14px;
		cursor: pointer;
		transition:
			background var(--m-quick) ease,
			opacity var(--m-quick) ease;
	}
	.primary:hover:not(:disabled) {
		background: var(--color-primary-hover, var(--color-primary));
	}
	.primary:disabled {
		opacity: 0.4;
		cursor: default;
	}
	.status {
		display: flex;
		align-items: baseline;
		justify-content: center;
		gap: 8px;
		margin: 12px 0 0;
		min-height: 1.4em;
		text-align: center;
		font-family: var(--font-sans);
		font-size: 13px;
		color: var(--color-foreground-muted);
		text-wrap: balance;
	}
	.error {
		color: var(--color-error, var(--color-foreground));
	}
	.link {
		padding: 0;
		border: 0;
		background: none;
		font: inherit;
		color: var(--color-primary);
		cursor: pointer;
		text-decoration: underline;
		text-underline-offset: 3px;
	}

	@media (max-width: 560px) {
		/* On a phone the one filled button takes its own full-width row under
		   the quiet one. */
		.actions .primary {
			order: -1;
			flex-basis: 100%;
			padding: 12px 20px;
			font-size: 15px;
		}
		.actions .quiet {
			min-height: 44px;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.timeline-step *,
		.timeline-step *::before {
			transition: none !important;
			animation: none !important;
		}
	}
</style>
