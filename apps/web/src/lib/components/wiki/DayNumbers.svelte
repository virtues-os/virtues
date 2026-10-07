<!--
	DayNumbers.svelte

	Your numbers for the day: a few measures you chose, each with where the
	day sat against your own usual. The tick's band is the middle half of the
	days before, the line through it is the middle day, and the dot is this
	one. The usual is said in words under it, because a mark alone is a guess
	at what it means.

	Which numbers: the ones you pinned (up to five, the same on every day).
	Before you pin any, a few common ones show, and only those your record
	actually holds. Nothing here is written by a model; every value is counted
	from the record (`/api/wiki/day/:date/measures`).
-->
<script lang="ts">
	import { FloatingContent, useClickOutside, useEscapeKey } from "$lib/floating";
	import { portal } from "$lib/actions/portal";
	import { getDayMeasures, type DayMeasureApi, type DayMeasuresApi } from "$lib/wiki/api";

	interface Props {
		/** The page's day, `YYYY-MM-DD`. */
		date: string;
		/** Your pins (`lane:id`): undefined while loading, null before you've
		 *  chosen any (the starters show), empty when you chose none. */
		pins: string[] | null | undefined;
		onchange: (pins: string[]) => void;
		/** Your last change couldn't be saved. */
		saveFailed?: boolean;
	}

	let { date, pins, onchange, saveFailed = false }: Props = $props();

	const MAX = 5;
	/** Shown before you've chosen, when your record holds them. */
	const STARTERS = ["health:steps", "financial:spend", "communication:people", "health:sleep", "activity:screen"];

	const chosen = $derived(pins ?? STARTERS);
	let data = $state<DayMeasuresApi | null>(null);
	let failed = $state(false);

	$effect(() => {
		const keys = chosen;
		const day = date;
		if (pins === undefined) return;
		// A newer pin set or day supersedes this request.
		let live = true;
		failed = false;
		getDayMeasures(day, keys)
			.then((d) => {
				if (live) data = d;
			})
			.catch(() => {
				if (live) failed = true;
			});
		return () => {
			live = false;
		};
	});

	/** Before you choose, a starter shows only if the record has held it lately. */
	const shown = $derived(
		(data?.measures ?? []).filter((m) => pins != null || m.value != null || m.before.some((v) => v != null)),
	);

	function quantile(sorted: number[], p: number): number {
		const i = (sorted.length - 1) * p;
		const lo = Math.floor(i);
		const hi = Math.ceil(i);
		return sorted[lo] + (sorted[hi] - sorted[lo]) * (i - lo);
	}

	/** The usual: 25th, 50th and 75th percentile of the days before, when there are enough of them. */
	function usual(m: DayMeasureApi): [number, number, number] | null {
		const v = m.before.filter((x): x is number => x != null).sort((a, b) => a - b);
		if (v.length < 7) return null;
		return [quantile(v, 0.25), quantile(v, 0.5), quantile(v, 0.75)];
	}

	function fmt(m: DayMeasureApi, v: number): string {
		switch (m.unit) {
			case "$":
				return `$${Math.round(v).toLocaleString()}`;
			case "h":
				return `${v.toFixed(1)} h`;
			case "":
				return Math.round(v).toLocaleString();
			default:
				return `${Math.round(v).toLocaleString()} ${m.unit}`;
		}
	}

	const label = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

	// One geometry for every cell: the middle half of the usual days fills the
	// middle third, the middle day sits at the centre, the day is placed
	// piecewise between them and clamped to the ends.
	const W = 72;
	function dotX(v: number, [q1, q2, q3]: [number, number, number]): number {
		const x = v <= q2 ? 36 - (12 * (q2 - v)) / Math.max(q2 - q1, 1e-9) : 36 + (12 * (v - q2)) / Math.max(q3 - q2, 1e-9);
		return Math.max(3, Math.min(W - 3, x));
	}

	function tip(m: DayMeasureApi, u: [number, number, number] | null): string {
		if (!u) return "Not enough days before this one to say what's usual.";
		const n = m.before.filter((x) => x != null).length;
		const days = n === m.before.length ? `the ${n} days before` : `the ${n} days with a count in the ${m.before.length} before`;
		return `The middle half of ${days} ran ${fmt(m, u[0])} to ${fmt(m, u[2])}.`;
	}

	// ── Choosing ────────────────────────────────────────────────────────────
	let button: HTMLElement | null = $state(null);
	let panel: HTMLElement | null = $state(null);
	let open = $state(false);
	useClickOutside(() => [panel, button], () => (open = false), () => open);
	useEscapeKey(() => {
		open = false;
		button?.focus();
	}, () => open);

	// Pins the registry no longer lists don't count toward the limit; the next
	// change writes them out.
	const known = $derived(new Set((data?.available ?? []).map((m) => m.key)));
	const picked = $derived(pins != null ? (data ? pins.filter((k) => known.has(k)) : pins) : shown.map((m) => m.key));

	// The panel is at the end of <body>, so it takes focus to make its first
	// checkbox the next Tab; Escape hands focus back to the button.
	$effect(() => {
		if (open && panel) panel.querySelector<HTMLElement>("input:not(:disabled)")?.focus({ preventScroll: true });
	});

	function toggle(key: string, on: boolean) {
		const next = on ? [...picked, key].slice(0, MAX) : picked.filter((k) => k !== key);
		onchange(next);
	}

	const lanes = $derived.by(() => {
		const by = new Map<string, { key: string; label: string }[]>();
		for (const m of data?.available ?? []) {
			if (!by.has(m.lane)) by.set(m.lane, []);
			by.get(m.lane)!.push({ key: m.key, label: label(m.label) });
		}
		return [...by.entries()];
	});
