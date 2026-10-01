<!--
	TimelineMonth.svelte - the date card's month: the month around the day,
	a dot under every day that holds any record (a location fix, a
	transcription window or a step reading). A click opens that day. It
	replaces the week strip.

	The dots will tell an eventful day (dark) from a recorded one (grey) once
	our significance exists; until then every recorded day is grey, never a
	guess at which ones stood out.
-->
<script lang="ts">
	import { untrack } from 'svelte';
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
			<button class="btn" aria-label="Previous month" onclick={() => step(-1)}>‹</button>
			<button class="btn" aria-label="Next month" disabled={month >= today.slice(0, 7)} onclick={() => step(1)}>›</button>
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
			<button class="day" class:cur={slug === date} disabled={slug > today} title={recorded && slug <= today ? (has ? 'Recorded' : 'Nothing recorded') : undefined} onclick={() => onpick(slug)}>
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
	.month {
		position: absolute;
		top: calc(100% + 8px);
		left: 0;
		width: 300px;
		/* design-ok: the mockup's month card, in the Timeline's tile (owner's call, 2026-09-30) */
		padding: 12px;
		/* The Timeline's one material (TimelineView's tile variables). */
		background: var(--tile-bg);
		border: var(--tile-border);
		border-radius: var(--tile-radius);
		/* design-ok: the Timeline follows the Dayback prototype's look (owner's call, 2026-09-30) */
		box-shadow: var(--tile-shadow);
		animation: open 0.28s cubic-bezier(0.2, 0.9, 0.25, 1.08);
		transform-origin: top left;
	}
	@keyframes open {
		from {
			opacity: 0;
			transform: translateY(-6px) scale(0.97);
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.month {
			animation: none;
		}
	}
	.head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		/* design-ok: the mockup's month header inset (owner's call, 2026-09-30) */
		padding: 0 2px 8px 6px;
	}
	.title {
		font-family: var(--font-sans);
		font-size: 14px;
		font-weight: 600;
		color: var(--color-foreground);
	}
	.nav {
		display: flex;
		/* design-ok: the mockup's control spacing (owner's call, 2026-09-30) */
		gap: 4px;
	}
	.btn {
		height: 26px;
		min-width: 26px;
		border: 0;
		border-radius: 999px;
		background: color-mix(in srgb, var(--color-foreground) 7%, transparent);
		color: var(--color-foreground);
		font-family: var(--font-sans);
		font-size: 13px;
		font-weight: 600;
		cursor: pointer;
	}
	.btn:hover:not(:disabled) {
		background: color-mix(in srgb, var(--color-foreground) 12%, transparent);
	}
	.btn:disabled {
		opacity: 0.35;
		cursor: default;
	}
	.grid {
		display: grid;
		grid-template-columns: repeat(7, 1fr);
		/* design-ok: the mockup's month rows (owner's call, 2026-09-30) */
		row-gap: 2px;
		text-align: center;
	}
	.dow {
		font-family: var(--font-sans);
		font-size: 11px;
		font-weight: 600;
		letter-spacing: 0.06em;
		color: var(--color-foreground-subtle);
		/* design-ok: the mockup's weekday row (owner's call, 2026-09-30) */
		padding-bottom: 6px;
	}
	.day {
		border: 0;
		background: none;
		/* design-ok: the mockup's day cell (owner's call, 2026-09-30) */
		padding: 2px 0;
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
		font-weight: 600;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground);
	}
	.day:disabled .num {
		color: var(--color-foreground-subtle);
	}
	.day:not(:disabled):not(.cur):hover .num {
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
	}
	.day.cur .num {
		background: var(--color-foreground);
		color: var(--color-background);
	}
	.dot {
		display: block;
		width: 5px;
		height: 5px;
		/* design-ok: the mockup's day dot (owner's call, 2026-09-30) */
		margin: 2px auto 0;
		border-radius: 50%;
		background: var(--color-foreground-subtle);
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
		width: 6px;
		height: 6px;
		border-radius: 50%;
		/* design-ok: the key's dot sits on the text's midline */
		margin-right: 5px;
		vertical-align: 1px;
		background: var(--color-foreground-subtle);
	}
</style>
