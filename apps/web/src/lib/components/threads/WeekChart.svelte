<script lang="ts" module>
	/** A thread with less than this many minutes in a week is quiet that week. */
	export const BUSY_MIN = 8;
</script>

<script lang="ts">
	/**
	 * The week of threads: one row per active thread with time that week,
	 * then "Not in a thread", seven days across. One dot is fifteen minutes.
	 * A solid dot is time spent, a ring is time ahead, a gray dot is recorded
	 * time in no thread. Today's column is softly filled. No row shows a total:
	 * the dots are the amount, and a quiet day stays blank. Threads with no
	 * time that week sit on one line under the chart, where they still
	 * preview, open and take a drop. Pointing at a row previews it; a click
	 * opens it.
	 */
	import type { Week } from '$lib/api/client';
	import { IconButton } from '$lib';
	import ProjectGlyph from '$lib/components/ProjectGlyph.svelte';
	import { dropZone } from '$lib/actions/dropZone';
	import { projectColor } from '$lib/sidebar/pin-colors';
	import { notesStore } from '$lib/stores/notes.svelte';

	let {
		week,
		shownId = null,
		pinnedId = null,
		hoverId = null,
		onPrev,
		onNext,
		onHover,
		onOpen,
		onMenu,
		accepts,
		onDropTo,
	}: {
		week: Week;
		shownId?: string | null;
		pinnedId?: string | null;
		/** The row under the pointer; the others' dots fade while it is. */
		hoverId?: string | null;
		onPrev: () => void;
		onNext: () => void;
		onHover: (id: string | null) => void;
		onOpen: (id: string) => void;
		/** A thread's menu, on a right-click. */
		onMenu?: (id: string, e: MouseEvent) => void;
		/** A row takes a drop when this says so. */
		accepts?: (data: DataTransfer) => boolean;
		onDropTo?: (id: string, e: DragEvent) => void;
	} = $props();

	let dropOn = $state<string | null>(null);

	/* Today's column is one filled shape laid over the chart, from its head
	   down to the last row, so it reads as one column through the gaps
	   between rows. It is measured from the laid-out cells. */
	let chartEl = $state<HTMLDivElement | null>(null);
	let ring = $state<{ x: number; y: number; w: number; h: number } | null>(null);

	function measureRing() {
		const el = chartEl;
		const head = el?.querySelector<HTMLElement>('.day-head.today');
		const rows = el?.querySelectorAll<HTMLElement>('.row');
		const last = rows?.[rows.length - 1];
		// A tab in the background is not laid out, and measures as zero.
		if (!el || !head || !last || !head.offsetParent || head.offsetWidth <= 2) {
			ring = null;
			return;
		}
		const y = head.offsetTop;
		ring = { x: head.offsetLeft, y, w: head.offsetWidth, h: last.offsetTop + last.offsetHeight - y };
	}

	$effect(() => {
		void week;
		const el = chartEl;
		if (!el) return;
		measureRing();
		// The day columns can change width while the chart keeps its own
		// size (a longer thread name, a font arriving), so each is watched.
		const ro = new ResizeObserver(measureRing);
		ro.observe(el);
		for (const part of el.querySelectorAll('.day-head, .row')) ro.observe(part);
		return () => ro.disconnect();
	});

	/** The most a day draws; more than twelve hours is still twelve hours of dots. */
	const MOST = 48;

	type Dot = 'done' | 'ahead' | 'unfiled';

	function dots(done: number, ahead: number, unfiled = 0): Dot[] {
		const n = (m: number) => Math.round(m / 15);
		const out: Dot[] = [
			...Array<Dot>(n(done)).fill('done'),
			...Array<Dot>(n(ahead)).fill('ahead'),
			...Array<Dot>(n(unfiled)).fill('unfiled'),
		];
		return out.slice(0, MOST);
	}

	function dayLabel(iso: string): { weekday: string; day: string } {
		const d = new Date(`${iso}T12:00:00`);
		return {
			weekday: d.toLocaleDateString('en-US', { weekday: 'short' }),
			day: String(d.getDate()),
		};
	}

	const days = $derived(
		(week.threads[0]?.days.map((d) => d.date) ??
			Array.from({ length: 7 }, (_, i) => {
				const d = new Date(`${week.start}T12:00:00`);
				d.setDate(d.getDate() + i);
				const m = String(d.getMonth() + 1).padStart(2, '0');
				return `${d.getFullYear()}-${m}-${String(d.getDate()).padStart(2, '0')}`;
			})),
	);
	const anyUnfiled = $derived(week.unfiled_minutes.some((m) => m >= BUSY_MIN));
	const minutesOf = (t: Week['threads'][number]) => t.days.reduce((n, d) => n + d.done_minutes + d.ahead_minutes, 0);
	const busy = $derived(week.threads.filter((t) => minutesOf(t) >= BUSY_MIN));
	/** The one row Tab reaches: the one shown, else the first. */
	const tabStop = $derived(busy.some((t) => t.id === shownId) ? shownId : (busy[0]?.id ?? null));
	const quiet = $derived(week.threads.filter((t) => minutesOf(t) < BUSY_MIN));

	function hours(minutes: number): string {
		const h = Math.floor(minutes / 60);
		const m = minutes % 60;
		if (h === 0) return `${m} minutes`;
		return m === 0 ? `${h} hours` : `${h} hours ${m} minutes`;
	}

	/** What the row says to a screen reader: the week's time, since the dots say it by eye. */
	function rowLabel(t: Week['threads'][number]): string {
		const done = t.days.reduce((n, d) => n + d.done_minutes, 0);
		const ahead = t.days.reduce((n, d) => n + d.ahead_minutes, 0);
		return `${t.name || 'Untitled thread'}. ${done ? `${hours(done)} done this week` : 'Nothing done this week'}, ${ahead ? `${hours(ahead)} ahead` : 'nothing ahead'}.`;
	}

	function dropParams(id: string) {
		return {
			accepts: (data: DataTransfer) => !!accepts && accepts(data),
			onActive: (on: boolean) => (dropOn = on ? id : dropOn === id ? null : dropOn),
			onDrop: (e: DragEvent) => onDropTo?.(id, e),
			effect: () => 'link' as const,
		};
	}