</script>

{#if shown.length}
	<section class="numbers" aria-label="Your numbers for the day">
		{#each shown as m (m.key)}
			{@const u = usual(m)}
			<div class="num" data-tip={m.value != null ? tip(m, u) : null}>
				<span class="k">{label(m.label)}</span>
				{#if m.value == null}
					<span class="none">Not recorded</span>
				{:else}
					<span class="v">{fmt(m, m.value)}</span>
					{#if u}
						<svg width={W} height="9" viewBox="0 0 {W} 9" aria-hidden="true">
							<line x1="24" x2="48" y1="4.5" y2="4.5" class="band" stroke-width="3" stroke-linecap="round" />
							<circle cx={dotX(m.value, u)} cy="4.5" r="2.5" class="dot" />
							<line x1="36" x2="36" y1="0" y2="9" class="mid" stroke-width="1" />
						</svg>
						<span class="u">Usual {fmt(m, u[1])}</span>
						<span class="sr-only">{tip(m, u)}</span>
					{/if}
				{/if}
			</div>
		{/each}
	</section>
{/if}

{#if data || failed}
	<div class="foot">
		{#if failed}<span class="err">Your server couldn't count your numbers for this day. Reload to try again.</span>{/if}
		{#if saveFailed}<span class="err" role="status">Your server couldn't save your numbers. Try again.</span>{/if}
		<button
			bind:this={button}
			type="button"
			class="choose"
			aria-haspopup="dialog"
			aria-expanded={open}
			onclick={() => (open = !open)}>Choose numbers</button
		>
	</div>
{/if}

{#if open && button}
	<div use:portal>
		<FloatingContent anchor={button} options={{ placement: "bottom-end", offset: 6, flip: true, shift: true, padding: 12, strategy: "fixed" }}>
			<div class="panel" bind:this={panel} role="dialog" aria-label="Choose your numbers">
				<div class="head"><span class="title">Your numbers</span><span class="sub">Up to {MAX}, the same on every day</span></div>
				{#each lanes as [lane, items] (lane)}
					<div class="lane">
						<div class="lane-name">{label(lane)}</div>
						{#each items as it (it.key)}
							{@const on = picked.includes(it.key)}
							<label class="row" class:full={!on && picked.length >= MAX}>
								<input type="checkbox" checked={on} disabled={!on && picked.length >= MAX} onchange={(e) => toggle(it.key, e.currentTarget.checked)} />
								<span>{it.label}</span>
							</label>
						{/each}
					</div>
				{/each}
			</div>
		</FloatingContent>
	</div>
{/if}

<style>
	.numbers {
		display: grid;
		grid-auto-flow: column;
		grid-auto-columns: minmax(0, 1fr);
		column-gap: 1.5rem;
		max-width: 40rem;
		padding: 0.75rem 0;
		border-top: 1px solid var(--color-border);
		border-bottom: 1px solid var(--color-border);
		font-family: var(--font-sans);
	}

	.num {
		position: relative;
		display: grid;
		grid-template-rows: 1fr auto 12px auto;
		align-items: end;
		row-gap: 3px;
		min-width: 0;
	}

	.k {
		font-size: 0.75rem;
		line-height: 1.3;
		color: var(--color-foreground-subtle);
	}

	.v {
		font-family: var(--font-serif);
		font-size: 1.125rem;
		line-height: 1.25;
		font-variant-numeric: lining-nums tabular-nums;
		color: var(--color-foreground);
		white-space: nowrap;
	}

	.none {
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
	}

	svg {
		display: block;
		align-self: center;
		overflow: visible;
	}

	.band {
		stroke: color-mix(in srgb, var(--color-foreground) 18%, transparent);
	}

	.dot {
		fill: var(--color-foreground);
	}

	.mid {
		stroke: var(--color-foreground-subtle);
	}

	.u {
		font-size: 0.75rem;
		line-height: 14px;
		color: var(--color-foreground-subtle);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.num[data-tip]:hover::after {
		content: attr(data-tip);
		position: absolute;
		left: 0;
		top: calc(100% + 0.5rem);
		z-index: 20;
		width: max-content;
		max-width: 16rem;
		padding: 0.5rem 0.625rem;
		border-radius: 6px;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.12);
		font-size: 0.75rem;
		line-height: 1.45;
		color: var(--color-foreground-muted);
		white-space: normal;
	}

	.foot {
		display: flex;
		justify-content: flex-end;
		align-items: baseline;
		gap: 1rem;
		max-width: 40rem;
		margin: 0.375rem 0 1.75rem;
		font-family: var(--font-sans);
		font-size: 0.8125rem;
	}

	.err {
		color: var(--color-foreground-subtle);
	}

	.choose {
		font: inherit;
		color: var(--color-foreground-subtle);
		background: none;
		border: none;
		padding: 0.25rem 0;
		cursor: pointer;
	}

	.choose:hover,
	.choose[aria-expanded="true"] {
		color: var(--color-foreground);
	}

	.panel {
		width: min(20rem, calc(100vw - 32px));
		max-height: min(30rem, 70vh);
		overflow-y: auto;
		overscroll-behavior: contain;
		padding: 0.5rem 0.375rem 0.625rem;
		font-family: var(--font-sans);
		font-size: 0.8125rem;
	}

	.head {
		display: flex;
		justify-content: space-between;
		align-items: baseline;
		padding: 0.375rem 0.5rem 0.5rem;
	}

	.title {
		font-family: var(--font-serif);
		font-size: 1.125rem;
		color: var(--color-foreground);
	}

	.sub,
	.lane-name {
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
	}

	.lane {
		border-top: 1px solid var(--color-border-subtle);
		padding: 0.375rem 0 0.25rem;
	}

	.lane-name {
		padding: 0.25rem 0.5rem;
	}

	.row {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.3rem 0.5rem;
		border-radius: 6px;
		color: var(--color-foreground);
		cursor: pointer;
	}

	.row:hover {
		background: var(--color-surface-elevated);
	}

	.row.full {
		opacity: 0.55;
		cursor: default;
	}

	.row input {
		margin: 0;
		accent-color: var(--color-foreground);
	}

	@media (max-width: 56rem) {
		.numbers {
			grid-auto-flow: row;
			grid-template-columns: 1fr;
			padding: 0;
		}

		.num {
			grid-template-rows: none;
			grid-template-columns: 1fr auto 72px;
			column-gap: 0.75rem;
			align-items: center;
			padding: 0.5rem 0;
		}

		.num + .num {
			border-top: 1px solid var(--color-border-subtle);
		}

		.u {
			grid-column: 1 / -1;
			justify-self: end;
		}
	}
</style>
