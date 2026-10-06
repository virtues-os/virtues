<!--
	ActivityHeatmap.svelte

	A contribution-style calendar: weeks as columns, one square per day, four
	tints of the accent for how busy a day was. For rhythm, streaks and quiet
	weeks, not exact comparison. Ported from uiarc's activity-heatmap (React +
	motion) to Svelte: the grid, keyboard model, legend and summary are the
	same; the motion is Svelte transitions plus motion's DOM `animate`.

	- Cells wave in once on view; a new range recolors the grid in a sweep.
	- One tooltip serves the whole grid and glides between cells.
	- Legend swatches preview a level on hover and pin it on click.
	- Arrow keys move by day (up/down) and week (left/right), Home/End jump,
	  Enter/Space select, Escape hides the tooltip.

	On touch a tap shows the tooltip but never selects. A day is a ~10px square
	among hundreds, so a finger can only pick *a* day, and opening the wrong one
	on every tap is worse than reading the calendar and opening days from the
	lists beside it.
-->

<script lang="ts">
	import { onDestroy, untrack, type Snippet } from 'svelte';
	import { prefersReducedMotion } from 'svelte/motion';
	import { cubicOut, quintOut } from 'svelte/easing';
	import type { TransitionConfig } from 'svelte/transition';
	import { animate } from 'motion';
	import type { ActivityDay } from '$lib/wiki/activity';

	interface Props {
		/** One entry per day, oldest first. Days missing inside the range count as zero. */
		days: ActivityDay[];
		/** Accessible name for the grid, such as "Events per day, last six months". */
		label: string;
		/** Finishes the summary line: "1,284 events in {period}". */
		period: string;
		/** Nouns for the count. */
		unit?: { one: string; other: string };
		/** Upper bounds for levels one to three; anything above the last is level four. Defaults to quarters of the busiest day. */
		thresholds?: [number, number, number];
		weekStartsOn?: 0 | 1;
		selectedDate?: string | null;
		onSelectDate?: (date: string) => void;
		/** The last day that can hold anything, usually today. Later days in the range keep the calendar's shape as blank squares. */
		lastDay?: string;
		/** Controls beside the summary, such as a range switch. */
		actions?: Snippet;
		/** A line under the legend, set off by a hairline: what the selected day holds. */
		footer?: Snippet;
		locale?: string;
		class?: string;
	}

	let {
		days,
		label,
		period,
		unit = { one: 'event', other: 'events' },
		thresholds,
		weekStartsOn = 0,
		selectedDate = null,
		onSelectDate,
		lastDay,
		actions,
		footer,
		locale = 'en-US',
		class: className,
	}: Props = $props();

	type Model = {
		start: number;
		length: number;
		/** Days that can be read and selected: the range up to `lastDay`. */
		live: number;
		lead: number;
		weeks: number;
		total: number;
		counts: number[];
		levels: number[];
		thresholds: [number, number, number];
		months: { key: string; col: number; label: string }[];
		/** How many days in the range sit at each level. */
		perLevel: number[];
	};
	type Tip = { key: string; primary: string; secondary: string; value: number; anchor: HTMLElement };
	/** How text changes: rolls up or down with the count, crossfades on a tie, or appears at once when the bubble opens. */
	type Change = 1 | -1 | 0 | 'instant';

	const DAY = 86_400_000;
	const LEVELS = [0, 1, 2, 3, 4] as const;
	const ROWS = [0, 1, 2, 3, 4, 5, 6] as const;
	/** Total wave travel in ms, however many weeks the range spans. */
	const WAVE = 420;
	const INSTANT = 120;
	const FAST = 150;
	const STANDARD = 240;
	const SNAPPY = { type: 'spring', visualDuration: 0.28, bounce: 0.12 } as const;
	const MORPH = { type: 'spring', visualDuration: 0.34, bounce: 0 } as const;
	const JUMP = { duration: 0 } as const;

	const toUtc = (iso: string) => Date.parse(`${iso}T00:00:00Z`);
	const toIso = (time: number) => new Date(time).toISOString().slice(0, 10);
	const noun = (count: number) => (count === 1 ? unit.one : unit.other);

	function buildModel(
		days: ActivityDay[],
		weekStartsOn: 0 | 1,
		thresholds: [number, number, number] | undefined,
		lastDay: string | undefined,
		locale: string
	): Model {
		const dates = days.map((day) => toUtc(day.date)).filter(Number.isFinite);
		const start = dates.length ? Math.min(...dates) : toUtc(toIso(Date.now()));
		const length = dates.length ? Math.round((Math.max(...dates) - start) / DAY) + 1 : 0;
		const live = lastDay
			? Math.min(length, Math.max(0, Math.round((toUtc(lastDay) - start) / DAY) + 1))
			: length;
		const counts = new Array<number>(length).fill(0);
		for (const day of days) {
			const index = Math.round((toUtc(day.date) - start) / DAY);
			if (index >= 0 && index < length) counts[index] += Math.max(0, day.count);
		}
		const max = counts.reduce((a, b) => Math.max(a, b), 0);
		const bounds =
			thresholds ??
			([
				Math.max(1, Math.ceil(max * 0.25)),
				Math.max(2, Math.ceil(max * 0.5)),
				Math.max(3, Math.ceil(max * 0.75)),
			] as [number, number, number]);
		const levels = counts.map((count) =>
			count <= 0 ? 0 : count <= bounds[0] ? 1 : count <= bounds[1] ? 2 : count <= bounds[2] ? 3 : 4
		);
		const lead = (new Date(start).getUTCDay() - weekStartsOn + 7) % 7;
		const weeks = Math.ceil((lead + length) / 7);
		const monthName = new Intl.DateTimeFormat(locale, { month: 'short', timeZone: 'UTC' });
		const months: Model['months'] = [];
		const seen = new Set<number>();
		for (let index = 0; index < length; index++) {
			const date = new Date(start + index * DAY);
			if (index !== 0 && date.getUTCDate() !== 1) continue;
			// Keyed by month of the year, so a label glides to its new week when
			// the range shifts; a range that spans the same month twice keys the
			// second one apart.
			const month = date.getUTCMonth();
			months.push({
				key: `${month}-${seen.has(month) ? 1 : 0}`,
				col: Math.floor((lead + index) / 7),
				label: monthName.format(date),
			});
			seen.add(month);
		}
		// A partial first month keeps its label only when there is room before the next one.
		if (months.length > 1 && months[1].col - months[0].col < 3) months.shift();
		const perLevel = [0, 0, 0, 0, 0];
		for (let index = 0; index < live; index++) perLevel[levels[index]]++;
		return {
			start,
			length,
			live,
			lead,
			weeks,
			total: counts.reduce((a, b) => a + b, 0),
			counts,
			levels,
			thresholds: bounds,
			months,
			perLevel,
		};
	}

	const reduced = $derived(prefersReducedMotion.current);
	const uid = $props.id();

	let rootEl = $state<HTMLDivElement>();
	let scrollerEl = $state<HTMLDivElement>();
	let gridEl = $state<HTMLDivElement>();
	let ringEl = $state<HTMLSpanElement>();
	let tipEl = $state<HTMLDivElement>();
	let tipBodyEl = $state<HTMLSpanElement>();
	let measureEl = $state<HTMLSpanElement>();

	const model = $derived(buildModel(days, weekStartsOn, thresholds, lastDay, locale));
	const formats = $derived({
		long: new Intl.DateTimeFormat(locale, {
			weekday: 'long',
			month: 'long',
			day: 'numeric',
			year: 'numeric',
			timeZone: 'UTC',
		}),
		short: new Intl.DateTimeFormat(locale, {
			weekday: 'short',
			month: 'short',
			day: 'numeric',
			year: 'numeric',
			timeZone: 'UTC',
		}),
		weekday: new Intl.DateTimeFormat(locale, { weekday: 'short', timeZone: 'UTC' }),
		number: new Intl.NumberFormat(locale),
	});

	// A new range sweeps forward when it is later than the last one and back
	// when it is earlier. The total rolls in the direction it moved.
	let direction = $state(1);
	let totalDirection = $state(1);
	let lastStart = untrack(() => model.start);
	let lastTotal = untrack(() => model.total);
	/** The total's width springs only right after it changes, never on a resize. */
	let widthArmedUntil = 0;
	$effect.pre(() => {
		const { start, total } = model;
		if (start !== lastStart) {
			direction = start > lastStart ? 1 : -1;
			lastStart = start;
		}
		if (total !== lastTotal) {
			totalDirection = total > lastTotal ? 1 : -1;
			lastTotal = total;
			widthArmedUntil = performance.now() + 600;
		}
	});

	// Reveal once, when the grid scrolls into view.
	let inView = $state(false);
	let reveal = $state<'hidden' | 'revealing' | 'done'>('hidden');
	$effect(() => {
		if (!scrollerEl || inView) return;
		if (typeof IntersectionObserver === 'undefined') {
			inView = true;
			return;
		}
		const observer = new IntersectionObserver(
			(entries) => {
				if (entries.some((entry) => entry.isIntersecting)) inView = true;
			},
			{ threshold: 0.35 }
		);
		observer.observe(scrollerEl);
		return () => observer.disconnect();
	});
	$effect(() => {
		if (reveal !== 'hidden' || !(inView || reduced)) return;
		const frame = requestAnimationFrame(() => (reveal = reduced ? 'done' : 'revealing'));
		return () => cancelAnimationFrame(frame);
	});
	$effect(() => {
		if (reveal !== 'revealing') return;
		const timer = setTimeout(() => (reveal = 'done'), WAVE + 700);
		return () => clearTimeout(timer);
	});

	const selectedIndex = $derived(
		selectedDate ? Math.round((toUtc(selectedDate) - model.start) / DAY) : -1
	);
	const hasSelection = $derived(selectedIndex >= 0 && selectedIndex < model.live);
	let focusIndex = $state<number | null>(null);
	const tabIndexDay = $derived(
		focusIndex !== null && focusIndex < model.live
			? focusIndex
			: hasSelection
				? selectedIndex
				: model.live - 1
	);

	// Legend: hovering or focusing a level previews it on the grid, clicking
	// pins it. The caption says how many days sit at that level.
	let preview = $state<number | null>(null);
	let pinned = $state<number | null>(null);
	let legendFocus = $state(0);
	let previewTimer = 0;
	const highlight = $derived(preview ?? pinned);
	function previewLevel(level: number | null) {
		clearTimeout(previewTimer);
		if (level !== null) preview = level;
		else previewTimer = window.setTimeout(() => (preview = null), 90);
	}

	const ranges = $derived.by(() => {
		const [a, b, c] = model.thresholds;
		return [
			`no ${unit.other}`,
			a === 1 ? `1 ${unit.one}` : `1 to ${a} ${unit.other}`,
			`${a + 1} to ${b} ${unit.other}`,
			`${b + 1} to ${c} ${unit.other}`,
			`${c + 1} or more ${unit.other}`,
		];
	});
	const shortRanges = $derived.by(() => {
		const [a, b, c] = model.thresholds;
		return [
			`no ${unit.other}`,
			a === 1 ? `1 ${unit.one}` : `1-${a} ${unit.other}`,
			`${a + 1}-${b} ${unit.other}`,
			`${b + 1}-${c} ${unit.other}`,
			`${c + 1}+ ${unit.other}`,
		];
	});
	const dayCount = $derived(
		highlight === null
			? ''
			: `${formats.number.format(model.perLevel[highlight])} ${model.perLevel[highlight] === 1 ? 'day' : 'days'}`
	);
	const caption = $derived(highlight === null ? '' : `${dayCount} with ${shortRanges[highlight]}`);
	const summary = $derived(`${formats.number.format(model.total)} ${noun(model.total)} in ${period}`);

	// Tooltip: one bubble for the whole grid. It jumps into place when it
	// opens and glides on a spring between cells after that.
	let tip = $state.raw<Tip | null>(null);
	let open = $state(false);
	let change = $state<Change>('instant');
	let wasOpen = false;
	let hideTimer = 0;

	function contentFor(index: number, anchor: HTMLElement): Tip {
		const count = model.counts[index];
		const time = model.start + index * DAY;
		return {
			key: `day-${toIso(time)}`,
			primary: count ? `${formats.number.format(count)} ${noun(count)}` : `No ${unit.other}`,
			secondary: formats.short.format(new Date(time)),
			value: count,
			anchor,
		};
	}
	function show(next: Tip) {
		clearTimeout(hideTimer);
		change = tip && wasOpen ? (Math.sign(next.value - tip.value) as Change) : 'instant';
		tip = next;
		open = true;
	}
	function hideSoon(delay = 110) {
		clearTimeout(hideTimer);
		hideTimer = window.setTimeout(() => (open = false), delay);
	}

	onDestroy(() => {
		clearTimeout(previewTimer);
		clearTimeout(hideTimer);
	});

	$effect(() => {
		const current = tip;
		const isOpen = open;
		const glide = isOpen && wasOpen && !reduced;
		const root = rootEl, measure = measureEl, bubble = tipEl, body = tipBodyEl, scroller = scrollerEl;
		if (!current || !root || !measure || !bubble || !body || !scroller) return;
		const place = (glide: boolean) => {
			const box = root.getBoundingClientRect();
			const cell = current.anchor.getBoundingClientRect();
			const width = Math.ceil(measure.getBoundingClientRect().width);
			const half = width / 2 + 13;
			const x = Math.min(
				Math.max(cell.left - box.left + cell.width / 2, half + 2),
				box.width - half - 2
			);
			const y = cell.top - box.top;
			animate(bubble, { x, y }, glide ? SNAPPY : JUMP);
			animate(body, { width }, glide ? MORPH : JUMP);
		};
		place(glide);
		wasOpen = isOpen;
		if (!isOpen) return;
		// The grid scrolls sideways on narrow screens; the bubble stays pinned to its cell while it does.
		const follow = () => place(false);
		scroller.addEventListener('scroll', follow, { passive: true });
		return () => scroller.removeEventListener('scroll', follow);
	});

	function indexOf(element: EventTarget | Element | null) {
		const cell = element instanceof Element ? element.closest<HTMLElement>('[data-index]') : null;
		return cell && gridEl?.contains(cell) ? { cell, index: Number(cell.dataset.index) } : null;
	}
	const focusDay = (index: number) =>
		gridEl?.querySelector<HTMLElement>(`[data-index="${index}"]`)?.focus();
	const select = (index: number) => onSelectDate?.(toIso(model.start + index * DAY));

	let lastPointer = 'mouse';
	function onGridPointerDown(event: PointerEvent) {
		lastPointer = event.pointerType;
	}
	function onGridPointerOver(event: PointerEvent) {
		const hit = indexOf(event.target);
		if (hit) show(contentFor(hit.index, hit.cell));
	}
	function onGridPointerLeave(event: PointerEvent) {
		// A keyboard user keeps their bubble on the focused day. On touch the
		// bubble lingers long enough to read.
		const focused = indexOf(document.activeElement);
		if (focused?.cell.matches(':focus-visible')) show(contentFor(focused.index, focused.cell));
		else hideSoon(event.pointerType === 'touch' ? 2400 : 110);
	}
	function onGridClick(event: MouseEvent) {
		if (lastPointer === 'touch') return;
		const hit = indexOf(event.target);
		if (hit) select(hit.index);
	}
	function onGridFocusIn(event: FocusEvent) {
		const hit = indexOf(event.target);
		if (!hit) return;
		focusIndex = hit.index;
		show(contentFor(hit.index, hit.cell));
	}
	function onGridFocusOut(event: FocusEvent) {
		if (!gridEl?.contains(event.relatedTarget as Node | null)) hideSoon(0);
	}
	function onGridKeyDown(event: KeyboardEvent) {
		const hit = indexOf(event.target);
		if (!hit) return;
		const moves: Record<string, number> = {
			ArrowUp: hit.index - 1,
			ArrowDown: hit.index + 1,
			ArrowLeft: hit.index - 7,
			ArrowRight: hit.index + 7,
			Home: 0,
			End: model.live - 1,
		};
		if (event.key in moves) {
			event.preventDefault();
			focusDay(Math.min(Math.max(moves[event.key], 0), model.live - 1));
		} else if (event.key === 'Enter' || event.key === ' ') {
			event.preventDefault();
			select(hit.index);
		} else if (event.key === 'Escape' && open) {
			event.preventDefault();
			open = false;
		}
	}

	function onLegendKeyDown(event: KeyboardEvent) {
		const moves: Record<string, number> = {
			ArrowLeft: legendFocus - 1,
			ArrowRight: legendFocus + 1,
			Home: 0,
			End: 4,
		};
		if (event.key === 'Escape') {
			previewLevel(null);
			return;
		}
		if (!(event.key in moves)) return;
		event.preventDefault();
		const next = Math.min(Math.max(moves[event.key], 0), 4);
		rootEl?.querySelector<HTMLElement>(`[data-level-key="${next}"]`)?.focus();
	}

	// The selection ring glides between cells on CSS. When it first appears it
	// fades in where it lands instead of flying in from its last spot.
	const ringShown = $derived(hasSelection && reveal !== 'hidden');
	let ringWasShown = untrack(() => ringShown);
	$effect(() => {
		const shown = ringShown;
		if (ringEl && shown && !ringWasShown) {
			ringEl.style.transitionProperty = 'opacity';
			void ringEl.offsetWidth;
			ringEl.style.transitionProperty = '';
		}
		ringWasShown = shown;
	});

	const cols = $derived(Array.from({ length: model.weeks }, (_, col) => col));
	const maxDiagonal = $derived(Math.max(1, model.weeks - 1 + 6));
	const step = $derived(Math.min(12, WAVE / maxDiagonal));
	const weekdayLabels = $derived(
		ROWS.map((row) => {
			const weekday = (row + weekStartsOn) % 7;
			// January 4, 1970 was a Sunday.
			return weekday % 2 === 1 ? formats.weekday.format(new Date((3 + weekday) * DAY)) : '';
		})
	);
	const selectedCol = $derived(hasSelection ? Math.floor((selectedIndex + model.lead) / 7) : 0);
	const selectedRow = $derived(hasSelection ? (selectedIndex + model.lead) % 7 : 0);

	function cellLabel(index: number): string {
		const count = model.counts[index];
		const date = new Date(model.start + index * DAY);
		return `${count ? formats.number.format(count) : 'No'} ${noun(count)}, ${formats.long.format(date)}`;
	}

	type RollParams = { dir: Change; phase: 'in' | 'out'; delay?: number };
	/**
	 * Text that changes rises in from a soft blur while the old text lifts away
	 * a little faster. The outgoing copy leaves the flow at once, so the two
	 * never share a line.
	 */
	function roll(node: HTMLElement, { dir, phase, delay = 0 }: RollParams): TransitionConfig {
		if (phase === 'out') {
			node.style.position = 'absolute';
			node.style.left = '0';
			node.style.top = '0';
		}
		if (dir === 'instant') return { duration: 0 };
		if (prefersReducedMotion.current) {
			return phase === 'in' ? { duration: FAST, css: (t) => `opacity: ${t}` } : { duration: 0 };
		}
		const sign = (dir || 1) * (phase === 'in' ? 1 : -1);
		const blur = phase === 'in' && dir !== 0 ? 4 : 2;
		return {
			delay: phase === 'in' ? delay : 0,
			duration: phase === 'in' ? STANDARD : INSTANT,
			easing: phase === 'in' ? quintOut : cubicOut,
			css: (t, u) =>
				`opacity: ${t}; transform: translateY(${(0.3 * sign * u).toFixed(3)}em); filter: blur(${(blur * u).toFixed(2)}px)`,
		};
	}

	/** When the total gains or loses a digit, its width follows on a spring instead of shifting the sentence in one frame. */
	function springWidth(frame: HTMLElement) {
		const inner = frame.firstElementChild as HTMLElement | null;
		if (!inner || typeof ResizeObserver === 'undefined') return;
		let sized = false;
		const observer = new ResizeObserver(() => {
			const width = inner.getBoundingClientRect().width;
			const glide = sized && !prefersReducedMotion.current && performance.now() < widthArmedUntil;
			animate(frame, { width }, glide ? MORPH : JUMP);
			sized = true;
		});
		observer.observe(inner);
		return () => observer.disconnect();
	}
