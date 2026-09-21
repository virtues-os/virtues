<script lang="ts">
	import IconButton from "$lib/components/IconButton.svelte";
	import Popover from "$lib/floating/primitives/Popover.svelte";
	import { getLocalDateSlug } from "$lib/utils/dateUtils";

	interface Props {
		pageDate: Date;
		currentDateSlug: string;
		todaySlug: string;
		onNavigateDay: (date: Date) => void;
		headerScrolledAway?: boolean;
	}

	let {
		pageDate,
		currentDateSlug,
		todaySlug,
		onNavigateDay,
		headerScrolledAway = false,
	}: Props = $props();

	const shortDateLabel = $derived(
		pageDate.toLocaleDateString("en-US", {
			weekday: "short",
			month: "short",
			day: "numeric",
			year: "numeric",
		}),
	);

	// Yesterday / Tomorrow dates
	const yesterday = $derived(() => {
		const d = new Date(pageDate);
		d.setDate(d.getDate() - 1);
		return d;
	});

	const tomorrow = $derived(() => {
		const d = new Date(pageDate);
		d.setDate(d.getDate() + 1);
		return d;
	});

	// Calendar popover state — positioning + click-outside + ESC handled
	// by the Popover primitive.
	let calendarOpen = $state(false);
	let calendarMonth = $state(pageDate.getMonth());
	let calendarYear = $state(pageDate.getFullYear());

	// Reset calendar view when current date slug changes
	$effect(() => {
		currentDateSlug; // track
		calendarMonth = pageDate.getMonth();
		calendarYear = pageDate.getFullYear();
	});

	// Calendar grid computation
	const calendarDays = $derived(() => {
		const firstDay = new Date(calendarYear, calendarMonth, 1);
		const startDow = firstDay.getDay(); // 0=Sun
		const daysInMonth = new Date(calendarYear, calendarMonth + 1, 0).getDate();

		const cells: (Date | null)[] = [];
		// Leading blanks
		for (let i = 0; i < startDow; i++) cells.push(null);
		// Days
		for (let d = 1; d <= daysInMonth; d++) {
			cells.push(new Date(calendarYear, calendarMonth, d));
		}
		return cells;
	});

	const calendarMonthLabel = $derived(
		new Date(calendarYear, calendarMonth).toLocaleDateString("en-US", {
			month: "long",
			year: "numeric",
		}),
	);

	function prevMonth() {
		if (calendarMonth === 0) {
			calendarMonth = 11;
			calendarYear--;
		} else {
			calendarMonth--;
		}
	}

	function nextMonth() {
		if (calendarMonth === 11) {
			calendarMonth = 0;
			calendarYear++;
		} else {
			calendarMonth++;
		}
	}

	function selectCalendarDay(date: Date) {
		onNavigateDay(date);
		calendarOpen = false;
	}

	const isNotToday = $derived(currentDateSlug !== todaySlug);
</script>

<div class="day-toolbar">
	<div class="toolbar-left">
		<IconButton
			icon="ri:arrow-left-s-line"
			label="Previous day"
			onclick={() => onNavigateDay(yesterday())}
		/>

		<Popover bind:open={calendarOpen} placement="bottom-start" offset={4}>
			{#snippet trigger({ toggle })}
				<IconButton
					icon="ri:calendar-line"
					label="Open date picker"
					expanded={calendarOpen}
					haspopup="dialog"
					onclick={toggle}
				/>
			{/snippet}
			{#snippet children()}
				<div class="calendar-popover">
					<div class="cal-header">
						<IconButton
							icon="ri:arrow-left-s-line"
							label="Previous month"
							size="sm"
							onclick={prevMonth}
						/>
						<span class="cal-month-label">{calendarMonthLabel}</span>
						<IconButton
							icon="ri:arrow-right-s-line"
							label="Next month"
							size="sm"
							onclick={nextMonth}
						/>
					</div>
					<div class="cal-dow-row">
						{#each ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"] as dow}
							<span class="cal-dow">{dow}</span>
						{/each}
					</div>
					<div class="cal-grid">
						{#each calendarDays() as cell}
							{#if cell === null}
								<span class="cal-cell cal-blank"></span>
							{:else}
								{@const slug = getLocalDateSlug(cell)}
								{@const isCurrent = slug === currentDateSlug}
								{@const isToday = slug === todaySlug}
								<button
									class="cal-cell cal-day"
									class:current={isCurrent}
									class:today={isToday}
									onclick={() => selectCalendarDay(cell)}
									type="button"
								>
									{cell.getDate()}
								</button>
							{/if}
						{/each}
					</div>
				</div>
			{/snippet}
		</Popover>

		{#if isNotToday}
			<IconButton
				icon="ri:calendar-check-line"
				label="Go to today"
				onclick={() => onNavigateDay(new Date())}
			/>
		{/if}

		<IconButton
			icon="ri:arrow-right-s-line"
			label="Next day"
			onclick={() => onNavigateDay(tomorrow())}
		/>
	</div>

	<span class="toolbar-date" class:visible={headerScrolledAway}>{shortDateLabel}</span>

	<div class="toolbar-right">
		<IconButton icon="ri:more-2-fill" label="Page settings" disabled />
	</div>
</div>

<style>
	.day-toolbar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 8px 12px;
		background: var(--color-background);
		border-bottom: 1px solid var(--color-border);
		flex-shrink: 0;
	}

	.toolbar-left {
		display: flex;
		align-items: center;
		gap: 2px;
	}

	.toolbar-date {
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
		opacity: 0;
		transition: opacity 0.2s ease;
		pointer-events: none;
		white-space: nowrap;
	}
	.toolbar-date.visible {
		opacity: 1;
	}

	.toolbar-right {
		display: flex;
		align-items: center;
		gap: 6px;
	}

	/* Calendar popover (positioning handled by the floating primitive) */
	.calendar-popover {
		background: var(--color-background);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.12);
		padding: 10px;
		width: 240px;
	}

	.cal-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 8px;
	}

	.cal-month-label {
		font-size: 0.8125rem;
		font-weight: 600;
		color: var(--color-foreground);
	}

	.cal-dow-row {
		display: grid;
		grid-template-columns: repeat(7, 1fr);
		gap: 0;
		margin-bottom: 2px;
	}

	.cal-dow {
		text-align: center;
		font-size: 0.625rem;
		font-weight: 500;
		color: var(--color-foreground-subtle);
		padding: 2px 0;
		letter-spacing: 0.03em;
	}

	.cal-grid {
		display: grid;
		grid-template-columns: repeat(7, 1fr);
		gap: 1px;
	}

	.cal-cell {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 100%;
		aspect-ratio: 1;
		font-size: 0.75rem;
		border-radius: 6px;
	}

	.cal-blank {
		background: none;
	}

	.cal-day {
		background: none;
		border: none;
		color: var(--color-foreground-muted);
		cursor: pointer;
		font-weight: 400;
		padding: 0;
	}
	.cal-day:hover {
		background: var(--hover-bg);
		color: var(--color-foreground);
	}
	.cal-day.current {
		background: var(--color-primary);
		color: white;
		font-weight: 600;
	}
	.cal-day.today:not(.current) {
		color: var(--color-success, #22c55e);
		font-weight: 600;
	}

</style>
