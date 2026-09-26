<!--
	Getting started, last step: build your timeline.

	One continuous experience in five beats, from the designer's step map
	("6 · Your story"):

	  a  "Imagine your life as a timeline."  a dot draws itself into an arrow
	  b  example chapters settle onto it one by one, the camera following
	  c  the camera pulls back: the whole example, birth to now
	  d  "Now build yours."  the hand-off
	  e  the example labels wipe away and the line becomes theirs: birth at
	     the left edge (this is the ONE place onboarding asks for it), bands
	     they add by clicking the line, names written on the band, boundaries
	     dragged or nudged a year at a time

	a-d advance on their own timing, about ten seconds in all (tightened
	from sixteen, 2026-09-25), or on a click; "Skip intro" jumps to e.

	The example is plainly an example ("An example" rides above it, and its
	chapters are the generic ones most lives share). It never pretends to be
	them; it shows the shape they are about to draw.

	WHAT A SAVE WRITES. The whole list, in one PUT: the timeline owns every
	chapter while it is open. The first band starts on the birth date; every
	later band starts on January 1 of its year (precision 'year', which is
	what the drag snaps to); the last runs to now (ended_at null). The box
	refuses the replace once the chapters have pages of their own, and says
	so; that message is shown beside the button as-is.
-->
<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { blur } from 'svelte/transition';
	import {
		ApiError,
		getProfile,
		listLifeChapters,
		replaceLifeChapters,
		updateProfile,
		type DatePrecision,
		type LifeChapterInput,
	} from '$lib/api/client';

	// `onskip` sets the step aside (Setup records it); without one, skipping
	// just moves on.
	let { onnext, onskip }: { onnext?: () => void; onskip?: () => void } = $props();

	type Beat = 'a' | 'b' | 'c' | 'd' | 'e';

	// ------------------------------------------------------------------
	// Motion
	// ------------------------------------------------------------------

	let reduced = $state(false);
	onMount(() => {
		const mq = window.matchMedia('(prefers-reduced-motion: reduce)');
		reduced = mq.matches;
		const on = (e: MediaQueryListEvent) => (reduced = e.matches);
		mq.addEventListener('change', on);
		return () => mq.removeEventListener('change', on);
	});
	const ms = (n: number) => (reduced ? 0 : n);
	const touch = typeof window !== 'undefined' && !!window.matchMedia?.('(pointer: coarse)').matches;

	// ------------------------------------------------------------------
	// The example (beats a-d). Abstract years, 0 = birth.
	// ------------------------------------------------------------------

	const SAMPLE_SPAN = 40;
	const SAMPLE: { title: string; from: number; to: number }[] = [
		{ title: 'Childhood', from: 0, to: 13 },
		{ title: 'High school', from: 13, to: 18 },
		{ title: 'College', from: 18, to: 22 },
		{ title: 'First job', from: 22, to: 27 },
		{ title: 'A new city', from: 27, to: 34 },
		{ title: 'Now', from: 34, to: SAMPLE_SPAN },
	];

	const HEADLINES: Record<Beat, string> = {
		a: 'Imagine your life as a timeline',
		b: 'Each stretch of it has a name',
		c: 'Seen whole, it has a shape',
		d: 'Now build yours',
		e: 'Now build yours',
	};

	let beat = $state<Beat>('a');
	let lineDrawn = $state(false);
	let shownBands = $state(0);
	let wiped = $state(false);
	let editorShown = $state(false);

	let timers: ReturnType<typeof setTimeout>[] = [];
	function later(fn: () => void, delay: number) {
		timers.push(setTimeout(fn, delay));
	}
	function clearTimers() {
		timers.forEach(clearTimeout);
		timers = [];
	}

	/** Enter a beat and schedule its own choreography and the next beat. */
	function enter(next: Beat) {
		clearTimers();
		beat = next;
		switch (next) {
			case 'a':
				lineDrawn = false;
				shownBands = 0;
				later(() => (lineDrawn = true), ms(500));
				later(() => enter('b'), reduced ? 2000 : 2400);
				break;
			case 'b':
				lineDrawn = true;
				shownBands = 0;
				SAMPLE.forEach((_, i) => later(() => (shownBands = i + 1), ms(400) + i * (reduced ? 300 : 480)));
				later(() => enter('c'), (reduced ? 300 : 480) * SAMPLE.length + 900);
				break;
			case 'c':
				lineDrawn = true;
				shownBands = SAMPLE.length;
				later(() => enter('d'), 2000);
				break;
			case 'd':
				lineDrawn = true;
				shownBands = SAMPLE.length;
				later(() => enter('e'), 1800);
				break;
			case 'e':
				lineDrawn = true;
				shownBands = SAMPLE.length;
				wiped = true;
				later(() => (editorShown = true), ms(900));
				later(() => focusFirstEmpty(), ms(1400));
				break;
		}
	}

	/** A click on the stage during the intro moves it along one beat. */
	function advance() {
		if (beat === 'a') enter('b');
		else if (beat === 'b') enter('c');
		else if (beat === 'c') enter('d');
		else if (beat === 'd') enter('e');
	}

	function skipIntro() {
		enter('e');
	}

	$effect(() => () => clearTimers());

	// ------------------------------------------------------------------
	// Geometry
	// ------------------------------------------------------------------

	let width = $state(720);
	const PAD_L = 28;
	const PAD_R = 36;
	const H = 300;
	const LINE_Y = 176;
	const BAND_H = 30;
	const inner = $derived(Math.max(1, width - PAD_L - PAD_R));

	/** Camera for the example: zoomed onto the newest band while they
	 *  arrive (b), pulled back to the whole line from c on. */
	const CAM_ZOOM = 1.9;
	const camera = $derived.by(() => {
		if (beat !== 'b' || shownBands === 0) return { s: 1, tx: 0 };
		const band = SAMPLE[Math.min(shownBands, SAMPLE.length) - 1];
		const focus = PAD_L + ((band.from + band.to) / 2 / SAMPLE_SPAN) * inner;
		const tx = Math.min(0, Math.max(width - width * CAM_ZOOM, width * 0.55 - focus * CAM_ZOOM));
		return { s: CAM_ZOOM, tx };
	});
	const sx = (y: number) => PAD_L + (y / SAMPLE_SPAN) * inner;

	/** The example's labels, in rows so a short chapter's name never runs
	 *  into the next one's ("High schoolCollege" at the whole view). Row 0
	 *  stands above the line; a label that would collide drops BELOW it, on
	 *  a leader, because a leader rising past the row above struck through
	 *  the neighbor's name. Widths are estimated from the 17px serif; the
	 *  rows hold at any zoom, because the labels scale with the camera. */
	const LABEL_ROW = 22;
	const sampleRow = $derived.by(() => {
		const ends: number[] = [];
		return SAMPLE.map((b) => {
			const x = sx(b.from) + 4;
			let r = ends.findIndex((e) => x >= e + 10);
			if (r < 0) {
				r = ends.length;
				ends.push(0);
			}
			ends[r] = x + b.title.length * 8.6;
			return r;
		});
	});
	/** A label the camera has panned past the left edge is hidden, rather
	 *  than showing its last letters ("ool"). */
	const offstage = (from: number) => camera.tx + (sx(from) + 4) * camera.s < 0;
	/** Baseline of a sample label: above the band on row 0, under it after. */
	const sampleY = (row: number) =>
		row === 0 ? LINE_Y - BAND_H / 2 - 12 : LINE_Y + BAND_H / 2 + 18 + (row - 1) * LABEL_ROW;
	const sampleRows = $derived(Math.max(0, ...sampleRow));

	// ------------------------------------------------------------------
	// Their timeline (beat e)
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
		return Number.isInteger(y) && y >= 1900 && y <= thisYear ? y : null;
	});
	const birthKnown = $derived(birthYear !== null);
	const t0 = $derived(birthYear !== null ? birthYear + (birthMonth - 1) / 12 : thisYear - 30);
	const t1 = $derived(Math.max(nowFrac, t0 + 1));
	const tx = (t: number) => PAD_L + ((t - t0) / (t1 - t0)) * inner;
	const tAt = (x: number) => t0 + ((x - PAD_L) / inner) * (t1 - t0);

	interface Band {
		key: number;
		title: string;
	}
	let nextKey = 1;
	let bands = $state<Band[]>([{ key: 0, title: '' }]);
	/** bounds[k] = the year band k+1 starts. Always strictly increasing. */
	let bounds = $state<number[]>([]);

	const firstYear = $derived(Math.floor(t0) + 1);
	const bandStart = (i: number) => (i === 0 ? t0 : bounds[i - 1]);
	const bandEnd = (i: number) => (i === bands.length - 1 ? t1 : bounds[i]);

	/** A new boundary can go at year y inside band i. */
	function canSplit(i: number, y: number) {
		const lo = i === 0 ? firstYear : bounds[i - 1] + 1;
		const hi = i === bands.length - 1 ? thisYear : bounds[i] - 1;
		return y >= lo && y <= hi;
	}

	function split(i: number, y: number) {
		if (!canSplit(i, y)) return;
		const band: Band = { key: nextKey++, title: '' };
		bands.splice(i + 1, 0, band);
		bounds.splice(i, 0, y);
		saveError = null;
		tick().then(() => labelEls.get(band.key)?.focus());
	}

	/** Remove band i; its years go to a neighbor. */
	function remove(i: number) {
		if (bands.length === 1) return;
		const key = bands[i].key;
		bands.splice(i, 1);
		// Band 0 owns the birth edge, so its end boundary goes; any other band
		// hands its years to the band before it.
		bounds.splice(i === 0 ? 0 : i - 1, 1);
		labelEls.delete(key);
		const focusTo = bands[Math.max(0, i - 1)];
		tick().then(() => bandEls.get(focusTo.key)?.focus());
	}

	/** The button's way to add a chapter: split the band that runs to now,
	 *  so chapters added one after another arrive in the order they were
	 *  lived. When that band is a single year, the widest band gives way. */
	function addChapter() {
		const last = bands.length - 1;
		const lastMid = Math.round((bandStart(last) + bandEnd(last)) / 2);
		if (canSplit(last, lastMid)) {
			split(last, lastMid);
			return;
		}
		if (canSplit(last, thisYear)) {
			split(last, thisYear);
			return;
		}
		let best = -1;
		let bestSpan = 0;
		bands.forEach((_, i) => {
			const lo = i === 0 ? firstYear : bounds[i - 1] + 1;
			const hi = i === bands.length - 1 ? thisYear : bounds[i] - 1;
			if (hi - lo + 1 > bestSpan) {
				bestSpan = hi - lo + 1;
				best = i;
			}
		});
		if (best < 0) return;
		const mid = Math.round((bandStart(best) + bandEnd(best)) / 2);
		const lo = best === 0 ? firstYear : bounds[best - 1] + 1;
		const hi = best === bands.length - 1 ? thisYear : bounds[best] - 1;
		split(best, Math.min(hi, Math.max(lo, mid)));
	}
	const canAdd = $derived(bands.some((_, i) => {
		const lo = i === 0 ? firstYear : bounds[i - 1] + 1;
		const hi = i === bands.length - 1 ? thisYear : bounds[i] - 1;
		return hi >= lo;
	}));

	/** Keep every boundary lawful after the birth edge moves: a boundary at
	 *  or before birth folds its band into the next. */
	function normalizeAfterBirth() {
		while (bounds.length && bounds[0] < firstYear) {
			bounds.splice(0, 1);
			const [gone] = bands.splice(0, 1);
			if (!bands[0].title.trim()) bands[0].title = gone.title;
		}
	}

	function boundClamp(k: number, y: number) {
		const lo = k === 0 ? firstYear : bounds[k - 1] + 1;
		const hi = k === bounds.length - 1 ? thisYear : bounds[k + 1] - 1;
		return Math.min(hi, Math.max(lo, y));
	}

	// Drag
	let stageEl = $state<HTMLDivElement | null>(null);
	let dragging = $state<number | null>(null);
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
		if (dragging !== k) return;
		bounds[k] = boundClamp(k, Math.round(tAt(xOf(e))));
	}
	function onHandleUp() {
		dragging = null;
	}
	function onHandleKey(e: KeyboardEvent, k: number) {
		const step = e.shiftKey ? 5 : 1;
		if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') {
			bounds[k] = boundClamp(k, bounds[k] - step);
		} else if (e.key === 'ArrowRight' || e.key === 'ArrowUp') {
			bounds[k] = boundClamp(k, bounds[k] + step);
		} else if (e.key === 'Home') {
			bounds[k] = boundClamp(k, -Infinity);
		} else if (e.key === 'End') {
			bounds[k] = boundClamp(k, Infinity);
		} else if (e.key === 'Delete' || e.key === 'Backspace') {
			// Removing a boundary joins the two bands; the earlier name stays.
			const right = bands[k + 1];
			if (!bands[k].title.trim()) bands[k].title = right.title;
			bands.splice(k + 1, 1);
			bounds.splice(k, 1);
			tick().then(() => bandEls.get(bands[k].key)?.focus());
		} else {
			return;
		}
		e.preventDefault();
	}

	// Clicking a band adds a boundary where you clicked
	let ghost = $state<{ i: number; y: number } | null>(null);
	function onBandMove(e: PointerEvent, i: number) {
		const y = Math.round(tAt(xOf(e)));
		ghost = canSplit(i, y) ? { i, y } : null;
	}
	function onBandClick(e: MouseEvent, i: number) {
		// A keyboard "click" (Enter/Space) has no position; rename instead.
		if (e.detail === 0) return;
		const y = Math.round(tAt(xOf(e)));
		if (canSplit(i, y)) {
			split(i, y);
			ghost = null;
		} else {
			labelEls.get(bands[i].key)?.focus();
		}
	}
	function onBandKey(e: KeyboardEvent, i: number) {
		if (e.key === 'Enter' || e.key === ' ') {
			labelEls.get(bands[i].key)?.focus();
			labelEls.get(bands[i].key)?.select();
			e.preventDefault();
		} else if (e.key === '+' || e.key === '=') {
			const mid = Math.round((bandStart(i) + bandEnd(i)) / 2);
			if (canSplit(i, mid)) split(i, mid);
			e.preventDefault();
		} else if ((e.key === 'Delete' || e.key === 'Backspace') && bands.length > 1) {
			remove(i);
			e.preventDefault();
		}
	}
	function onLabelKey(e: KeyboardEvent, i: number) {
		if (e.key === 'Enter' || e.key === 'Escape') {
			bandEls.get(bands[i].key)?.focus();
			e.preventDefault();
		}
	}

	const labelEls = new Map<number, HTMLInputElement>();
	const bandEls = new Map<number, HTMLElement>();
	function registerLabel(el: HTMLInputElement, key: number) {
		labelEls.set(key, el);
		return { destroy: () => { if (labelEls.get(key) === el) labelEls.delete(key); } };
	}
	function registerBand(el: HTMLElement, key: number) {
		bandEls.set(key, el);
		return { destroy: () => { if (bandEls.get(key) === el) bandEls.delete(key); } };
	}
	function focusFirstEmpty() {
		if (birthYear === null) {
			birthYearEl?.focus();
			return;
		}
		const empty = bands.find((b) => !b.title.trim());
		if (empty) labelEls.get(empty.key)?.focus();
	}
	let birthYearEl = $state<HTMLInputElement | null>(null);

	/** Labels stack into rows when bands are too narrow to hold them. */
	const NARROW = 104;
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
			// so a focused label never runs off the pane.
			const alignRight = w < 150 && x1 > width - 150;
			return { x0, x1, w, row, alignRight };
		});
	});

	/** THE EDITOR SITS UNDER ITS INSTRUCTION. The stage keeps the intro's
	 *  height, which the example's labels need; the editor's names stand on
	 *  the line, so without this the line sat ~300px below the sentence
	 *  telling you to click it. The stage rises by the room its label rows
	 *  don't use: a name on the first row sits 121px down the stage, so a
	 *  rise of 190 leaves it just under the sentence, and each extra row
	 *  gives back 30. */
	const editorPull = $derived(Math.max(0, 190 - Math.max(0, ...layout.map((l) => l.row)) * 30));

	/** Which boundary years have room to be printed: a year that would
	 *  overprint its neighbor, or "Now", stays silent until its mark is held. */
	const YEAR_GAP = 38;
	const yearRoom = $derived.by(() => {
		let last = -Infinity;
		const endX = PAD_L + inner;
		return bounds.map((y) => {
			const x = tx(y);
			const ok = x - last >= YEAR_GAP && endX - x >= YEAR_GAP;
			if (ok) last = x;
			return ok;
		});
	});

	const unnamed = $derived(bands.filter((b) => !b.title.trim()).length);
	const canSave = $derived(birthKnown && unnamed === 0);

	// ------------------------------------------------------------------
	// Load and save
	// ------------------------------------------------------------------

	onMount(() => {
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
			const named = chapters.filter((c) => c.kind === 'chapter' || c.title);
			if (chapters.length) {
				// They have drawn this before: open on their timeline, not the example.
				bands = chapters.map((c) => ({ key: nextKey++, title: c.title ?? '' }));
				bounds = chapters.slice(1).map((c) => Number(c.started_at.slice(0, 4)));
				if (birthYear !== null) normalizeAfterBirth();
				if (named.length) {
					enter('e');
					return;
				}
			}
			enter('a');
		})();
		return () => {
			cancelled = true;
		};
	});

	let saving = $state(false);
	let saveError = $state<string | null>(null);

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
			const last = bands.length - 1;
			const payload: LifeChapterInput[] = bands.map((b, i) => ({
				title: b.title.trim(),
				started_at: i === 0 ? birthDate : `${bounds[i - 1]}-01-01`,
				started_precision: i === 0 ? birthPrecision : 'year',
				ended_at: i === last ? null : `${bounds[i]}-01-01`,
				ended_precision: i === last ? null : 'year',
			}));
			await replaceLifeChapters(payload);
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

	const spanLabel = (i: number) => {
		const from = i === 0 ? (birthYear ?? '') : bounds[i - 1];
		const to = i === bands.length - 1 ? 'now' : bounds[i];
		return `${from} to ${to}`;
	};
