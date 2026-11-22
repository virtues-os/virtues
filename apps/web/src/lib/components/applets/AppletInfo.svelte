<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import Card from '$lib/components/Card.svelte';
	import type { Applet } from '$lib/api/client';
	import { problemOf } from '$lib/applets/standing';
	import { whenInWords } from '$lib/utils/dateUtils';

	/**
	 * An applet's Info: how its last run went, then Technical details. Its
	 * switch and schedule are in the page's header, Delete in its More menu. Run history
	 * and Technical details open in place: each is one tap from the page, and
	 * neither holds enough to need a page of its own.
	 */
	let {
		action = $bindable(),
		historyOpen = $bindable(false),
		technicalOpen = $bindable(false),
		history,
		technical
	}: {
		action: Applet;
		historyOpen?: boolean;
		technicalOpen?: boolean;
		history: Snippet;
		technical: Snippet;
	} = $props();

	// How its last run went, in one sentence. The runs themselves are a list
	// under it: no count of them and no picture of days, since a row of
	// marked days reads as a streak to someone who is counting days.
	const lastRun = $derived.by(() => {
		const trouble = problemOf(action);
		if (trouble) return `${trouble}.`;
		const run = action.last_run;
		if (run?.status === 'running') return 'Running now.';
		if (!run?.started_at) return "Hasn't run yet.";
		const when = whenInWords(run.started_at);
		if (run.status === 'error') return `Its last run, ${when}, didn't finish.`;
		if (run.status === 'budget_exceeded') return `Stopped at its spending limit ${when}.`;
		return `Last ran ${when}.`;
	});
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
				<span class="row-label">{lastRun}</span>
				<Icon icon={historyOpen ? 'mynaui:chevron-up' : 'mynaui:chevron-down'} width="16" />
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
			<Icon icon={technicalOpen ? 'mynaui:chevron-up' : 'mynaui:chevron-down'} width="16" />
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
	.history-row .row-label {
		color: var(--color-foreground-muted);
	}

	.opened {
		padding: 16px;
		border-top: 1px solid var(--color-border);
	}

</style>
