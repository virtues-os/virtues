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

	interface Person {
		name: string;
		href: string;
	}

	interface Props {
		facts: DayFactsApi | null;
		people: Person[];
		/** The zone the day was windowed in, so the coverage bar reads in local hours. */
		timezone: string | null;
	}

	let { facts, people, timezone }: Props = $props();

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

	const hours = $derived(facts ? Math.round(facts.recorded_minutes / 60) : 0);
	const hasWeather = $derived(facts?.temperature_high_c != null && facts?.temperature_low_c != null);
</script>

{#if people.length || hasWeather || (facts && facts.recorded_minutes > 0) || (facts && facts.chats > 0)}
	<div class="strip" role="group" aria-label="The day at a glance">
		{#if people.length}
			<span class="fact">
				<span class="key">With</span>
				<span>
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

	.bar {
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