</script>

<section class="timeline-step" class:editing={beat === 'e'}>
	<header class="head">
		<div class="headline-slot" aria-live="polite">
			{#key HEADLINES[beat]}
				<h1
					class="headline"
					in:blur={{ duration: ms(1100), amount: 10, delay: ms(150) }}
					out:blur={{ duration: ms(500), amount: 8 }}
				>
					{HEADLINES[beat]}
				</h1>
			{/key}
		</div>
		<!-- One instruction at a time: the year first, because nothing on the
		     line works without it; then the line. "Tap" on a touch screen. -->
		<p class="sub" class:show={editorShown}>
			{#if !birthKnown}
				Start with the year you were born.
			{:else}
				{touch ? 'Tap' : 'Click'} the line to add a chapter, then name it.
			{/if}
		</p>
	</header>

	<div
		class="stage"
		bind:this={stageEl}
		bind:clientWidth={width}
		style:height="{H}px"
		style:margin-top={editorShown ? `calc(clamp(8px, 4vh, 40px) - ${editorPull}px)` : undefined}
	>
		{#if beat !== 'e'}
			<!-- The whole stage moves the intro along a beat -->
			<button type="button" class="advance" aria-label="Continue" onclick={advance}></button>
		{/if}
		<!-- The example, beats a-d -->
		<svg
			class="intro"
			class:gone={editorShown}
			width={width}
			height={H}
			viewBox="0 0 {width} {H}"
			aria-hidden="true"
		>
			<text class="eyebrow" class:show={shownBands > 0 && !wiped} x={PAD_L} y={18}>
				An example
			</text>
			<g
				class="camera"
				style:transform="translate({camera.tx}px, 0) scale({camera.s})"
				style:transform-origin="0 {LINE_Y}px"
			>
				{#each SAMPLE as band, i (band.title)}
					<g class="sband" class:show={i < shownBands}>
						<rect
							x={sx(band.from) + 1}
							y={LINE_Y - BAND_H / 2}
							width={Math.max(0, sx(band.to) - sx(band.from) - 2)}
							height={BAND_H}
							rx="3"
							class:alt={i % 2 === 1}
						/>
						{#if sampleRow[i] > 0}
							<line
								class="sleader"
								class:wiped
								x1={sx(band.from) + 4}
								x2={sx(band.from) + 4}
								y1={LINE_Y + BAND_H / 2 + 3}
								y2={sampleY(sampleRow[i]) - 13}
							/>
						{/if}
						<text
							class="slabel"
							class:wiped
							class:offstage={beat === 'b' && offstage(band.from)}
							style:transition-delay="{wiped ? i * 90 : 0}ms"
							x={sx(band.from) + 4}
							y={sampleY(sampleRow[i])}
						>
							{band.title}
						</text>
					</g>
				{/each}

				<rect
					class="line"
					class:drawn={lineDrawn}
					x={PAD_L}
					y={LINE_Y - 0.75}
					width={inner}
					height="1.5"
				/>
				<circle class="origin" cx={PAD_L} cy={LINE_Y} r="5" />
				<path
					class="arrow"
					class:show={lineDrawn}
					d="M {PAD_L + inner - 1} {LINE_Y - 6} L {PAD_L + inner + 9} {LINE_Y} L {PAD_L + inner - 1} {LINE_Y + 6}"
				/>
				<text class="end" class:show={beat === 'c' || beat === 'd'} x={PAD_L} y={LINE_Y + 40 + sampleRows * LABEL_ROW}>Birth</text>
				<text
					class="end"
					class:show={beat === 'c' || beat === 'd'}
					x={PAD_L + inner + 8}
					y={LINE_Y + 40 + sampleRows * LABEL_ROW}
					text-anchor="end">Now</text
				>
			</g>
		</svg>

		<!-- Theirs, beat e -->
		{#if editorShown}
			<div
				class="editor"
				class:unborn={!birthKnown}
				in:blur={{ duration: ms(700), amount: 6 }}
			>
				<svg class="axis" width={width} height={H} viewBox="0 0 {width} {H}" aria-hidden="true">
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
						{#if l.row > 0}
							<line
								class="leader"
								x1={l.alignRight ? l.x1 - 4 : l.x0 + 4}
								x2={l.alignRight ? l.x1 - 4 : l.x0 + 4}
								y1={LINE_Y - BAND_H / 2 - 4}
								y2={LINE_Y - BAND_H / 2 - 12 - l.row * 30 + 6}
							/>
						{/if}
					{/each}
					<rect class="line drawn" x={PAD_L} y={LINE_Y - 0.75} width={inner} height="1.5" />
					<circle class="origin" cx={PAD_L} cy={LINE_Y} r="5" />
					<path
						class="arrow show"
						d="M {PAD_L + inner - 1} {LINE_Y - 6} L {PAD_L + inner + 9} {LINE_Y} L {PAD_L + inner - 1} {LINE_Y + 6}"
					/>
					{#if ghost && dragging === null}
						<line
							class="ghost"
							x1={tx(ghost.y)}
							x2={tx(ghost.y)}
							y1={LINE_Y - BAND_H / 2 - 6}
							y2={LINE_Y + BAND_H / 2 + 6}
						/>
						<text class="year ghost-year" x={tx(ghost.y)} y={LINE_Y + 42} text-anchor="middle">
							+ {ghost.y}
						</text>
					{/if}
					{#each bounds as y, k (bands[k + 1]?.key ?? k)}
						<text
							class="year"
							class:active={dragging === k}
							class:hidden={dragging !== k &&
								(!yearRoom[k] ||
									(ghost !== null && dragging === null && Math.abs(tx(ghost.y) - tx(y)) < 34))}
							x={tx(y)}
							y={LINE_Y + 42}
							text-anchor="middle">{y}</text
						>
					{/each}
					<text class="year end-year" x={PAD_L + inner + 8} y={LINE_Y + 42} text-anchor="end">Now</text>
				</svg>

				<!-- Birth, at the left edge: first in the tab order, as on the line -->
				<fieldset class="birth" style:left="{PAD_L - 6}px" style:top="{LINE_Y + 58}px">
					<legend>Born</legend>
					<select bind:value={birthMonth} aria-label="Birth month" onchange={normalizeAfterBirth}>
						{#each MONTHS as m, i}
							<option value={i + 1}>{m}</option>
						{/each}
					</select>
					<input
						class="birth-year"
						type="text"
						inputmode="numeric"
						maxlength="4"
						placeholder="Year"
						aria-label="Birth year"
						bind:this={birthYearEl}
						bind:value={birthYearText}
						oninput={() => {
							saveError = null;
							if (birthYear !== null) normalizeAfterBirth();
						}}
					/>
				</fieldset>

				<!-- Adding a chapter belongs with the line, opposite Born, not
				     with the way forward. -->
				{#if birthKnown}
					<button
						type="button"
						class="quiet add"
						style:right="{PAD_R - 8}px"
						style:top="{LINE_Y + 62}px"
						onclick={addChapter}
						disabled={!canAdd}
						in:blur={{ duration: ms(400), amount: 3 }}
					>
						+ Add a chapter
					</button>
				{/if}

				<!-- Per band, in tab order: the band (click to add a boundary there,
				     Enter to rename), its name, then the boundary that ends it
				     (drag, or arrow keys a year at a time). -->
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
						style:--w="{l.row > 0
							? Math.max(l.w - 8, Math.min(220, 24 + band.title.trim().length * 9))
							: Math.max(56, l.w - 8)}px"
					>
						<input
							type="text"
							maxlength="120"
							placeholder="Name it"
							aria-label="Name of the chapter from {spanLabel(i)}"
							bind:value={band.title}
							use:registerLabel={band.key}
							onkeydown={(e) => onLabelKey(e, i)}
							oninput={() => (saveError = null)}
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

					{#if i < bounds.length}
						{@const y = bounds[i]}
						<div
							class="handle"
							class:active={dragging === i}
							role="slider"
							tabindex="0"
							aria-label="Start of {bands[i + 1]?.title.trim() || 'the next chapter'}"
							aria-valuenow={y}
							aria-valuemin={i === 0 ? firstYear : bounds[i - 1] + 1}
							aria-valuemax={i === bounds.length - 1 ? thisYear : bounds[i + 1] - 1}
							aria-valuetext={String(y)}
							style:left="{tx(y)}px"
							style:top="{LINE_Y - BAND_H / 2 - 8}px"
							style:height="{BAND_H + 16}px"
							onpointerdown={(e) => onHandleDown(e, i)}
							onpointermove={(e) => onHandleMove(e, i)}
							onpointerup={onHandleUp}
							onpointercancel={onHandleUp}
							onkeydown={(e) => onHandleKey(e, i)}
						>
							<span class="tick" aria-hidden="true"></span>
						</div>
					{/if}
				{/each}
			</div>
		{/if}
	</div>

	<footer class="foot">
		{#if beat !== 'e'}
			<button type="button" class="quiet" onclick={skipIntro}>Skip intro</button>
		{:else if editorShown}
			<div class="actions" in:blur={{ duration: ms(600), amount: 4, delay: ms(200) }}>
				<button type="button" class="primary" onclick={save} disabled={!canSave || saving}>
					{saving ? 'Saving' : 'Save my chapters'}
				</button>
				<button type="button" class="quiet" onclick={() => (onskip ?? onnext)?.()}>Skip for now</button>
			</div>
			<p class="status" role={saveError ? 'alert' : undefined}>
				{#if saveError}
					<span class="error">{saveError}</span>
				{:else if unnamed > 0 && birthKnown}
					{unnamed === 1 ? 'One chapter needs' : `${unnamed} chapters need`} a name before you can save.
				{:else}
					&nbsp;
				{/if}
			</p>
		{/if}
	</footer>
</section>

<style>
	/* Setup's one column (StepFrame): every step starts at the same left
	   edge, so the line is the column's width rather than the window's. */
	.timeline-step {
		display: flex;
		flex-direction: column;
		min-height: 100%;
		width: 100%;
		max-width: 44rem;
		margin: 0 auto;
		padding: clamp(2.5rem, 8vh, 5.5rem) 16px 4rem;
		box-sizing: border-box;
		color: var(--color-foreground);
	}

	/* ---------- headline ---------- */
	/* On the center line, like every step (setup.css, one alignment). */
	.head {
		min-height: 150px;
		text-align: center;
	}
	.headline-slot {
		display: grid;
	}
	.headline {
		grid-area: 1 / 1;
		margin: 0;
		font-family: var(--font-serif);
		font-weight: 400;
		font-style: normal;
		font-size: clamp(2rem, 4vw, 2.75rem);
		line-height: 1.08;
		letter-spacing: -0.015em;
		color: var(--color-foreground);
		text-wrap: balance;
	}
	.sub {
		margin: 16px auto 0;
		max-width: 34em;
		font-family: var(--font-sans);
		font-size: 15px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
		opacity: 0;
		transition: opacity 700ms ease 300ms;
	}
	.sub.show {
		opacity: 1;
	}

	/* ---------- stage ---------- */
	.stage {
		position: relative;
		width: 100%;
		overflow-x: clip;
		margin-top: clamp(8px, 4vh, 40px);
		transition: margin-top 700ms cubic-bezier(0.3, 0.7, 0.2, 1);
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
	.stage > svg.intro {
		/* the camera zooms past the edges; the pane never scrolls sideways */
		overflow: hidden;
		transition: opacity 600ms ease;
	}
	.intro.gone {
		opacity: 0;
	}
	.camera {
		transition: transform 1400ms cubic-bezier(0.25, 0.7, 0.2, 1);
	}

	.line {
		fill: var(--color-primary);
		transform-box: fill-box;
		transform-origin: left center;
		transform: scaleX(0);
		transition: transform 2200ms cubic-bezier(0.45, 0, 0.2, 1);
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
		transition: opacity 500ms ease 2000ms;
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
		transition: opacity 900ms ease;
	}
	.sband.show {
		opacity: 1;
	}
	.sband rect {
		transform-box: fill-box;
		transform-origin: left center;
		transform: scaleX(0.4);
		transition: transform 900ms cubic-bezier(0.25, 0.7, 0.2, 1);
	}
	.sband.show rect {
		transform: scaleX(1);
	}
	.slabel {
		font-family: var(--font-serif-ui, var(--font-serif));
		font-weight: 400;
		font-size: 17px;
		fill: var(--color-foreground);
		clip-path: inset(0 0 0 0);
		transition:
			clip-path 700ms cubic-bezier(0.6, 0, 0.3, 1),
			opacity 700ms ease;
	}
	.slabel.wiped {
		clip-path: inset(0 0 0 100%);
		opacity: 0;
	}
	.slabel.offstage {
		opacity: 0;
	}
	.sleader {
		stroke: color-mix(in srgb, var(--color-foreground) 25%, transparent);
		stroke-width: 1;
		transition: opacity 700ms ease;
	}
	.sleader.wiped {
		opacity: 0;
	}
	.eyebrow,
	.end {
		font-family: var(--font-sans);
		font-size: 12px;
		letter-spacing: 0.04em;
		fill: var(--color-foreground-muted);
		opacity: 0;
		transition: opacity 700ms ease;
	}
	.eyebrow.show,
	.end.show {
		opacity: 1;
	}

	/* ---------- their timeline ---------- */
	.editor {
		position: absolute;
		inset: 0;
		transition: opacity 400ms ease;
	}
	.editor.unborn .band-hit,
	.editor.unborn .label,
	.editor.unborn .axis .band {
		opacity: 0.35;
		pointer-events: none;
	}
	.band {
		transition: fill 300ms ease;
	}
	.band.empty {
		fill: color-mix(in srgb, var(--color-primary) 7%, transparent);
		stroke: color-mix(in srgb, var(--color-primary) 35%, transparent);
		stroke-width: 1;
		stroke-dasharray: 3 3;
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
		transition: opacity 150ms ease;
	}
	.year.active,
	.ghost-year {
		fill: var(--color-primary);
	}
	.year.hidden {
		opacity: 0;
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
	.band-hit:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 1px;
	}

	.label.right {
		flex-direction: row-reverse;
	}
	.label.right input {
		text-align: right;
	}
	.label {
		position: absolute;
		display: flex;
		align-items: center;
		gap: 0;
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
		/* opaque, so a neighbor's leader line passes behind the name */
		background: var(--color-surface);
		font-family: var(--font-serif-ui, var(--font-serif));
		font-weight: 400;
		font-style: normal;
		font-size: 17px;
		line-height: 1.2;
		color: var(--color-foreground);
		text-overflow: ellipsis;
		outline: none;
		transition:
			border-color 200ms ease,
			width 200ms ease;
	}
	.label input::placeholder {
		color: var(--color-foreground-subtle, var(--color-foreground-muted));
	}
	.label input:hover {
		border-bottom-color: var(--color-border);
	}
	.label input:focus {
		width: max(var(--w), 14ch);
		border-bottom-color: var(--color-primary);
		position: relative;
		z-index: 2;
		/* the room's own paper, so a widened name covers its neighbor cleanly */
		background: var(--color-surface);
	}
	.remove {
		display: grid;
		place-items: center;
		width: 18px;
		height: 18px;
		padding: 0;
		border: 0;
		border-radius: 50%;
		background: transparent;
		color: var(--color-foreground-muted);
		cursor: pointer;
		opacity: 0;
		transition: opacity 150ms ease;
	}
	.remove path {
		stroke: currentColor;
		stroke-width: 1.3;
		stroke-linecap: round;
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
		width: 22px;
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
		margin-left: 0;
		background: var(--color-primary);
		border-radius: 0;
		transition: transform 150ms ease;
	}
	.tick::before {
		content: '';
		position: absolute;
		left: 50%;
		top: -3px;
		width: 7px;
		height: 7px;
		margin-left: -4px;
		border-radius: 50%;
		background: var(--color-background);
		border: 1.5px solid var(--color-primary);
		box-sizing: border-box;
		transition: transform 150ms ease;
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
		cursor: pointer;
	}
	.birth-year {
		width: 4.2ch;
		font-variant-numeric: tabular-nums;
	}
	.birth select:focus,
	.birth input:focus {
		border-bottom-color: var(--color-primary);
	}
	.editor.unborn .birth-year {
		border-bottom-color: var(--color-primary);
	}

	/* ---------- footer ---------- */
	.foot {
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
	}
	.quiet {
		padding: 6px 0;
		border: 0;
		background: transparent;
		font-family: var(--font-sans);
		font-size: 14px;
		color: var(--color-foreground-muted);
		cursor: pointer;
		transition: color 150ms ease;
	}
	.quiet:hover:not(:disabled) {
		color: var(--color-foreground);
	}
	.quiet:disabled {
		opacity: 0.45;
		cursor: default;
	}
	.quiet:focus-visible,
	.primary:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 3px;
		border-radius: 6px;
	}
	.add {
		position: absolute;
		z-index: 2;
		color: var(--color-primary);
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
			background 150ms ease,
			opacity 150ms ease;
	}
	.primary:hover:not(:disabled) {
		background: var(--color-primary-hover, var(--color-primary));
	}
	.primary:disabled {
		opacity: 0.4;
		cursor: default;
	}
	.status {
		margin: 12px 0 0;
		min-height: 1.4em;
		text-align: right;
		font-family: var(--font-sans);
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	.error {
		color: var(--color-error, var(--color-foreground));
	}

	@media (max-width: 560px) {
		.status {
			text-align: left;
		}
		/* On a phone the one filled button takes its own full-width row
		   under the two quiet ones. */
		.actions .primary {
			order: 5;
			flex-basis: 100%;
			padding: 12px 20px;
			font-size: 15px;
		}
	}

	/* No travel, instant. Svelte's transitions read `reduced` for their own
	   durations; this catches the CSS ones. */
	@media (prefers-reduced-motion: reduce) {
		.timeline-step *,
		.timeline-step *::before {
			transition: none !important;
			animation: none !important;
		}
	}
</style>