</script>

<div
	class="week"
	role="grid"
	tabindex="-1"
	aria-label="This week by thread"
	bind:this={chartEl}
	onmouseleave={() => onHover(null)}
>
	{#if ring}
		<span
			class="today-col"
			aria-hidden="true"
			style={`left: ${ring.x}px; top: ${ring.y}px; width: ${ring.w}px; height: ${ring.h}px`}
		></span>
	{/if}
	<div class="head" role="row">
		<span class="nav" role="columnheader">
			<IconButton icon="mynaui:chevron-left" label="Previous week" onclick={onPrev} />
			<IconButton icon="mynaui:chevron-right" label="Next week" onclick={onNext} />
		</span>
		{#each days as iso (iso)}
			{@const l = dayLabel(iso)}
			<span class="day-head" class:today={iso === week.today} role="columnheader">
				{l.weekday} {l.day}
			</span>
		{/each}
	</div>


	{#each busy as t (t.id)}
		<div
			class="row"
			class:shown={t.id === shownId}
			class:pinned={t.id === pinnedId}
			class:drop-on={dropOn === t.id}
			class:faded={!!hoverId && hoverId !== t.id}
			style={`--thread: ${projectColor(t)}`}
			aria-label={rowLabel(t)}
			role="row"
			data-thread-id={t.id}
			tabindex={t.id === tabStop ? 0 : -1}
			onmouseenter={() => onHover(t.id)}
			onfocus={() => onHover(t.id)}
			onclick={() => onOpen(t.id)}
			oncontextmenu={onMenu ? (e) => onMenu(t.id, e) : undefined}
			onkeydown={(e) => {
				if (e.key === 'Enter') {
					e.preventDefault();
					onOpen(t.id);
				}
			}}
			use:dropZone={dropParams(t.id)}
		>
			<span class="label" role="rowheader">
				<ProjectGlyph project={t} size={16} tinted />
				<span class="name">{t.name || 'Untitled thread'}</span>
				{#if notesStore.about(`/thread/${t.id}`).length}
					<span class="noticed" role="img" aria-label="Your server noticed something"></span>
				{/if}
			</span>
			{#each t.days as d (d.date)}
				<span class="cell" class:today={d.date === week.today} role="gridcell">
					{#each dots(d.done_minutes, d.ahead_minutes) as kind, i (i)}
						<span class="dot {kind}"></span>
					{/each}
				</span>
			{/each}
		</div>
	{/each}

	{#if anyUnfiled}
		<div class="row quiet" role="row">
			<span class="label" role="rowheader"><span class="name muted">Not in a thread</span></span>
			{#each week.unfiled_minutes as m, i (i)}
				<span class="cell" class:today={days[i] === week.today} role="gridcell">
					{#each dots(0, 0, m) as kind, j (j)}
						<span class="dot {kind}"></span>
					{/each}
				</span>
			{/each}
		</div>
	{/if}
</div>

{#if busy.length === 0 && !anyUnfiled}
	<p class="none">
		Nothing is on your threads this week. Calendar events you add to a thread, and its dated tasks, show here as dots.
	</p>
{/if}

{#if quiet.length > 0}
	<div class="quiet-list" role="list" aria-label="Threads with nothing this week" onmouseleave={() => onHover(null)}>
		<span class="quiet-label">Nothing this week</span>
		{#each quiet as t (t.id)}
			<span role="listitem">
			<button
				type="button"
				class="chip"
				class:shown={t.id === shownId}
				class:pinned={t.id === pinnedId}
				class:drop-on={dropOn === t.id}
				onmouseenter={() => onHover(t.id)}
				onfocus={() => onHover(t.id)}
				onclick={() => onOpen(t.id)}
				oncontextmenu={onMenu ? (e) => onMenu(t.id, e) : undefined}
				use:dropZone={dropParams(t.id)}
			>
				<ProjectGlyph project={t} size={14} tinted />
				<span>{t.name || 'Untitled thread'}</span>
				{#if notesStore.about(`/thread/${t.id}`).length}
					<span class="noticed" role="img" aria-label="Your server noticed something"></span>
				{/if}
			</button>
			</span>
		{/each}
	</div>
{/if}

<style>
	/* Rows sit 4px apart, so a hovered row's fill never touches the next
	   one, and the header sits 8px above the first. */
	.week {
		position: relative;
		display: flex;
		flex-direction: column;
		gap: 4px;
		overflow-x: auto;
	}
	.head,
	.row {
		display: grid;
		grid-template-columns: minmax(120px, 220px) repeat(7, minmax(52px, 1fr));
		align-items: start;
	}
	.head {
		align-items: center;
		min-height: 34px;
		margin-bottom: 4px;
	}
	.nav {
		display: inline-flex;
		gap: 4px;
		padding-left: 4px;
	}
	.day-head {
		margin: 0 4px;
		padding: 8px 0;
		font-size: var(--text-caption);
		line-height: 16px;
		text-align: center;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
	}
	.day-head.today {
		color: var(--color-primary);
	}
	.row {
		min-height: 44px;
		border-radius: var(--radius-md);
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.row:hover,
	.row.shown {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}
	.row.pinned {
		background: var(--active-bg);
	}
	.row.drop-on {
		background: var(--drop-tint);
	}
	.row.quiet {
		cursor: default;
		background: transparent;
	}
	.label {
		display: flex;
		align-items: center;
		gap: 8px;
		min-width: 0;
		min-height: 28px;
		padding: 8px 12px;
	}
	.name {
		font-family: var(--font-serif-ui);
		font-size: var(--text-body);
		line-height: 20px;
		color: var(--color-foreground);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.name.muted {
		font-family: var(--font-sans);
		font-size: var(--text-small);
		color: var(--color-foreground-muted);
	}
	.cell {
		display: flex;
		flex-wrap: wrap;
		align-content: flex-start;
		justify-content: center;
		gap: 4px;
		min-height: 44px;
		padding: 12px 4px;
		margin: 0 4px;
		box-sizing: border-box;
	}
	/* Laid over the rows, so a hovered row's fill does not hide it; at this
	   strength it tints the dots under it only slightly. */
	.today-col {
		position: absolute;
		z-index: 1;
		border-radius: var(--radius-lg);
		background: color-mix(in srgb, var(--color-primary) 7%, transparent);
		pointer-events: none;
	}
	.dot {
		flex: none;
		width: 8px;
		height: 8px;
		border-radius: 50%;
		box-sizing: border-box;
	}
	/* A thread's dots are in its own color: solid for time spent, a ring
	   for time ahead. */
	.dot.done {
		background: var(--thread, var(--color-foreground));
	}
	.dot.ahead {
		border: 1.5px solid var(--thread, var(--color-foreground-muted));
	}
	.dot {
		transition: opacity var(--motion-state) var(--ease-premium);
	}
	.row.faded .dot {
		opacity: 0.22;
	}
	.dot.unfiled {
		background: var(--color-foreground-subtle);
	}
	/* A note about this thread: the one accent, meaning "now". */
	.noticed {
		flex: none;
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--color-primary);
	}
	.none {
		margin: 8px 0 0;
		max-width: 520px;
		font-size: var(--text-label);
		line-height: 20px;
		color: var(--color-foreground-muted);
	}
	.quiet-list {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 4px 8px;
		margin-top: 24px;
	}
	.quiet-label {
		margin-right: 4px;
		font-size: var(--text-small);
		color: var(--color-foreground-muted);
	}
	.chip {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		height: 28px;
		padding: 0 8px;
		border: 0;
		border-radius: var(--radius-md);
		background: transparent;
		font-family: var(--font-serif-ui);
		font-size: var(--text-body);
		color: var(--color-foreground);
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.chip:hover,
	.chip.shown {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}
	.chip.pinned {
		background: var(--active-bg);
	}
	.chip.drop-on {
		background: var(--drop-tint);
	}
	@media (prefers-reduced-motion: reduce) {
		.row,
		.chip,
		.dot {
			transition: none;
		}
	}
</style>
