<script lang="ts">
	/**
	 * A thread's week as seven small bars in its color: solid for time spent,
	 * a tint for time ahead, with each day's letter under its bar and the
	 * week's total under the chart. The preview column and the thread's page
	 * both draw it.
	 */
	import type { ThreadWeekDay } from '$lib/api/client';

	let {
		days,
		today,
		color,
	}: {
		days: ThreadWeekDay[];
		/** The owner's today, `YYYY-MM-DD`, from the week the days came in. */
		today: string | null;
		color: string;
	} = $props();

	/** The days can be another week's; the heading says which. */
	const isThisWeek = $derived(!!today && days.length > 0 && today >= days[0].date && today <= days[days.length - 1].date);
	const heading = $derived.by(() => {
		if (isThisWeek || days.length === 0) return 'This week';
		const d = new Date(`${days[0].date}T12:00:00`);
		return `Week of ${d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' })}`;
	});
	/** The bars' full height stands for the week's busiest day, and never less than two hours. */
	const top = $derived(Math.max(120, ...days.map((d) => d.done_minutes + d.ahead_minutes)));

	function hours(minutes: number): string {
		const h = Math.floor(minutes / 60);
		const m = minutes % 60;
		if (h === 0) return `${m} m`;
		return m === 0 ? `${h} h` : `${h} h ${m} m`;
	}

	const line = $derived.by(() => {
		const done = days.reduce((n, d) => n + d.done_minutes, 0);
		const ahead = days.reduce((n, d) => n + d.ahead_minutes, 0);
		const parts = [done > 0 ? `${hours(done)} spent` : null, ahead > 0 ? `${hours(ahead)} ahead` : null].filter(Boolean);
		return parts.length ? parts.join(', ') : isThisWeek ? 'Nothing on it this week' : 'Nothing on it that week';
	});

	function weekday(iso: string, style: 'narrow' | 'short'): string {
		return new Date(`${iso}T12:00:00`).toLocaleDateString('en-US', { weekday: style });
	}

	/** "Tue · 1 h 15 m spent, 30 m ahead", for a pointer resting on a day. */
	function dayTip(d: ThreadWeekDay): string {
		const parts = [
			d.done_minutes ? `${hours(d.done_minutes)} spent` : null,
			d.ahead_minutes ? `${hours(d.ahead_minutes)} ahead` : null,
		].filter(Boolean);
		return `${weekday(d.date, 'short')} · ${parts.length ? parts.join(', ') : 'nothing'}`;
	}
</script>

<section aria-label={heading}>
	<h3 class="label">{heading}</h3>
	<div class="bars" style={`--thread: ${color}`}>
		{#each days as d (d.date)}
			{@const total = d.done_minutes + d.ahead_minutes}
			<div class="day" class:today={d.date === today} data-tip={dayTip(d)}>
				<div class="track">
					{#if total > 0}
						{#if d.ahead_minutes > 0}
							<span class="bar ahead" style={`height: ${(d.ahead_minutes / top) * 100}%`}></span>
						{/if}
						{#if d.done_minutes > 0}
							<span class="bar done" style={`height: ${(d.done_minutes / top) * 100}%`}></span>
						{/if}
					{:else}
						<span class="floor"></span>
					{/if}
				</div>
				<span class="wd">{weekday(d.date, 'narrow')}</span>
			</div>
		{/each}
	</div>
	<p class="sum">{line}</p>
</section>

<style>
	.label {
		margin: 0 0 8px;
		font-size: var(--text-label);
		font-weight: 500;
		line-height: 20px;
		color: var(--color-foreground-muted);
	}
	/* A quiet day keeps a short floor so the seven days still read as a week. */
	.bars {
		display: grid;
		grid-template-columns: repeat(7, 1fr);
		gap: 4px;
	}
	.day {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 4px;
	}
	.track {
		display: flex;
		flex-direction: column;
		justify-content: flex-end;
		gap: 4px;
		width: 16px;
		height: 48px;
	}
	.bar {
		display: block;
		width: 100%;
		min-height: 4px;
		border-radius: var(--radius-sm);
	}
	.bar.done {
		background: var(--thread);
	}
	.bar.ahead {
		background: color-mix(in srgb, var(--thread) 40%, transparent);
	}
	.floor {
		display: block;
		width: 100%;
		height: 2px;
		border-radius: var(--radius-full);
		background: var(--color-border);
	}
	.wd {
		font-size: var(--text-caption);
		line-height: 16px;
		color: var(--color-foreground-subtle);
	}
	.today .wd {
		color: var(--color-primary);
		font-weight: 500;
	}
	.sum {
		margin: 8px 0 0;
		font-size: var(--text-caption);
		line-height: 16px;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
	}
</style>
