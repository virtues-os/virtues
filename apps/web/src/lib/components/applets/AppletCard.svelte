<script lang="ts">
	import type { Applet, AppletRun } from '$lib/api/client';
	import { describeSchedule, relativeTime } from '$lib/applets/palette';

	let {
		applet,
		lastRun,
		lastSuccess,
		pulseRuns = [],
		onclick
	}: {
		applet: Applet;
		lastRun?: AppletRun | Applet['last_run'] | null;
		lastSuccess?: AppletRun | null;
		pulseRuns?: AppletRun[];
		onclick?: (applet: Applet) => void;
	} = $props();

	const schedule = $derived(describeSchedule(applet.cron_schedule));
	const isUserOwned = $derived(applet.owner === 'user');

	const lastStatus = $derived((lastRun as { status?: string } | null)?.status ?? null);
	const isFailing = $derived(lastStatus === 'error');

	// The right column is the applet's own last words — never an error, always
	// a real successful output. What it IS lives on the left now, as its own
	// line, rather than standing in here when there was no output yet.
	const excerpt = $derived(lastSuccess?.result_summary ?? null);

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
		const counts = {
			success: 0,
			error: 0,
			skipped: 0,
			running: 0,
			cancelled: 0,
			budget_exceeded: 0
		};
		for (const r of pulseRuns) {
			if (r.status in counts) counts[r.status as keyof typeof counts]++;
		}
		const parts: string[] = [];
		if (counts.success) parts.push(`${counts.success} succeeded`);
		if (counts.error) parts.push(`${counts.error} failed`);
		if (counts.skipped) parts.push(`${counts.skipped} skipped`);
		if (counts.running) parts.push(`${counts.running} running`);
		if (counts.budget_exceeded) parts.push(`${counts.budget_exceeded} stopped on budget`);
		return `Last ${pulseRuns.length} runs: ${parts.join(', ') || 'none completed'}`;
	});

	function handleClick() {
		onclick?.(applet);
	}
</script>

<button
	type="button"
	class="applet-card"
	class:disabled={!applet.enabled}
	onclick={handleClick}
>
	<div class="meta-col">
		<div class="name-row">
			<h3 class="name">{applet.name}</h3>
			{#if !applet.enabled}
				<!-- Explicit, because dimming alone reads as "loading" rather than
				     "you turned this off". -->
				<span class="off-pill">off</span>
			{/if}
		</div>

		{#if applet.description}
			<p class="line">{applet.description}</p>
		{/if}

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
			<p class="excerpt" class:agent={isUserOwned}>{excerpt}</p>
		{:else}
			<p class="excerpt placeholder">Hasn't produced anything yet</p>
		{/if}
	</div>
</button>

<style>
	.applet-card {
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
	.applet-card:hover {
		background: var(--color-surface-elevated, #f9fafb);
	}
	.applet-card:focus-visible {
		outline: 2px solid var(--color-primary, #4338ca);
		outline-offset: 1px;
	}
	.applet-card.disabled {
		opacity: 0.55;
	}

	.meta-col {
		display: flex;
		flex-direction: column;
		gap: 0.375rem;
		min-width: 0;
	}
	.name-row {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		min-width: 0;
	}
	.off-pill {
		flex-shrink: 0;
		padding: 0.05rem 0.375rem;
		border-radius: 999px;
		border: 1px solid var(--color-border);
		background: var(--color-surface-elevated);
		color: var(--color-foreground-subtle);
		font-size: 0.625rem;
		letter-spacing: 0.02em;
	}
	/* The plain-English line the plan asks the row to carry. */
	.line {
		margin: 0;
		font-family: var(--font-serif, Georgia, serif);
		font-size: 0.75rem;
		line-height: 1.45;
		color: var(--color-foreground-muted);
		display: -webkit-box;
		line-clamp: 2;
		-webkit-line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}
	.name {
		margin: 0;
		font-size: 0.9375rem;
		font-weight: 600;
		line-height: 1.3;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
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
		border-radius: var(--radius-full);
		background: var(--color-error);
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
		border-radius: var(--radius-full);
		background: var(--color-surface-elevated, #f3f4f6);
		border: 1px solid var(--color-border, #e5e7eb);
	}
	.dot[data-status='success'] {
		background: var(--color-success);
		border-color: color-mix(in srgb, var(--color-success) 75%, #000);
	}
	.dot[data-status='error'] {
		background: var(--color-error);
		border-color: color-mix(in srgb, var(--color-error) 75%, #000);
	}
	.dot[data-status='skipped'] {
		background: var(--color-border);
		border-color: var(--color-foreground-subtle);
	}
	.dot[data-status='running'] {
		background: var(--color-warning);
		border-color: color-mix(in srgb, var(--color-warning) 75%, #000);
	}
	.dot[data-status='cancelled'] {
		background: var(--color-warning-subtle);
		border-color: var(--color-warning);
	}
	/* Stopped at a spend ceiling — deliberately not the error red. The run
	   did what it was told to do; the pulse should read as "held", not
	   "broken". */
	.dot[data-status='budget_exceeded'] {
		background: var(--color-warning-subtle);
		border-color: var(--color-warning);
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
	.excerpt:not(.agent) {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 0.6875rem;
	}
	/* Not mono — the placeholder is prose about the applet, not its output. */
	.excerpt.placeholder {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 0.75rem;
		font-style: italic;
		opacity: 0.5;
	}
</style>
