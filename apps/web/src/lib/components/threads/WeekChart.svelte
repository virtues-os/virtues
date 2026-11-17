<script lang="ts">
	/**
	 * The week of threads: one row per active thread with time that week,
	 * then "Not in a thread", seven days across. One dot is fifteen minutes.
	 * A solid dot is time spent, a ring is time ahead, a gray dot is recorded
	 * time in no thread. Today has a dotted outline. No row shows a total:
	 * the dots are the amount, and a quiet day stays blank. Threads with no
	 * time that week sit on one line under the chart, where they still
	 * preview, open and take a drop.
	 */
	import type { Week } from '$lib/api/client';
	import { IconButton } from '$lib';
	import ProjectGlyph from '$lib/components/ProjectGlyph.svelte';
	import { dropZone } from '$lib/actions/dropZone';

	let {
		week,
		shownId = null,
		pinnedId = null,
		onPrev,
		onNext,
		onHover,
		onPick,
		onOpen,
		accepts,
		onDropTo,
	}: {
		week: Week;
		shownId?: string | null;
		pinnedId?: string | null;
		onPrev: () => void;
		onNext: () => void;
		onHover: (id: string | null) => void;
		onPick: (id: string) => void;
		onOpen: (id: string) => void;
		/** A row takes a drop when this says so. */
		accepts?: (data: DataTransfer) => boolean;
		onDropTo?: (id: string, e: DragEvent) => void;
	} = $props();

	let dropOn = $state<string | null>(null);

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
	const anyUnfiled = $derived(week.unfiled_minutes.some((m) => m >= 8));
	const minutesOf = (t: Week['threads'][number]) => t.days.reduce((n, d) => n + d.done_minutes + d.ahead_minutes, 0);
	const busy = $derived(week.threads.filter((t) => minutesOf(t) >= 8));
	const quiet = $derived(week.threads.filter((t) => minutesOf(t) < 8));

	function dropParams(id: string) {
		return {
			accepts: (data: DataTransfer) => !!accepts && accepts(data),
			onActive: (on: boolean) => (dropOn = on ? id : dropOn === id ? null : dropOn),
			onDrop: (e: DragEvent) => onDropTo?.(id, e),
			effect: () => 'link' as const,
		};
	}
</script>

<div class="week" role="grid" tabindex="-1" aria-label="This week by thread" onmouseleave={() => onHover(null)}>
	<div class="head" role="row">
		<span class="nav" role="columnheader">
			<IconButton icon="mynaui:chevron-left" label="Previous week" size="sm" onclick={onPrev} />
			<IconButton icon="mynaui:chevron-right" label="Next week" size="sm" onclick={onNext} />
		</span>
		{#each days as iso (iso)}
			{@const l = dayLabel(iso)}
			<span class="day-head" class:today={iso === week.today} role="columnheader">
				{l.weekday} {l.day}
			</span>
		{/each}
	</div>

	{#if busy.length === 0 && !anyUnfiled}
		<p class="none">
			Nothing is on your threads this week. Calendar events you add to a thread, and its dated tasks, show here as
			dots.
		</p>
	{/if}

	{#each busy as t (t.id)}
		<div
			class="row"
			class:shown={t.id === shownId}
			class:pinned={t.id === pinnedId}
			class:drop-on={dropOn === t.id}
			role="row"
			tabindex="-1"
			onmouseenter={() => onHover(t.id)}
			onclick={() => onPick(t.id)}
			ondblclick={() => onOpen(t.id)}
			onkeydown={(e) => {
				if (e.key === 'Enter') onOpen(t.id);
			}}
			use:dropZone={dropParams(t.id)}
		>
			<span class="label" role="rowheader">
				<ProjectGlyph project={t} size={16} />
				<span class="name">{t.name || 'Untitled thread'}</span>
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

{#if quiet.length > 0}
	<div class="quiet-list" role="list" aria-label="Threads with nothing this week" onmouseleave={() => onHover(null)}>
		<span class="quiet-label">Nothing this week</span>
		{#each quiet as t (t.id)}
			<button
				type="button"
				role="listitem"
				class="chip"
				class:shown={t.id === shownId}
				class:pinned={t.id === pinnedId}
				class:drop-on={dropOn === t.id}
				onmouseenter={() => onHover(t.id)}
				onfocus={() => onHover(t.id)}
				onclick={() => onPick(t.id)}
				ondblclick={() => onOpen(t.id)}
				use:dropZone={dropParams(t.id)}
			>
				<ProjectGlyph project={t} size={14} />
				<span>{t.name || 'Untitled thread'}</span>
			</button>
		{/each}
	</div>
{/if}

<style>
	.week {
		display: flex;
		flex-direction: column;
		margin-top: 8px;
		overflow-x: auto;
	}
	.head,
	.row {
		display: grid;
		grid-template-columns: minmax(160px, 220px) repeat(7, minmax(56px, 1fr));
		align-items: start;
	}
	.head {
		align-items: center;
		min-height: 32px;
	}
	.nav {
		display: inline-flex;
		gap: 4px;
	}
	.day-head {
		margin: 0 4px;
		padding: 8px 0 4px;
		border: 1px dotted transparent;
		border-bottom: 0;
		border-radius: var(--radius-md) var(--radius-md) 0 0;
		font-size: 12px;
		line-height: 16px;
		text-align: center;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
	}
	.day-head.today {
		border-color: var(--color-foreground-subtle);
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
		font-size: 16px;
		line-height: 20px;
		color: var(--color-foreground);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.name.muted {
		font-family: var(--font-sans);
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	/* Today is one dotted column: the head draws its top, each row its
	   sides, and the last row its foot. */
	.cell {
		display: flex;
		flex-wrap: wrap;
		align-content: flex-start;
		gap: 4px;
		min-height: 44px;
		padding: 12px 8px;
		margin: 0 4px;
		box-sizing: border-box;
		border: 1px dotted transparent;
		border-top: 0;
		border-bottom: 0;
	}
	.cell.today {
		border-color: var(--color-foreground-subtle);
	}
	.row:last-child .cell.today {
		border-bottom: 1px dotted var(--color-foreground-subtle);
		border-radius: 0 0 var(--radius-md) var(--radius-md);
	}
	.dot {
		flex: none;
		width: 6px;
		height: 6px;
		border-radius: 50%;
		box-sizing: border-box;
	}
	.dot.done {
		background: var(--color-foreground);
	}
	.dot.ahead {
		border: 1px solid var(--color-foreground-muted);
	}
	.dot.unfiled {
		background: var(--color-foreground-subtle);
	}
	.none {
		margin: 8px 0 0;
		max-width: 520px;
		font-size: 15px;
		line-height: 24px;
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
		font-size: 13px;
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
		font-size: 16px;
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
		.chip {
			transition: none;
		}
	}
</style>
