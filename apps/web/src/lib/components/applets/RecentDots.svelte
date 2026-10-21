<script lang="ts">
	import DayDot from './DayDot.svelte';
	import type { Applet } from '$lib/api/client';
	import {
		appletSpans,
		countRuns,
		DAY_LABEL,
		recentSpans,
		recentSummary,
		spanDays,
		type DayIndex
	} from '$lib/applets/days';

	/**
	 * Seven dots and one sentence for how an applet's recent runs went: a dot
	 * a day, or a dot a week for an applet that runs weekly or less. Info and
	 * the applet's inspector both show it.
	 */
	let { action, index }: { action: Applet; index: DayIndex } = $props();

	const spans = $derived(recentSpans(action.schedule));
	const unit = $derived(spanDays(action.schedule) === 7 ? 'weeks' : 'days');
	const states = $derived(appletSpans(action, index, spans));
	const counts = $derived(countRuns(index, action.id, spans.flat()));
	const summary = $derived(recentSummary(states, counts, unit));
	const spanLabel = (span: string[]) => (span.length === 1 ? span[0] : `${span[0]} to ${span.at(-1)}`);
	// Red only for what needs you: a failure, or a missed run now.
	const problem = $derived((counts.get('error') ?? 0) > 0 || states.at(-1) === 'missed');
</script>

<span class="recent">
	<span class="dots">
		{#each states as state, i (spans[i][0])}
			<DayDot {state} size="md" title={`${spanLabel(spans[i])}: ${DAY_LABEL[state]}`} />
		{/each}
	</span>
	<span class="summary" class:problem>{summary}</span>
</span>

<style>
	.recent {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.dots {
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.summary {
		font-size: 14px;
		color: var(--color-foreground-muted);
	}
	.summary.problem {
		color: var(--color-error);
	}
</style>
