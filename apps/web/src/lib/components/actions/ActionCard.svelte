<script lang="ts">
	import type { Action, ActionRun } from '$lib/api/client';
	import { describeSchedule, relativeTime } from '$lib/actions/palette';
	import { descriptionFor } from '$lib/actions/descriptions';

	let {
		action,
		lastRun,
		lastSuccess,
		pulseRuns = [],
		onclick
	}: {
		action: Action;
		lastRun?: ActionRun | Action['last_run'] | null;
		lastSuccess?: ActionRun | null;
		pulseRuns?: ActionRun[];
		onclick?: (action: Action) => void;
	} = $props();

	const schedule = $derived(describeSchedule(action.cron_schedule));
	const isUserOwned = $derived(action.owner === 'user');

	const lastStatus = $derived((lastRun as { status?: string } | null)?.status ?? null);
	const isFailing = $derived(lastStatus === 'error');

	// Excerpt resolution — always prefer a real successful output, never an error.
	// Falls back to the action's description (self-introduction).
	const excerpt = $derived.by(() => {
		const success = lastSuccess?.result_summary;
		if (success) return { text: success, kind: 'output' as const };
		const desc = descriptionFor(action);
		if (desc) return { text: desc, kind: 'description' as const };
		return null;
	});

	const pulseDots = $derived.by(() => {
		const slots = 10;
		const arr = pulseRuns.slice(0, slots).reverse();
		const empty = slots - arr.length;
		return [
			...Array.from({ length: empty }, () => null),
			...arr.map((r) => r.status)
		];
	});

	const pulseLabel = $derived.by(() => {
		if (pulseRuns.length === 0) return 'No recent runs';
		const counts = { success: 0, error: 0, skipped: 0, running: 0, cancelled: 0 };
		for (const r of pulseRuns) {
			if (r.status in counts) counts[r.status as keyof typeof counts]++;
		}
		const parts: string[] = [];
		if (counts.success) parts.push(`${counts.success} succeeded`);
		if (counts.error) parts.push(`${counts.error} failed`);
		if (counts.skipped) parts.push(`${counts.skipped} skipped`);
		if (counts.running) parts.push(`${counts.running} running`);
		return `Last ${pulseRuns.length} runs: ${parts.join(', ') || 'none completed'}`;
	});

	function handleClick() {
		onclick?.(action);
	}
</script>

<button
	type="button"
	class="action-card"
	class:disabled={!action.enabled}
	onclick={handleClick}
>
	<div class="meta-col">
		<h3 class="name">{action.name}</h3>

		<div class="meta">
			<span>{schedule}</span>
			{#if lastRun?.started_at}
				<span class="dot-sep">·</span>
				<span>{relativeTime(lastRun.started_at)}</span>
			{/if}
			{#if isFailing}
				<span class="fail-pip" title="Last run failed" aria-label="Last run failed"></span>
			{/if}
		</div>

		<div class="pulse" role="img" aria-label={pulseLabel}>
			{#each pulseDots as status}
				<span class="dot" data-status={status ?? 'empty'} aria-hidden="true"></span>
			{/each}
		</div>
	</div>

	<div class="excerpt-col">
		{#if excerpt}
			<p
				class="excerpt"
				class:agent={isUserOwned && excerpt.kind === 'output'}
				class:description={excerpt.kind === 'description'}
				class:output={excerpt.kind === 'output'}
			>
				{excerpt.text}
			</p>
		{:else}
			<p class="excerpt placeholder">Hasn't produced output yet</p>
		{/if}
	</div>
</button>

<style>
	.action-card {
		display: grid;
		grid-template-columns: minmax(0, 0.9fr) minmax(0, 1.1fr);
		gap: 0.875rem;
		padding: 0.875rem 1rem;
		border-radius: 8px;
		border: 1px solid var(--color-border, #e5e7eb);
		background: var(--color-surface, #fff);
		color: var(--color-foreground, inherit);
		text-align: left;
		font: inherit;
		cursor: pointer;
		min-height: 120px;
	}
	.action-card:hover {
		background: var(--color-surface-elevated, #f9fafb);
	}
	.action-card:focus-visible {
		outline: 2px solid var(--color-primary, #4338ca);
		outline-offset: 1px;
	}
	.action-card.disabled {
		opacity: 0.55;
	}

	.meta-col {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		min-width: 0;
	}
	.name {
		margin: 0;
		font-size: 0.9375rem;
		font-weight: 600;
		line-height: 1.3;
		display: -webkit-box;
		line-clamp: 2;
		-webkit-line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}
	.meta {
		display: flex;
		align-items: center;
		gap: 0.375rem;
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle, #9ca3af);
		flex-wrap: wrap;
	}
	.dot-sep {
		opacity: 0.5;
	}
	.fail-pip {
		display: inline-block;
		width: 6px;
		height: 6px;
		border-radius: 999px;
		background: #ef4444;
		margin-left: 0.25rem;
	}

	.pulse {
		display: flex;
		gap: 3px;
		margin-top: auto;
	}
	.dot {
		width: 6px;
		height: 6px;
		border-radius: 999px;
		background: var(--color-surface-elevated, #f3f4f6);
		border: 1px solid var(--color-border, #e5e7eb);
	}
	.dot[data-status='success'] {
		background: #22c55e;
		border-color: #16a34a;
	}
	.dot[data-status='error'] {
		background: #ef4444;
		border-color: #b91c1c;
	}
	.dot[data-status='skipped'] {
		background: #d1d5db;
		border-color: #9ca3af;
	}
	.dot[data-status='running'] {
		background: #fbbf24;
		border-color: #d97706;
	}
	.dot[data-status='cancelled'] {
		background: #fef3c7;
		border-color: #d97706;
	}
	.dot[data-status='empty'] {
		background: transparent;
	}

	.excerpt-col {
		border-left: 1px solid var(--color-border-subtle, #f3f4f6);
		padding-left: 0.875rem;
		min-width: 0;
		display: flex;
		align-items: flex-start;
	}
	.excerpt {
		margin: 0;
		font-size: 0.75rem;
		line-height: 1.55;
		color: var(--color-foreground-muted, #6b7280);
		display: -webkit-box;
		line-clamp: 4;
		-webkit-line-clamp: 4;
		-webkit-box-orient: vertical;
		overflow: hidden;
		mask-image: linear-gradient(to bottom, black 70%, transparent 100%);
	}
	.excerpt.agent {
		font-family: var(--font-serif, Georgia, 'Times New Roman', serif);
		font-style: italic;
		font-size: 0.8125rem;
		color: var(--color-foreground, #1f2937);
	}
	.excerpt.description {
		font-family: var(--font-serif, Georgia, 'Times New Roman', serif);
		font-style: italic;
		color: var(--color-foreground-muted, #6b7280);
	}
	.excerpt.output:not(.agent) {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 0.6875rem;
	}
	.excerpt.placeholder {
		font-style: italic;
		opacity: 0.5;
	}
</style>
