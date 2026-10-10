<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import Card from '$lib/components/Card.svelte';
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
	 * An applet's Info: how its last 7 days went, then Technical details. Its
	 * switch and schedule are in the page's header, Delete in its More menu. Run history
	 * and Technical details open in place: each is one tap from the page, and
	 * neither holds enough to need a page of its own.
	 */
	let {
		action = $bindable(),
		index,
		daysErr,
		historyOpen = $bindable(false),
		technicalOpen = $bindable(false),
		history,
		technical
	}: {
		action: Applet;
		index: DayIndex;
		daysErr: string | null;
		historyOpen?: boolean;
		technicalOpen?: boolean;
		history: Snippet;
		technical: Snippet;
	} = $props();

	// A dot a day, or a dot a week for an applet that runs weekly or less.
	const spans = $derived(recentSpans(action.schedule));
	const unit = $derived(spanDays(action.schedule) === 7 ? 'weeks' : 'days');
	const states = $derived(appletSpans(action, index, spans));
	const counts = $derived(countRuns(index, action.id, spans.flat()));
	const summary = $derived(recentSummary(states, counts, unit));
	const spanLabel = (span: string[]) => (span.length === 1 ? span[0] : `${span[0]} to ${span.at(-1)}`);
	// Red only for what needs you: a failure, or today's missed run.
	const weekProblem = $derived((counts.get('error') ?? 0) > 0 || states.at(-1) === 'missed');

</script>

<div class="info">
	<section>
		<h3 class="group-head">Run history</h3>
		<Card list>
			<button
				type="button"
				class="row history-row"
				aria-expanded={historyOpen}
				onclick={() => (historyOpen = !historyOpen)}
			>
				{#if daysErr}
					<span class="row-label">Your server couldn't read its recent runs</span>
				{:else}
					<span class="history-body">
						<span class="dots">
							{#each states as state, i (spans[i][0])}
								<DayDot {state} size="md" title={`${spanLabel(spans[i])}: ${DAY_LABEL[state]}`} />
							{/each}
						</span>
						<span class="summary" class:problem={weekProblem}>
							{summary}
						</span>
					</span>
				{/if}
				<Icon icon={historyOpen ? 'ri:arrow-up-s-line' : 'ri:arrow-down-s-line'} width="16" />
			</button>
			{#if historyOpen}
				<div class="opened">{@render history()}</div>
			{/if}
		</Card>
	</section>

	<Card list>
		<button
			type="button"
			class="row"
			aria-expanded={technicalOpen}
			onclick={() => (technicalOpen = !technicalOpen)}
		>
			<span class="row-label">Technical details</span>
			<Icon icon={technicalOpen ? 'ri:arrow-up-s-line' : 'ri:arrow-down-s-line'} width="16" />
		</button>
		{#if technicalOpen}
			<div class="opened">{@render technical()}</div>
		{/if}
	</Card>
</div>


<style>
	.info {
		display: flex;
		flex-direction: column;
		gap: 16px;
	}
	.group-head {
		margin: 0 0 8px;
		font-family: var(--font-sans);
		font-size: 13px;
		font-weight: 500;
		color: var(--color-foreground-muted);
	}
	.row {
		display: flex;
		align-items: center;
		gap: 12px;
		min-height: 48px;
		padding: 8px 16px;
		font-size: 14px;
		color: var(--color-foreground);
	}
	button.row {
		width: 100%;
		border: 0;
		background: none;
		font: inherit;
		font-size: 14px;
		text-align: left;
		cursor: pointer;
		color: var(--color-foreground);
	}
	button.row:hover {
		background: color-mix(in srgb, var(--color-foreground) 4%, transparent);
	}
	.row-label {
		flex: 1;
		min-width: 0;
	}
	.dots {
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.history-row .row-label {
		color: var(--color-foreground-muted);
	}

	.opened {
		padding: 16px;
		border-top: 1px solid var(--color-border);
	}

	.history-body {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 8px 0;
	}
	.summary {
		font-size: 14px;
		color: var(--color-foreground-muted);
	}
	.summary.problem {
		color: var(--color-error);
	}

</style>