</script>

{#snippet rolling(value: number, dir: number)}
	{@const chars = [...formats.number.format(value)]}
	<!-- Places keep their identity from the right, so only changed digits turn. -->
	{@const places = chars.map((char, i) => ({ char, place: chars.length - i }))}
	<span class="rolling-frame" aria-hidden="true" {@attach springWidth}>
		<span class="rolling">
			{#each places as { char, place } (place)}
				<span class="place">
					{#key char}
						<span
							class="place-char"
							in:roll={{ dir: dir as Change, phase: 'in', delay: Math.min(place * 18, 90) }}
							out:roll={{ dir: dir as Change, phase: 'out' }}>{char}</span
						>
					{/key}
				</span>
			{/each}
		</span>
	</span>
{/snippet}

<div bind:this={rootEl} class={['heatmap', className]} style:--weeks={model.weeks}>
	<div class="header">
		<p class="summary">
			<span class="total">{@render rolling(model.total, totalDirection)} {noun(model.total)}</span>
			<span class="period">
				in
				<span class="rise" aria-hidden="true">
					{#key period}
						<span class="rise-line" in:roll={{ dir: 1, phase: 'in' }} out:roll={{ dir: 1, phase: 'out' }}
							>{period}</span
						>
					{/key}
				</span>
			</span>
			<span class="sr-only" role="status">{summary}</span>
		</p>
		{#if actions}
			<div class="actions">{@render actions()}</div>
		{/if}
	</div>

	<div bind:this={scrollerEl} class="scroller">
		<div class="canvas">
			<div class="months" aria-hidden="true">
				{#each model.months as month (month.key)}
					<span class="month" style:--col={month.col}>{month.label}</span>
				{/each}
			</div>
			<div class="weekdays" aria-hidden="true">
				{#each weekdayLabels as text, row (row)}
					<span class="weekday">{text}</span>
				{/each}
			</div>
			<div class="plot">
				<div
					bind:this={gridEl}
					role="grid"
					tabindex="-1"
					aria-label={label}
					aria-readonly="true"
					aria-describedby="{uid}-legend"
					class="grid"
					data-reveal={reveal}
					data-direction={direction < 0 ? 'back' : undefined}
					data-highlight={highlight ?? undefined}
					onpointerdown={onGridPointerDown}
					onpointerover={onGridPointerOver}
					onpointerleave={onGridPointerLeave}
					onclick={onGridClick}
					onfocusin={onGridFocusIn}
					onfocusout={onGridFocusOut}
					onkeydown={onGridKeyDown}
				>
					{#each ROWS as row (row)}
						<div role="row" class="row">
							{#each cols as col (col)}
								{@const index = col * 7 + row - model.lead}
								{@const inRange = index >= 0 && index < model.length}
								{@const live = inRange && index < model.live}
								<span
									class="cell"
									style:--wave="{Math.round((col + row) * step)}ms"
									style:--wave-back="{Math.round((maxDiagonal - col - row) * step)}ms"
									role="gridcell"
									data-empty={inRange ? undefined : ''}
									data-future={inRange && !live ? '' : undefined}
									aria-hidden={live ? undefined : 'true'}
									data-index={live ? index : undefined}
									data-level={live ? model.levels[index] : undefined}
									tabindex={live ? (index === tabIndexDay ? 0 : -1) : undefined}
									aria-selected={live && onSelectDate ? index === selectedIndex : undefined}
									aria-label={live ? cellLabel(index) : undefined}
								></span>
							{/each}
						</div>
					{/each}
				</div>
				<span
					bind:this={ringEl}
					class="ring"
					data-shown={ringShown || undefined}
					style:--col={selectedCol}
					style:--row={selectedRow}
					aria-hidden="true"
				></span>
			</div>
		</div>
	</div>

	<div class="legend">
		<span id="{uid}-legend" class="sr-only"
			>Darker squares mean more {unit.other}. Levels: {ranges.join(', ')}.</span
		>
		<span class="caption" aria-live="polite">
			<span class="rise" aria-hidden="true">
				{#key caption}
					<span class="rise-line" in:roll={{ dir: 1, phase: 'in' }} out:roll={{ dir: 1, phase: 'out' }}>
						{#if caption}
							<span class="caption-long">{caption}</span><span class="caption-short">{dayCount}</span>
						{/if}
					</span>
				{/key}
			</span>
			<span class="sr-only">{caption}</span>
		</span>
		<span class="scale">
			<span class="legend-text" aria-hidden="true">Less</span>
			<!-- Arrow keys move the roving tabindex between the swatch buttons inside. -->
			<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
			<div class="swatches" role="group" aria-label="Highlight days by level" onkeydown={onLegendKeyDown}>
				{#each LEVELS as level (level)}
					<button
						type="button"
						class="swatch"
						data-level-key={level}
						data-level={level}
						tabindex={level === legendFocus ? 0 : -1}
						aria-pressed={pinned === level}
						aria-label="Highlight days with {ranges[level]}"
						onclick={() => (pinned = pinned === level ? null : level)}
						onpointerenter={() => previewLevel(level)}
						onpointerleave={() => previewLevel(null)}
						onfocus={() => {
							legendFocus = level;
							previewLevel(level);
						}}
						onblur={() => previewLevel(null)}
					></button>
				{/each}
			</div>
			<span class="legend-text" aria-hidden="true">More</span>
		</span>
	</div>

	{#if footer}
		<div class="footer">{@render footer()}</div>
	{/if}

	<div bind:this={tipEl} class="tip" aria-hidden="true">
		<div class="bubble" data-open={open && tip ? '' : undefined}>
			<span bind:this={tipBodyEl} class="tip-body">
				<span bind:this={measureEl} class="tip-measure"
					><span class="tip-primary">{tip?.primary}</span><span class="tip-secondary"
						>{tip?.secondary}</span
					></span
				>
				{#if tip}
					{#key tip.key}
						<span
							class="tip-lines"
							in:roll={{ dir: change, phase: 'in' }}
							out:roll={{ dir: change, phase: 'out' }}
						>
							<span class="tip-primary">{tip.primary}</span>
							<span class="tip-secondary">{tip.secondary}</span>
						</span>
					{/key}
				{/if}
			</span>
		</div>
	</div>
</div>

<style>
	/* Levels are tints of the accent over a neutral empty square, so the grid
	   follows the theme. Set --heatmap-surface when the heatmap sits on a
	   ground other than the page's. */
	.heatmap {
		--surface: var(--heatmap-surface, var(--color-surface));
		--level-0: color-mix(in oklab, var(--color-foreground) 7%, var(--surface));
		--level-1: color-mix(in oklab, var(--color-primary) 26%, var(--level-0));
		--level-2: color-mix(in oklab, var(--color-primary) 50%, var(--level-0));
		--level-3: color-mix(in oklab, var(--color-primary) 76%, var(--level-0));
		--level-4: var(--color-primary);
		--spring: cubic-bezier(0.34, 1.3, 0.64, 1);
		--ease-enter: cubic-bezier(0.22, 1, 0.36, 1);
		--gap: 4px;
		--label: 30px;
		--cell-max: 15px;
		container-type: inline-size;
		position: relative;
		display: grid;
		min-width: 0;
		/* No wider than the grid at its largest, so the summary, the legend and
		   the footer end where the squares do and the whole piece sits left. */
		max-width: calc(var(--label) + var(--weeks) * (var(--cell-max) + var(--gap)));
		gap: 16px;
		color: var(--color-foreground);
		font-family: var(--font-sans);
	}

	.header {
		display: flex;
		min-width: 0;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: 12px 16px;
	}
	.summary {
		display: flex;
		min-width: 0;
		flex-wrap: wrap;
		align-items: baseline;
		column-gap: 0.3em;
		margin: 0;
		color: var(--color-foreground-muted);
		font-size: 13px;
		line-height: 1.5;
		font-variant-numeric: tabular-nums;
	}
	.total {
		color: var(--color-foreground);
		font-weight: 500;
		white-space: nowrap;
	}
	.period {
		white-space: nowrap;
	}
	.actions {
		display: flex;
		flex: none;
		align-items: center;
	}
	.rolling-frame {
		display: inline-flex;
		justify-content: flex-end;
		overflow: clip;
		overflow-clip-margin: 0.2em;
	}
	.rolling {
		display: inline-flex;
		flex: none;
	}
	/* Each place is its own window, so a turning digit clips at the line. */
	.place {
		position: relative;
		display: inline-flex;
		overflow: clip;
		overflow-clip-margin: 0.15em;
	}
	.place-char,
	.rise-line {
		display: inline-block;
		white-space: nowrap;
	}
	.rise {
		position: relative;
		display: inline-flex;
	}

	/* The scroller starts at its right edge (the most recent weeks) without
	   script, through its writing direction. */
	.scroller {
		direction: rtl;
		overflow-x: auto;
		overflow-y: hidden;
		margin: -6px;
		padding: 6px;
		overscroll-behavior-x: contain;
		scrollbar-color: var(--color-border-strong) transparent;
		scrollbar-width: thin;
	}
	.canvas {
		--cell: clamp(
			9px,
			calc((100cqi - var(--label) - var(--gap) * var(--weeks)) / var(--weeks)),
			var(--cell-max)
		);
		--step: calc(var(--cell) + var(--gap));
		direction: ltr;
		display: grid;
		width: max-content;
		/* The scroller is rtl so an overflowing grid opens on its newest week;
		   this keeps one that fits on the left instead. */
		margin-right: auto;
		grid-template-columns: var(--label) auto;
		grid-template-rows: auto auto;
		column-gap: var(--gap);
		row-gap: 8px;
	}
	.months {
		position: relative;
		grid-row: 1;
		grid-column: 2;
		height: 1rem;
		color: var(--color-foreground-muted);
		font-size: 12px;
		line-height: 1rem;
	}
	/* Month labels glide to their new week when a range starts on a different weekday. */
	.month {
		position: absolute;
		top: 0;
		left: 0;
		white-space: nowrap;
		transform: translateX(calc(var(--col) * var(--step)));
		transition: transform 420ms var(--spring);
	}
	/* Weekday labels stay pinned while a narrow grid scrolls under them. */
	.weekdays {
		position: sticky;
		z-index: 1;
		left: -6px;
		display: grid;
		grid-row: 1 / 3;
		grid-column: 1;
		grid-template-rows: repeat(7, var(--cell));
		align-content: end;
		row-gap: var(--gap);
		margin: 0 calc(var(--gap) * -1) 0 -6px;
		padding: 0 var(--gap) 0 6px;
		background: var(--surface);
		color: var(--color-foreground-muted);
		font-size: 12px;
	}
	.weekday {
		display: flex;
		height: var(--cell);
		align-items: center;
		white-space: nowrap;
		line-height: 1;
	}
	.plot {
		position: relative;
		grid-row: 2;
		grid-column: 2;
	}
	.grid {
		display: grid;
		gap: var(--gap);
		outline: none;
	}
	.row {
		display: flex;
		gap: var(--gap);
	}

	/* One square per day. The wave delay rides a custom property so a reveal
	   and a recolor sweep the same diagonal. */
	.cell {
		--delay: var(--wave);
		display: block;
		width: var(--cell);
		height: var(--cell);
		flex: none;
		border-radius: calc(var(--cell) * 0.28);
		background: var(--level-0);
		outline: 1.5px solid transparent;
		outline-offset: 1px;
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
		transition:
			background-color 320ms var(--ease-premium) var(--delay),
			transform 420ms var(--spring) var(--delay),
			opacity var(--duration-fast) var(--ease-premium);
	}
	.grid[data-direction='back'] .cell {
		--delay: var(--wave-back);
	}
	.cell[data-level='1'] {
		background: var(--level-1);
	}
	.cell[data-level='2'] {
		background: var(--level-2);
	}
	.cell[data-level='3'] {
		background: var(--level-3);
	}
	.cell[data-level='4'] {
		background: var(--level-4);
	}
	.cell:focus-visible {
		outline-color: var(--color-foreground);
	}
	/* Days after the last day keep the year's shape: blank, not zero. */
	.cell[data-future] {
		border: 1px solid var(--level-0);
		background: transparent;
		cursor: default;
	}
	/* Days outside the range shrink away in the sweep and grow back when a range needs them. */
	.cell[data-empty] {
		transform: scale(0);
		background: var(--level-0);
		cursor: default;
	}
	.grid[data-reveal='hidden'] .cell {
		opacity: 0;
		transform: scale(0.5);
		transition: none;
	}
	.grid[data-reveal='hidden'] .cell[data-empty] {
		transform: scale(0);
	}
	.grid[data-reveal='revealing'] .cell {
		transition:
			background-color 320ms var(--ease-premium) var(--delay),
			transform 420ms var(--spring) var(--delay),
			opacity 360ms var(--ease-enter) var(--delay);
	}
	.grid[data-highlight] .cell:not([data-empty]) {
		opacity: 0.2;
	}
	.grid[data-highlight='0'] .cell[data-level='0'],
	.grid[data-highlight='1'] .cell[data-level='1'],
	.grid[data-highlight='2'] .cell[data-level='2'],
	.grid[data-highlight='3'] .cell[data-level='3'],
	.grid[data-highlight='4'] .cell[data-level='4'] {
		opacity: 1;
	}
	@media (hover: hover) and (pointer: fine) {
		.cell[data-index]:hover {
			outline-color: color-mix(in oklab, var(--color-foreground) 45%, transparent);
		}
	}
	/* On touch a tap reads a day and never opens one. */
	@media (pointer: coarse) {
		.cell {
			cursor: default;
		}
	}

	/* The selection ring glides between days, and follows its day when the range changes. */
	.ring {
		position: absolute;
		top: -3px;
		left: -3px;
		width: calc(var(--cell) + 6px);
		height: calc(var(--cell) + 6px);
		border: 1.5px solid var(--color-foreground);
		border-radius: calc(var(--cell) * 0.28 + 3px);
		opacity: 0;
		pointer-events: none;
		transform: translate(calc(var(--col) * var(--step)), calc(var(--row) * var(--step)));
		transition:
			transform 420ms var(--spring),
			opacity var(--duration-fast) var(--ease-premium);
	}
	.ring[data-shown] {
		opacity: 1;
	}

	.legend {
		display: flex;
		min-width: 0;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		color: var(--color-foreground-muted);
		font-size: 12px;
	}
	/* The caption names the previewed level; it sits apart from the scale so
	   its changing width never nudges the swatches. */
	.caption {
		position: relative;
		min-width: 0;
		overflow: hidden;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
		text-overflow: ellipsis;
	}
	.scale {
		display: flex;
		flex: none;
		align-items: center;
		gap: 8px;
	}
	.caption-short {
		display: none;
	}
	/* Narrow cards keep the day count and let the highlighted squares say which level it is. */
	@container (max-width: 440px) {
		.caption-long {
			display: none;
		}
		.caption-short {
			display: inline;
		}
	}
	.legend-text {
		line-height: 1;
	}
	.swatches {
		display: flex;
	}
	/* The visible square stays the size of a day while the button keeps a comfortable target around it. */
	.swatch {
		position: relative;
		display: grid;
		width: 15px;
		height: 20px;
		place-items: center;
		padding: 0;
		border: 0;
		background: transparent;
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
	}
	.swatch::before {
		content: '';
		width: 11px;
		height: 11px;
		border-radius: calc(11px * 0.28);
		background: var(--level-0);
		outline: 1.5px solid transparent;
		outline-offset: 1px;
	}
	.swatch[data-level='1']::before {
		background: var(--level-1);
	}
	.swatch[data-level='2']::before {
		background: var(--level-2);
	}
	.swatch[data-level='3']::before {
		background: var(--level-3);
	}
	.swatch[data-level='4']::before {
		background: var(--level-4);
	}
	.swatch[aria-pressed='true']::before,
	.swatch:focus-visible::before {
		outline-color: var(--color-foreground);
	}
	.swatch:focus-visible {
		outline: none;
	}

	/* One bubble for the whole grid. The outer layer glides between cells; the
	   bubble fades and scales from its tail. */
	.tip {
		position: absolute;
		z-index: 5;
		top: 0;
		left: 0;
		width: 0;
		height: 0;
		pointer-events: none;
	}
	.bubble {
		position: absolute;
		bottom: 8px;
		left: 0;
		display: block;
		padding: 8px 12px;
		border: 1px solid color-mix(in oklab, var(--color-background) 14%, var(--color-foreground));
		border-radius: 6px;
		background: var(--color-foreground);
		color: var(--color-background);
		opacity: 0;
		transform: translateX(-50%) scale(0.96);
		transform-origin: 50% 100%;
		transition:
			opacity 120ms var(--ease-premium),
			transform 120ms var(--ease-premium);
		will-change: transform, opacity;
	}
	.bubble[data-open] {
		opacity: 1;
		transform: translateX(-50%) scale(1);
		transition:
			opacity var(--duration-fast) var(--ease-enter),
			transform 280ms var(--spring);
	}
	.tip-body {
		position: relative;
		display: block;
		overflow: clip;
		overflow-clip-margin: 8px;
	}
	.tip-measure {
		position: absolute;
		top: 0;
		left: 0;
		display: grid;
		width: max-content;
		visibility: hidden;
	}
	.tip-lines {
		display: grid;
		width: max-content;
	}
	.tip-measure > span,
	.tip-lines > span {
		white-space: nowrap;
	}
	.tip-primary {
		font-size: 13px;
		font-weight: 500;
		line-height: 1.3;
		font-variant-numeric: tabular-nums;
	}
	.tip-secondary {
		color: color-mix(in oklab, var(--color-background) 68%, var(--color-foreground));
		font-size: 12px;
		line-height: 1.35;
	}

	.footer {
		padding-top: 16px;
		border-top: 1px solid var(--color-border-subtle);
	}

	.sr-only {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip-path: inset(50%);
		white-space: nowrap;
	}

	/* Keep the information, drop the travel: no wave, sweep, glide, or scale. */
	@media (prefers-reduced-motion: reduce) {
		.cell,
		.grid[data-reveal='revealing'] .cell {
			transition:
				background-color var(--duration-fast) var(--ease-premium),
				opacity var(--duration-fast) var(--ease-premium);
		}
		.grid[data-reveal='hidden'] .cell {
			transform: none;
		}
		.cell[data-empty],
		.grid[data-reveal='hidden'] .cell[data-empty] {
			opacity: 0;
			transform: none;
		}
		.month,
		.ring {
			transition: opacity var(--duration-fast) var(--ease-premium);
		}
		.bubble,
		.bubble[data-open] {
			transform: translateX(-50%);
			transition: opacity var(--duration-fast) var(--ease-premium);
		}
	}
</style>
