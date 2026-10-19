<!--
	DayDatePicker.svelte

	The day page's date is its own way to another day: click it and a month
	opens under it. This and the days named at the foot of the page are how
	you move between days.
-->
<script lang="ts">
	import type { Snippet } from "svelte";
	import IconButton from "$lib/components/IconButton.svelte";
	import { Popover } from "$lib/floating";
	import { getLocalDateSlug } from "$lib/utils/dateUtils";

	interface Props {
		pageDate: Date;
		currentDateSlug: string;
		todaySlug: string;
		onNavigateDay: (date: Date) => void;
		/** What the trigger shows: the page's title. */
		label: Snippet;
		/** Hover text for the trigger. */
		title?: string;
	}

	let { pageDate, currentDateSlug, todaySlug, onNavigateDay, label, title }: Props = $props();

	let open = $state(false);
	let month = $state(pageDate.getMonth());
	let year = $state(pageDate.getFullYear());

	$effect(() => {
		currentDateSlug;
		month = pageDate.getMonth();
		year = pageDate.getFullYear();
	});

	const cells = $derived.by(() => {
		const lead = new Date(year, month, 1).getDay();
		const days = new Date(year, month + 1, 0).getDate();
		const out: (Date | null)[] = Array.from({ length: lead }, () => null);
		for (let d = 1; d <= days; d++) out.push(new Date(year, month, d));
		return out;
	});

	const monthLabel = $derived(
		new Date(year, month).toLocaleDateString("en-US", { month: "long", year: "numeric" }),
	);

	function shift(by: number) {
		const d = new Date(year, month + by, 1);
		month = d.getMonth();
		year = d.getFullYear();
	}

	function pick(d: Date) {
		open = false;
		onNavigateDay(d);
	}
</script>

<Popover bind:open placement="bottom-start" offset={6}>
	{#snippet trigger({ toggle })}
		<button type="button" class="trigger" {title} aria-haspopup="dialog" aria-expanded={open} onclick={toggle}>
			{@render label()}
		</button>
	{/snippet}
	{#snippet children()}
		<div class="calendar" role="dialog" aria-label="Go to a day">
			<div class="head">
				<IconButton icon="ri:arrow-left-s-line" label="Previous month" size="sm" onclick={() => shift(-1)} />
				<span class="month">{monthLabel}</span>
				<IconButton icon="ri:arrow-right-s-line" label="Next month" size="sm" onclick={() => shift(1)} />
			</div>
			<div class="dow">
				{#each ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"] as d (d)}<span>{d}</span>{/each}
			</div>
			<div class="grid">
				{#each cells as cell, i (i)}
					{#if cell === null}
						<span></span>
					{:else}
						{@const slug = getLocalDateSlug(cell)}
						<button
							type="button"
							class="day"
							class:current={slug === currentDateSlug}
							class:today={slug === todaySlug}
							onclick={() => pick(cell)}
						>
							{cell.getDate()}
						</button>
					{/if}
				{/each}
			</div>
			{#if currentDateSlug !== todaySlug}
				<button type="button" class="to-today" onclick={() => pick(new Date())}>Today</button>
			{/if}
		</div>
	{/snippet}
</Popover>

<style>
	.trigger {
		all: unset;
		cursor: pointer;
		border-radius: 6px;
	}

	.trigger:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 4px;
	}

	.calendar {
		width: 15rem;
		padding: 0.625rem;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 12px;
		font-family: var(--font-sans);
	}

	.head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 0.5rem;
	}

	.month {
		font-size: 0.8125rem;
		color: var(--color-foreground);
	}

	.dow,
	.grid {
		display: grid;
		grid-template-columns: repeat(7, 1fr);
		gap: 1px;
	}

	.dow span {
		text-align: center;
		font-size: 0.625rem;
		color: var(--color-foreground-subtle);
		padding: 2px 0;
	}

	.day {
		aspect-ratio: 1;
		border: none;
		background: none;
		border-radius: 6px;
		padding: 0;
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
		cursor: pointer;
	}

	.day:hover {
		background: var(--hover-bg);
		color: var(--color-foreground);
	}

	.day.today:not(.current) {
		color: var(--color-primary);
	}

	.day.current {
		background: var(--color-primary);
		color: var(--color-background);
	}

	.to-today {
		display: block;
		width: 100%;
		margin-top: 0.5rem;
		padding: 0.375rem 0;
		border: none;
		border-top: 1px solid var(--color-border-subtle);
		background: none;
		font-size: 0.75rem;
		color: var(--color-primary);
		cursor: pointer;
	}
</style>
