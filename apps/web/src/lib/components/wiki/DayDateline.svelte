<!--
	DayDateline.svelte

	The line above a day's title: the weekday, the weather, and how much of
	the day your phone heard, drawn as a bar of the day's clock. It is the one
	fixed statement of what the page can know: a page written from eight heard
	hours is a page about eight hours. Computed facts only; an absent one is
	left out, never "no data".
-->
<script lang="ts">
	import type { DayFactsApi } from "$lib/wiki/api";

	interface Props {
		weekday: string;
		facts: DayFactsApi | null;
		/** The zone the day was windowed in, so the bar reads in local hours. */
		timezone: string | null;
		/** Draw the bar: in the Article, where it grows into Data's dayline (the two share a transition name). */
		clock?: boolean;
	}

	let { weekday, facts, timezone, clock = true }: Props = $props();

	const f = (c: number) => Math.round((c * 9) / 5 + 32);

	/** Minutes since local midnight for an instant, in the day's zone. */
	function localMinutes(iso: string): number {
		const parts = new Intl.DateTimeFormat("en-US", {
			hour: "numeric",
			minute: "numeric",
			hourCycle: "h23",
			timeZone: timezone ?? undefined,
		}).formatToParts(new Date(iso));
		const h = Number(parts.find((p) => p.type === "hour")?.value ?? 0);
		const m = Number(parts.find((p) => p.type === "minute")?.value ?? 0);
		return h * 60 + m;
	}

	/** The day's length: 24 hours, or 23 or 25 on a daylight-saving change. */
	const dayMinutes = $derived(facts?.day_minutes ?? 1440);

	const spans = $derived(
		(facts?.coverage ?? []).map(([s, e]) => {
			const a = localMinutes(s);
			let b = localMinutes(e);
			if (b < a) b = 1440; // runs past midnight: the bar ends at the day's end
			return { left: (a / 1440) * 100, width: Math.max(((b - a) / 1440) * 100, 0.4) };
		}),
	);

	const heard = $derived.by(() => {
		const m = facts?.recorded_minutes ?? 0;
		if (m <= 0) return null;
		if (m < 60) return `Heard ${m} ${m === 1 ? "minute" : "minutes"}`;
		return `Heard ${Math.round(m / 60)} of ${Math.round(dayMinutes / 60)} hours`;
	});

	const weather = $derived(
		facts?.temperature_high_c != null && facts?.temperature_low_c != null
			? `${f(facts.temperature_high_c)}° / ${f(facts.temperature_low_c)}°`
			: null,
	);
</script>

<p class="dateline">
	<span>{weekday}</span>
	{#if weather}<span class="dot" aria-hidden="true">·</span><span title="High and low, from a weather model">{weather}</span>{/if}
	{#if heard}
		<span class="dot" aria-hidden="true">·</span>
		<span class="heard">
			{heard}
			{#if clock}
				<span class="bar" aria-hidden="true">
					{#each spans as s, i (i)}
						<span class="span" style:left="{s.left}%" style:width="{s.width}%"></span>
					{/each}
				</span>
			{/if}
		</span>
	{/if}
</p>

<style>
	.dateline {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.5rem;
		margin: 0 0 0.375rem;
		font-family: var(--font-sans);
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
	}

	.heard {
		display: inline-flex;
		align-items: center;
		gap: 0.5rem;
	}

	/* The same element as Data's dayline, for the view transition between them. */
	.bar {
		view-transition-name: day-clock;
		position: relative;
		display: inline-block;
		width: 6rem;
		height: 4px;
		border-radius: 999px;
		background: var(--color-border);
	}

	.span {
		position: absolute;
		top: 0;
		bottom: 0;
		border-radius: 999px;
		background: var(--color-foreground-subtle);
	}

</style>
