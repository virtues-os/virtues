<script lang="ts">
	import { fade } from "svelte/transition";
	import Icon from "$lib/components/Icon.svelte";
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

	// Calendar popover state
	let calendarOpen = $state(false);
	let calendarMonth = $state(pageDate.getMonth());
	let calendarYear = $state(pageDate.getFullYear());
	let popoverEl = $state<HTMLDivElement | null>(null);
	let calendarBtnEl = $state<HTMLButtonElement | null>(null);

	// Reset calendar view when current date slug changes
	$effect(() => {
		currentDateSlug; // track
		calendarMonth = pageDate.getMonth();
		calendarYear = pageDate.getFullYear();
	});

	function toggleCalendar() {
		calendarOpen = !calendarOpen;
	}

	function closeCalendar() {
		calendarOpen = false;
	}

	// Close on click outside (exclude the toggle button itself)
	function handleWindowClick(e: MouseEvent) {
		const target = e.target as Node;
		if (calendarBtnEl?.contains(target)) return;
		if (popoverEl && !popoverEl.contains(target)) {
			closeCalendar();
		}
	}

	$effect(() => {
		if (calendarOpen) {
			window.addEventListener("click", handleWindowClick, true);
			return () => window.removeEventListener("click", handleWindowClick, true);
		}
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
		closeCalendar();
	}

	const isNotToday = $derived(currentDateSlug !== todaySlug);
</script>

<div class="day-toolbar">
	<div class="toolbar-left">
		<button
			class="nav-btn"
			onclick={() => onNavigateDay(yesterday())}
			type="button"
			aria-label="Previous day"
			title={yesterday().toLocaleDateString("en-US", { weekday: "short", month: "short", day: "numeric" })}
		>
			<Icon icon="ri:arrow-left-s-line" width="16" />
		</button>

		<div class="calendar-anchor">
			<button
				class="nav-btn calendar-btn"
				bind:this={calendarBtnEl}
				onclick={toggleCalendar}
				type="button"
				aria-label="Open date picker"
			>
				<Icon icon="ri:calendar-line" width="15" />
			</button>

			{#if calendarOpen}
				<div class="calendar-popover" bind:this={popoverEl} transition:fade={{ duration: 100 }}>
					<div class="cal-header">
						<button class="cal-nav" onclick={prevMonth} type="button" aria-label="Previous month">
							<Icon icon="ri:arrow-left-s-line" width="14" />
						</button>
						<span class="cal-month-label">{calendarMonthLabel}</span>
						<button class="cal-nav" onclick={nextMonth} type="button" aria-label="Next month">
							<Icon icon="ri:arrow-right-s-line" width="14" />
						</button>
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
			{/if}
		</div>

		{#if isNotToday}
			<button
				class="nav-btn calendar-btn"
				onclick={() => onNavigateDay(new Date())}
				type="button"
				aria-label="Go to today"
				title="Today"
			>
				<Icon icon="ri:calendar-check-line" width="15" />
			</button>
		{/if}

		<button
			class="nav-btn"
			onclick={() => onNavigateDay(tomorrow())}
			type="button"
			aria-label="Next day"
			title={tomorrow().toLocaleDateString("en-US", { weekday: "short", month: "short", day: "numeric" })}
		>
			<Icon icon="ri:arrow-right-s-line" width="16" />
		</button>
	</div>

	<span class="toolbar-date" class:visible={headerScrolledAway}>{shortDateLabel}</span>

	<div class="toolbar-right">
		<button
			class="nav-btn"
			type="button"
			title="Page settings"
			aria-label="Page settings"
			disabled
		>
			<Icon icon="ri:more-2-fill" width="16" />
		</button>
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
		gap: 4px;
	}

	/* Navigation buttons (chevrons + calendar) */
	.nav-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		background: none;
		border: none;
		padding: 0;
		color: var(--color-foreground-subtle);
		cursor: pointer;
		border-radius: 6px;
		flex-shrink: 0;
	}
	.nav-btn:hover {
		color: var(--color-foreground-muted);
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
	}

	.calendar-btn {
		color: var(--color-foreground-muted);
	}

	/* Calendar popover anchor */
	.calendar-anchor {
		position: relative;
	}

	.calendar-popover {
		position: absolute;
		top: calc(100% + 4px);
		left: -4px;
		z-index: 100;
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

	.cal-nav {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 24px;
		height: 24px;
		background: none;
		border: none;
		color: var(--color-foreground-subtle);
		cursor: pointer;
		border-radius: 4px;
		padding: 0;
	}
	.cal-nav:hover {
		color: var(--color-foreground-muted);
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
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
		text-transform: uppercase;
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
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
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

	.nav-btn:disabled {
		opacity: 0.4;
		cursor: default;
	}
</style>
