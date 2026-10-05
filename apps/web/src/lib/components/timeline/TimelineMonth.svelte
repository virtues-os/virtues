<!--
	TimelineMonth.svelte - the date card's month: the month around the day,
	a dot under every day with a location fix or a recorded conversation
	(steps alone don't count: the phone's step history runs years past its
	location and audio). A click opens that day.
-->
<script lang="ts">
	import { untrack } from 'svelte';
	import IconButton from '$lib/components/IconButton.svelte';
	import { fetchRecorded } from '$lib/timeline/day';

	let { date, today, onpick }: { date: string; today: string; onpick: (slug: string) => void } = $props();

	/** The month on show, YYYY-MM; it opens on the day's (the month is made
	 *  afresh each time it opens). */
	let month = $state(untrack(() => date.slice(0, 7)));
	let recorded = $state<Set<string> | null>(null);
	let failed = $state(false);

	const DOW = ['S', 'M', 'T', 'W', 'T', 'F', 'S'];
	const pad = (n: number) => String(n).padStart(2, '0');

	const days = $derived.by(() => {
		const [y, m] = month.split('-').map(Number);
		const count = new Date(y, m, 0).getDate();
		return Array.from({ length: count }, (_, i) => `${month}-${pad(i + 1)}`);
	});
	const lead = $derived.by(() => {
		const [y, m] = month.split('-').map(Number);
		return new Date(y, m - 1, 1).getDay();
	});
	const title = $derived.by(() => {
		const [y, m] = month.split('-').map(Number);
		return new Date(y, m - 1, 1).toLocaleDateString('en-US', { month: 'long', year: 'numeric' });
	});

	// The month's recorded days, asked for again whenever the month changes.
	let asked = 0;
	$effect(() => {
		const from = days[0];
		const to = days[days.length - 1];
		const mine = ++asked;
		recorded = null;
		failed = false;
		fetchRecorded(from, to < today ? to : today)
			.then((ds) => {
				if (mine === asked) recorded = new Set(ds);
			})
			.catch((e) => {
				console.warn('[Timeline] recorded days fetch failed', e);
				if (mine === asked) failed = true;
			});
	});

	function step(dir: 1 | -1) {
		const [y, m] = month.split('-').map(Number);
		const t = new Date(y, m - 1 + dir, 1);
		month = `${t.getFullYear()}-${pad(t.getMonth() + 1)}`;
	}
</script>

<div class="month" role="dialog" aria-label="Pick a day">
	<div class="head">
		<span class="title">{title}</span>
		<span class="nav">
			<IconButton icon="ri:arrow-left-s-line" label="Previous month" size="sm" onclick={() => step(-1)} />
			<IconButton icon="ri:arrow-right-s-line" label="Next month" size="sm" disabled={month >= today.slice(0, 7)} onclick={() => step(1)} />
		</span>
	</div>
	<div class="grid">
		{#each DOW as d, i (i)}
			<span class="dow">{d}</span>
		{/each}
		{#each { length: lead } as _, i (i)}
			<span></span>
		{/each}
		{#each days as slug (slug)}
			{@const has = recorded?.has(slug) ?? false}
			<button class="day" class:cur={slug === date} disabled={slug > today} title={recorded && slug <= today ? (has ? 'Recorded' : 'No location or audio') : undefined} onclick={() => onpick(slug)}>
				<span class="num">{Number(slug.slice(8))}</span>
				<span class="dot" class:on={has}></span>
			</button>
		{/each}
	</div>
	{#if failed}
		<p class="note">Your server couldn't say which days hold a record. Reload the page to try again.</p>
	{:else}
		<p class="key"><i></i>recorded</p>
	{/if}
</div>

<style>
	/* The page's surface in a hairline, like every card over the map. */
	.month {
		position: absolute;
		top: calc(100% + 8px);
		left: 0;
		width: 300px;
		padding: 12px;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: var(--tile-radius);
	}
	.head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0 0 8px 8px;
	}
	.title {
		font-family: var(--font-serif-ui);
		font-size: 18px;
		color: var(--color-foreground);
	}
	.nav {
		display: flex;
		gap: 4px;
	}
	.grid {
		display: grid;
		grid-template-columns: repeat(7, 1fr);
		row-gap: 4px;
		text-align: center;
	}
	.dow {
		font-family: var(--font-sans);
		font-size: 11px;
		color: var(--color-foreground-subtle);
		padding-bottom: 4px;
	}
	.day {
		border: 0;
		background: none;
		padding: 0;
		cursor: pointer;
	}
	.day:disabled {
		cursor: default;
	}
	.num {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		margin: 0 auto;
		border-radius: 50%;
		font-family: var(--font-sans);
		font-size: 13px;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground);
	}
	.day:disabled .num {
		color: var(--color-foreground-subtle);
	}
	.day:not(:disabled):not(.cur):hover .num {
		background: var(--hover-bg);
	}
	/* The day on screen is the one you are on now: the accent. */
	.day.cur .num {
		background: var(--color-primary);
		color: var(--color-background);
	}
	.dot {
		display: block;
		width: 4px;
		height: 4px;
		margin: 4px auto 0;
		border-radius: 50%;
		background: var(--color-foreground-muted);
		visibility: hidden;
	}
	.dot.on {
		visibility: visible;
	}
	.key,
	.note {
		margin: 8px 0 0;
		font-family: var(--font-sans);
		font-size: 12px;
		color: var(--color-foreground-muted);
		text-align: center;
	}
	.key i {
		display: inline-block;
		width: 4px;
		height: 4px;
		border-radius: 50%;
		margin-right: 4px;
		vertical-align: 2px;
		background: var(--color-foreground-muted);
	}
</style>
