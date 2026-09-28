<!--
	DayFactStrip.svelte

	The thin row between a day's Abstract and its body: who it was with, the
	weather, how much of it was recorded, and what you wrote. Facts only, from
	the record; an absent fact is left out rather than shown as "no data".

	"With" is the people the Abstract links — the article's own claim about
	who the day was with, already checked against the record when it was written.
-->
<script lang="ts">
	import type { DayFactsApi } from "$lib/wiki/api";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { veiled } from "$lib/actions/veil";
	import { veil } from "$lib/stores/veil.svelte";

	interface Person {
		name: string;
		href: string;
	}

	interface Props {
		facts: DayFactsApi | null;
		people: Person[];
		/** The zone the day was windowed in, so the coverage bar reads in local hours. */
		timezone: string | null;
		/** The day's parts, each at its midpoint, scored against your usual:
		 *  below zero is routine, above is unlike your usual. */
		novelty?: { at: string; z: number }[];
	}

	let { facts, people, timezone, novelty = [] }: Props = $props();

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

	const spans = $derived(
		(facts?.coverage ?? []).map(([s, e]) => {
			const a = localMinutes(s);
			let b = localMinutes(e);
			if (b < a) b = 1440; // runs past midnight: the bar ends at the day's end
			return { left: (a / 1440) * 100, width: Math.max(((b - a) / 1440) * 100, 0.4) };
		}),
	);

	// The mark is 84 × 18: time across, novelty up, the midline is your usual.
	const MARK_W = 84;
	const MARK_H = 18;
	const markPoints = $derived(
		novelty.map((n) => ({
			x: Math.round((localMinutes(n.at) / 1440) * MARK_W * 10) / 10,
			y: Math.round((MARK_H / 2 - (Math.max(-3, Math.min(3, n.z)) / 3) * (MARK_H / 2 - 2)) * 10) / 10,
			above: n.z > 0,
		})),
	);
	const markPath = $derived(
		[...markPoints].sort((a, b) => a.x - b.x).map((p, i) => `${i ? "L" : "M"}${p.x},${p.y}`).join(" "),
	);

	const hours = $derived(facts ? Math.round(facts.recorded_minutes / 60) : 0);
	const hasWeather = $derived(facts?.temperature_high_c != null && facts?.temperature_low_c != null);
</script>

{#if people.length || hasWeather || (facts && facts.recorded_minutes > 0) || (facts && facts.chats > 0) || novelty.length > 1}
	<div class="strip" role="group" aria-label="The day at a glance">
		{#if people.length}
			<span class="fact">
				<span class="key">With</span>
				<span use:veiled={{ hiding: veil.hiding, whole: true }}>
					{#each people as p, i (p.href)}<a
						href={p.href}
						onclick={(e) => {
							e.preventDefault();
							windowShellStore.openTabFromRoute(p.href);
						}}>{p.name}</a
					>{#if i < people.length - 1},&nbsp;{/if}{/each}
				</span>
			</span>
		{/if}
		{#if hasWeather}
			<span class="fact">
				<span class="key">Weather</span>
				<span>{f(facts?.temperature_high_c as number)}° / {f(facts?.temperature_low_c as number)}°</span>
			</span>
		{/if}
		{#if facts && facts.recorded_minutes > 0}
			<span class="fact">
				<span class="key">Recorded</span>
				<span>{hours} of 24 hours</span>
				<span class="bar" aria-hidden="true">
					{#each spans as s, i (i)}
						<span class="span" style:left="{s.left}%" style:width="{s.width}%"></span>
					{/each}
				</span>
			</span>
		{/if}
		{#if novelty.length > 1}
			<span class="fact" title="Each dot is a part of the day. Above the line is unlike your usual; below is routine.">
				<span class="key">Novelty</span>
				<svg class="mark" width={MARK_W} height={MARK_H} viewBox="0 0 {MARK_W} {MARK_H}" role="img" aria-label="How unlike your usual each part of the day was">
					<line x1="0" x2={MARK_W} y1={MARK_H / 2} y2={MARK_H / 2} class="mark-mid" />
					<path d={markPath} class="mark-line" />
					{#each markPoints as p, i (i)}
						<circle cx={p.x} cy={p.y} r="1.75" class:above={p.above} class="mark-dot" />
					{/each}
				</svg>
			</span>
		{/if}
		{#if facts && facts.chats > 0}
			<span class="fact">
				<span class="key">Wrote</span>
				<span>{facts.chats} {facts.chats === 1 ? "chat" : "chats"}</span>
			</span>
		{/if}
	</div>
{/if}

<style>
	.strip {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.5rem 1.25rem;
		background: var(--color-surface-elevated);
		border-radius: 12px;
		padding: 0.6875rem 1.125rem;
		margin: 0 0 2.25rem;
		font-family: var(--font-sans);
		font-size: 0.8125rem;
		color: var(--color-foreground);
	}

	.fact {
		display: inline-flex;
		align-items: baseline;
		gap: 0.375rem;
	}

	.fact + .fact {
		padding-left: 1.25rem;
		border-left: 1px solid var(--color-border);
	}

	.key {
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
	}

	/* The same element as Record's dayline, for the view transition between them. */
	.bar {
		view-transition-name: day-clock;
		position: relative;
		display: inline-block;
		align-self: center;
		width: 5.25rem;
		height: 4px;
		border-radius: 999px;
		background: var(--color-border);
		margin-left: 0.25rem;
	}

	.span {
		position: absolute;
		top: 0;
		height: 4px;
		border-radius: 999px;
		background: var(--color-primary);
	}

	.mark {
		align-self: center;
		overflow: visible;
	}

	.mark-mid {
		stroke: var(--color-border);
		stroke-width: 1;
	}

	.mark-line {
		fill: none;
		stroke: var(--color-foreground-subtle);
		stroke-width: 0.75;
	}

	.mark-dot {
		fill: var(--color-foreground-subtle);
	}

	.mark-dot.above {
		fill: var(--color-primary);
	}

	a {
		color: var(--color-primary);
		text-decoration: none;
	}

	@media (max-width: 40rem) {
		.fact + .fact {
			padding-left: 0;
			border-left: none;
		}
	}
</style>
