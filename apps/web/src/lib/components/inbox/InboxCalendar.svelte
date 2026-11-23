<script lang="ts">
	/**
	 * The Inbox as a month. Each thing sits on its day (`itemDay`): a task on
	 * its due day, a request on the day it runs, an insight on the day it
	 * stops mattering. A day shows three things and opens to the rest.
	 * Things with no day stay in the List, and the page says so.
	 */
	import { IconButton, TextAction } from '$lib';
	import { byDay, monthDays } from '$lib/inbox/calendar';
	import type { InboxItem } from '$lib/inbox/items';
	import { isoDay } from '$lib/tasks/due';
	import InboxMark from './InboxMark.svelte';

	let {
		items,
		picked = null,
		onPick,
	}: {
		items: InboxItem[];
		picked?: string | null;
		onPick: (key: string) => void;
	} = $props();

	/** A day shows this many things before it opens to the rest. */
	const SHOWN = 3;
	const WEEKDAYS = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];

	const today = isoDay(new Date());
	let month = $state(new Date(new Date().getFullYear(), new Date().getMonth(), 1));
	/** The day you opened to see everything on it. */
	let opened = $state<string | null>(null);

	const days = $derived(monthDays(month));
	const on = $derived(byDay(items));
	const inMonth = (day: string) => Number(day.slice(5, 7)) === month.getMonth() + 1;
	const isThisMonth = $derived(month.getFullYear() === new Date().getFullYear() && month.getMonth() === new Date().getMonth());
	const title = $derived(month.toLocaleDateString('en-US', { month: 'long', year: 'numeric' }));
	const undated = $derived(items.some((it) => !it.task?.due_on && !it.task?.next_run_at && !it.note?.until));

	function shift(by: number) {
		month = new Date(month.getFullYear(), month.getMonth() + by, 1);
		opened = null;
	}
</script>

<div class="calendar">
	<div class="head">
		<h3 class="month">{title}</h3>
		<span class="nav">
			{#if !isThisMonth}<TextAction quiet onclick={() => shift(new Date().getMonth() - month.getMonth() + 12 * (new Date().getFullYear() - month.getFullYear()))}>Today</TextAction>{/if}
			<IconButton icon="mynaui:chevron-left" label="Previous month" size="nested" onclick={() => shift(-1)} />
			<IconButton icon="mynaui:chevron-right" label="Next month" size="nested" onclick={() => shift(1)} />
		</span>
	</div>

	<div class="grid" role="grid" aria-label={title}>
		{#each WEEKDAYS as w (w)}
			<span class="weekday" role="columnheader">{w}</span>
		{/each}
		{#each days as day (day)}
			{@const things = on.get(day) ?? []}
			{@const open = opened === day}
			<div class="day" class:other={!inMonth(day)} class:today={day === today} role="gridcell" aria-label={day}>
				<span class="num">{Number(day.slice(8))}</span>
				{#each open ? things : things.slice(0, SHOWN) as it (it.key)}
					<button type="button" class="thing" class:on={it.key === picked} data-inbox-key={it.key} onclick={() => onPick(it.key)} data-tip={it.title}>
						<InboxMark kind={it.kind} glyph={it.glyph} />
						<span class="title">{it.title}</span>
					</button>
				{/each}
				{#if things.length > SHOWN}
					<span class="more"><TextAction quiet onclick={() => (opened = open ? null : day)}>{open ? 'Fewer' : 'More'}</TextAction></span>
				{/if}
			</div>
		{/each}
	</div>

	{#if undated}
		<p class="note">Things with no day stay in the List.</p>
	{/if}
</div>

<style>
	.calendar {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}
	.head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
	}
	.month {
		margin: 0;
		font-family: var(--font-serif);
		font-size: var(--text-lead);
		font-weight: 400;
		line-height: 24px;
		color: var(--color-foreground);
	}
	.nav {
		display: inline-flex;
		align-items: center;
		gap: 8px;
	}
	.grid {
		display: grid;
		grid-template-columns: repeat(7, minmax(0, 1fr));
		border-top: 1px solid var(--color-border-subtle);
		border-left: 1px solid var(--color-border-subtle);
	}
	.weekday {
		padding: 4px 8px;
		border-right: 1px solid var(--color-border-subtle);
		border-bottom: 1px solid var(--color-border-subtle);
		font-size: var(--text-caption);
		line-height: 16px;
		color: var(--color-foreground-muted);
	}
	.day {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
		min-height: 104px;
		padding: 4px;
		border-right: 1px solid var(--color-border-subtle);
		border-bottom: 1px solid var(--color-border-subtle);
	}
	.num {
		align-self: flex-end;
		min-width: 20px;
		height: 20px;
		padding: 0 4px;
		box-sizing: border-box;
		border-radius: var(--radius-full);
		font-size: var(--text-caption);
		line-height: 20px;
		text-align: center;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
	}
	.day.other .num {
		color: var(--color-foreground-subtle);
	}
	/* Today is the one day in the accent. */
	.day.today .num {
		background: var(--color-primary);
		color: var(--color-page);
	}
	.thing {
		display: flex;
		align-items: center;
		gap: 4px;
		min-width: 0;
		height: 24px;
		padding: 0 4px;
		border: 0;
		border-radius: var(--radius-sm);
		background: none;
		font: inherit;
		font-size: var(--text-small);
		text-align: left;
		color: var(--color-foreground);
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.thing:hover {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}
	.thing.on {
		background: var(--active-bg);
	}
	.thing .title {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.more {
		padding: 0 4px;
		font-size: var(--text-small);
	}
	.note {
		margin: 0;
		font-size: var(--text-small);
		line-height: 20px;
		color: var(--color-foreground-muted);
	}
	@media (prefers-reduced-motion: reduce) {
		.thing {
			transition: none;
		}
	}
</style>
