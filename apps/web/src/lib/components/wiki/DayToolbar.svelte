<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { getLocalDateSlug } from "$lib/utils/dateUtils";
	import { computeCompleteness, type ContextVector } from "$lib/wiki";

	interface Props {
		pageDate: Date;
		weekDays: Date[];
		currentDateSlug: string;
		todaySlug: string;
		contextVector: ContextVector | null;
		chaosScore: number | null;
		entropyCalibrationDays: number | null;
		summaryGenerating: boolean;
		summaryExists: boolean;
		onNavigateDay: (date: Date) => void;
		onWeekPrev: () => void;
		onWeekNext: () => void;
		onGenerate: () => void;
		onToggleMetrics: () => void;
	}

	let {
		pageDate,
		weekDays,
		currentDateSlug,
		todaySlug,
		contextVector,
		chaosScore,
		entropyCalibrationDays,
		summaryGenerating,
		summaryExists,
		onNavigateDay,
		onWeekPrev,
		onWeekNext,
		onGenerate,
		onToggleMetrics,
	}: Props = $props();

	const completeness = $derived(
		contextVector ? Math.round(computeCompleteness(contextVector) * 100) : 0,
	);

	const entropyDisplay = $derived(() => {
		if (entropyCalibrationDays == null) return "--";
		const calibDays = entropyCalibrationDays;
		const isCalibrated = calibDays >= 7;
		if (isCalibrated && chaosScore != null) {
			return String(Math.round(chaosScore * 100));
		}
		return `(${calibDays}/7)`;
	});

	const isPast = $derived(currentDateSlug < todaySlug);

	const monthLabel = $derived(() => {
		if (weekDays.length === 0) return "";
		const first = weekDays[0].toLocaleDateString("en-US", { month: "short" });
		const last = weekDays[weekDays.length - 1].toLocaleDateString("en-US", { month: "short" });
		return first === last ? first : `${first}–${last}`;
	});
</script>

<div class="day-toolbar">
	<button class="toolbar-metric-btn" onclick={onToggleMetrics} type="button">
		{completeness}% coverage · Entropy {entropyDisplay()}
	</button>

	<div class="week-picker">
		<button
			class="week-chevron"
			onclick={onWeekPrev}
			type="button"
			aria-label="Previous week"
		>
			<Icon icon="ri:arrow-left-s-line" width="14" />
		</button>
		<span class="week-month">{monthLabel()}</span>
		{#each weekDays as day}
			{@const slug = getLocalDateSlug(day)}
			{@const isCurrent = slug === currentDateSlug}
			{@const isToday = slug === todaySlug}
			<button
				class="week-day"
				class:current={isCurrent}
				class:today={isToday}
				onclick={() => onNavigateDay(day)}
				type="button"
				aria-label={day.toLocaleDateString("en-US", {
					weekday: "long",
					month: "long",
					day: "numeric",
				})}
			>
				{day.getDate()}
			</button>
		{/each}
		<button
			class="week-chevron"
			onclick={onWeekNext}
			type="button"
			aria-label="Next week"
		>
			<Icon icon="ri:arrow-right-s-line" width="14" />
		</button>
	</div>

	<div class="toolbar-right">
		{#if isPast}
			<button
				class="toolbar-generate"
				onclick={onGenerate}
				disabled={summaryGenerating}
				type="button"
			>
				<Icon
					icon={summaryGenerating ? "ri:loader-4-line" : "ri:refresh-line"}
					width="12"
					class={summaryGenerating ? "spin-icon" : ""}
				/>
				{summaryGenerating ? "Generating..." : summaryExists ? "Regenerate" : "Generate"}
			</button>
		{/if}
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

	.week-picker {
		display: flex;
		align-items: center;
		gap: 1px;
	}

	.week-month {
		font-size: 0.6875rem;
		color: var(--color-foreground-muted);
		margin-right: 2px;
		white-space: nowrap;
	}

	.week-chevron {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 22px;
		height: 22px;
		background: none;
		border: none;
		padding: 0;
		color: var(--color-foreground-subtle);
		cursor: pointer;
		border-radius: 4px;
		flex-shrink: 0;
	}
	.week-chevron:hover {
		color: var(--color-foreground-muted);
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
	}

	.week-day {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 24px;
		height: 24px;
		background: none;
		border: none;
		border-radius: 4px;
		cursor: pointer;
		color: var(--color-foreground-subtle);
		font-size: 0.75rem;
		font-weight: 500;
		padding: 0;
		position: relative;
	}
	.week-day:hover {
		color: var(--color-foreground-muted);
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
	}
	.week-day.current {
		color: var(--color-primary);
		font-weight: 600;
	}
	.week-day.today:not(.current) {
		color: var(--color-success, #22c55e);
	}

	.toolbar-metric-btn {
		flex: 1;
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
		white-space: nowrap;
		background: none;
		border: none;
		padding: 2px 4px;
		border-radius: 3px;
		cursor: pointer;
		text-align: left;
	}
	.toolbar-metric-btn:hover {
		color: var(--color-foreground-muted);
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
	}

	.toolbar-right {
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: flex-end;
		min-width: 0;
	}

	.toolbar-generate {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 3px 8px;
		border: none;
		background: none;
		color: var(--color-foreground-muted);
		font-size: 0.6875rem;
		cursor: pointer;
		border-radius: 4px;
		white-space: nowrap;
		transition: all 0.15s ease;
	}

	.toolbar-generate:hover {
		color: var(--color-foreground);
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
	}

	.toolbar-generate:disabled {
		opacity: 0.5;
		cursor: default;
	}

	:global(.spin-icon) {
		animation: spin 1s linear infinite;
	}

	@keyframes spin {
		from {
			transform: rotate(0deg);
		}
		to {
			transform: rotate(360deg);
		}
	}
</style>
