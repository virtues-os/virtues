<!--
	DayNumbers.svelte

	Your numbers for the day: a few measures you chose, each with the day's
	value, a bar for each of the 30 days before and this one, and the usual
	said in words, because a mark alone is a guess at what it means. The light
	band behind the bars is the middle half of the days before; the dark bar
	is this day.

	Which numbers: the ones you pinned (up to five, the same on every day).
	One pin, "Most unusual today", stands for whichever number sat furthest
	from its usual, from a kind the other pins don't show; your server picks
	it. Before you pin any, a few common ones show, and only those your record
	actually holds. Nothing here is written by a model; every value is counted
	from the record (`/api/wiki/day/:date/measures`), and one request brings
	every measure, so the picker draws its rows without asking again.

	The apps carry their own copy of this page and can meet a server older than
	the catalog. That server returns only the pins it knows and a bare list of
	measures, so the strip shows those pins without "Most unusual today", and
	the picker lists the measures without their charts.
-->
<script lang="ts">
	import { FloatingContent, useClickOutside, useEscapeKey } from "$lib/floating";
	import { portal } from "$lib/actions/portal";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { getDayMeasures, UNUSUAL_PIN, type CatalogMeasureApi, type DayMeasureApi, type DayMeasuresApi } from "$lib/wiki/api";

	interface Props {
		/** The page's day, `YYYY-MM-DD`. */
		date: string;
		/** Your pins (`lane:id`, or `unusual`): undefined while loading, null
		 *  before you've chosen any (the starters show), empty when you chose none. */
		pins: string[] | null | undefined;
		onchange: (pins: string[]) => void;
		/** Your last change couldn't be saved. */
		saveFailed?: boolean;
	}

	let { date, pins, onchange, saveFailed = false }: Props = $props();

	const MAX = 5;
	/** Shown before you've chosen, when your record holds them. */
	const STARTERS = ["health:steps", "communication:people", "communication:sent", "activity:screen", UNUSUAL_PIN];

	const chosen = $derived(pins ?? STARTERS);
	let data = $state<DayMeasuresApi | null>(null);
	let failed = $state(false);

	$effect(() => {
		if (pins === undefined) return;
		const day = date;
		const keys = chosen;
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

	/** Every measure with its days; null from a server older than the catalog. */
	const catalog = $derived<CatalogMeasureApi[] | null>(data?.catalog ?? null);
	const byKey = $derived(new Map<string, DayMeasureApi>((catalog ?? data?.measures ?? []).map((m) => [m.key, m])));

	/** Days in the 31 with a value. */
	const daysWith = (m: DayMeasureApi) => m.before.filter((v) => v != null).length + (m.value != null ? 1 : 0);
	const held = (m: DayMeasureApi) => daysWith(m) > 0;

	type Cell = { key: string; m: DayMeasureApi | null; unusual: boolean };

	const cells = $derived.by((): Cell[] => {
		if (!data) return [];
		const out: Cell[] = [];
		for (const key of chosen) {
			if (key === UNUSUAL_PIN) {
				// An older server can't pick one: no cell, rather than a wrong one.
				if (data.unusual !== undefined) out.push({ key, m: (data.unusual && byKey.get(data.unusual)) || null, unusual: true });
				continue;
			}
			const m = byKey.get(key);
			// A pin can outlive its measure; a starter shows only if the record has held it lately.
			if (!m || (pins == null && !held(m))) continue;
			out.push({ key, m, unusual: false });
		}
		// Before you choose, "Nothing stood out" alone is not worth a strip.
		if (pins == null && out.every((c) => c.unusual && !c.m)) return [];
		return out;
	});

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

	/** A duration in whole minutes, when the unit is one. */
	function minutes(m: DayMeasureApi, v: number): number | null {
		if (m.unit === "h") return Math.round(v * 60);
		if (m.unit === "min") return Math.round(v);
		return null;
	}

	/** The value as text and unit pieces, so the units can be set smaller. */
	function pieces(m: DayMeasureApi, v: number): [string, string][] {
		const min = minutes(m, v);
		if (min != null) {
			const h = Math.floor(min / 60);
			return h ? [[`${h}`, "h"], [` ${min % 60}`, "m"]] : [[`${min}`, "m"]];
		}
		const n = Math.round(v).toLocaleString();
		if (m.unit === "$") return [[`$${n}`, ""]];
		if (m.unit === "") return [[n, ""]];
		return [[n, ` ${m.unit}`]];
	}

	const fmt = (m: DayMeasureApi, v: number) => pieces(m, v).map(([n, u]) => n + u).join("");

	const label = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

	function tip(m: DayMeasureApi, u: [number, number, number] | null): string {
		if (!u) return "Not enough days before this one to say what's usual.";
		const n = m.before.filter((x) => x != null).length;
		const days = n === m.before.length ? `the ${n} days before` : `the ${n} days with a count in the ${m.before.length} before`;
		return `The middle half of ${days} ran ${fmt(m, u[0])} to ${fmt(m, u[2])}.`;
	}

	/** The 31 bars: the days before, oldest first, then this one. */
	function chart(m: DayMeasureApi, w: number, h: number) {
		const vals = [...m.before, m.value];
		const known = vals.filter((v): v is number => v != null);
		const max = Math.max(...known, 0);
		const min = Math.min(...known, max);
		// A rate (a heart rate, say) floats between its own floor and peak: from
		// zero, 60 bpm spends most of the bar saying "alive".
		const lo = m.kind === "rate" ? Math.max(0, min - (max - min) / 2) : 0;
		const span = max - lo || 1;
		const y = (v: number) => h - 1 - ((v - lo) / span) * (h - 3);
		const step = w / vals.length;
		const u = usual(m);
		return {
			band: u ? { top: y(u[2]), height: Math.max(1, y(u[0]) - y(u[2])) } : null,
			bars: vals.map((v, i) => ({
				x: i * step + step * 0.18,
				width: step * 0.64,
				top: v == null ? h - 1.5 : y(v),
				height: v == null ? 1 : Math.max(1, h - 1 - y(v)),
				none: v == null,
				today: i === vals.length - 1,
			})),
		};
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
	// change writes them out. Before you choose, the starters on the strip are
	// your pins.
	const known = $derived(new Set((data?.available ?? []).map((m) => m.key)));
	const picked = $derived(
		pins != null ? (data ? pins.filter((k) => k === UNUSUAL_PIN || known.has(k)) : pins) : cells.map((c) => c.key),
	);
	const full = $derived(picked.length >= MAX);
	const unusualOn = $derived(picked.includes(UNUSUAL_PIN));

	// The panel is at the end of <body>, so it takes focus to make its first
	// checkbox the next Tab; Escape hands focus back to the button.
	$effect(() => {
		if (open && panel) panel.querySelector<HTMLElement>("input:not(:disabled)")?.focus({ preventScroll: true });
	});

	function toggle(key: string, on: boolean) {
		onchange(on ? [...picked, key].slice(0, MAX) : picked.filter((k) => k !== key));
	}

	/** In your record: what the 31 days hold, the most-held first. */
	const inRecord = $derived(
		(catalog ?? [])
			.map((m, i) => ({ m, i, days: daysWith(m) }))
			.filter((r) => r.days > 0)
			.sort((a, b) => b.days - a.days || a.i - b.i),
	);
	const missing = $derived((catalog ?? []).filter((m) => !held(m)));

	/** Sentence case for a registry display name: "Sleep Sessions" reads "Sleep sessions". */
	const sentence = (s: string) => s.replace(/ ([A-Z][a-z]+)/g, (_, w: string) => ` ${w.toLowerCase()}`);

	/** What gives a measure, by the ontology it reads. */
	const NEEDS: Record<string, string> = {
		health_heart_rate: "Needs Apple Health on your phone",
		health_hrv: "Needs Apple Health on your phone",
		health_sleep: "Needs a sleep source, like Apple Health on your phone",
		health_steps: "Needs Apple Health on your phone",
		health_workout: "Needs Apple Health on your phone or Strava",
		location_visit: "Needs location on your phone",
		communication_message: "Needs Messages on your computer",
		communication_transcription: "Needs the microphone on your phone",
		financial_transaction: "Needs a bank or card connection",
		activity_app_session: "Needs app usage on your computer",
		activity_web_browsing: "Needs browsing history on your computer",
	};
	/** Why a measure reads nothing when its ontology holds rows it can't judge. */
	const UNJUDGED: Record<string, string> = {
		financial_transaction: "Your phone's card transactions don't say which way the money went yet",
	};

	/** The line under a measure the 31 days don't hold, and whether a source would give it. */
	function why(m: CatalogMeasureApi): { line: string; connect: boolean } {
		const rows = (catalog ?? []).some((o) => o.ontology === m.ontology && held(o));
		if (rows && UNJUDGED[m.ontology]) return { line: UNJUDGED[m.ontology], connect: false };
		return { line: NEEDS[m.ontology] ?? `Needs ${sentence(m.source).toLowerCase()} in your record`, connect: true };
	}

	function openSources() {
		open = false;
		windowShellStore.openTabFromRoute("/sources");
	}
</script>

{#snippet bars(m: DayMeasureApi, w: number, h: number, hot: boolean)}
	{@const c = chart(m, w, h)}
	<svg width={w} height={h} viewBox="0 0 {w} {h}" aria-hidden="true">
		{#if c.band}<rect x="0" y={c.band.top} width={w} height={c.band.height} class="band" />{/if}
		{#each c.bars as b, i (i)}
			<rect x={b.x} y={b.top} width={b.width} height={b.height} rx="0.5" class={b.none ? "tick" : b.today ? (hot ? "today hot" : "today") : "day"} />
		{/each}
	</svg>
{/snippet}

{#if data || failed}
	<div class="nums-row">
		{#if cells.length}
			<section class="nums" aria-label="Your numbers for the day">
				{#each cells as c (c.key)}
					{@const m = c.m}
					{#if !m}
						<div class="num">
							<span class="k"><span class="hot">Most unusual today</span><span>Nothing stood out</span></span>
						</div>
					{:else}
						{@const u = usual(m)}
						<div class="num" data-tip={tip(m, u)}>
							<span class="k">
								{#if c.unusual}<span class="hot">Most unusual today</span>{/if}
								<span>{label(m.label)}</span>
							</span>
							{#if m.value == null}
								<span class="v none">Not recorded</span>
							{:else}
								<span class="v">{#each pieces(m, m.value) as [n, unit], i (i)}{n}{#if unit}<small>{unit}</small>{/if}{/each}</span>
							{/if}
							{@render bars(m, 92, 22, c.unusual)}
							<span class="u">{u ? `Usual ${fmt(m, u[1])}` : "Too few days to say"}</span>
							<span class="sr-only">{tip(m, u)}</span>
						</div>
					{/if}
				{/each}
			</section>
		{/if}
		{#if failed}<p class="err">Your server couldn't count your numbers for this day. Reload to try again.</p>{/if}
		{#if saveFailed}<p class="err" role="status">Your server couldn't save your numbers. Try again.</p>{/if}
		{#if data}
			<button
				bind:this={button}
				type="button"
				class="edit"
				class:bare={!cells.length}
				aria-haspopup="dialog"
				aria-expanded={open}
				onclick={() => (open = !open)}>Edit numbers</button
			>
		{/if}
	</div>
{/if}

{#if open && button && data}
	<div use:portal>
		<FloatingContent anchor={button} options={{ placement: "bottom-end", offset: 6, flip: true, shift: true, padding: 12, strategy: "fixed" }}>
			<div class="panel" bind:this={panel} role="dialog" aria-label="Your numbers">
				<div class="head"><span class="title">Your numbers</span><span class="count">{picked.length} of {MAX}</span></div>
				<p class="sub">The same five on every day.</p>
				{#if full}<p class="sub" role="status">Uncheck one to choose another.</p>{/if}

				<label class="pick unusual-pick" class:full={!unusualOn && full}>
					<input type="checkbox" checked={unusualOn} disabled={!unusualOn && full} onchange={(e) => toggle(UNUSUAL_PIN, e.currentTarget.checked)} />
					<span
						>Most unusual today<span class="from"
							>{catalog ? "Each day, the number furthest from your usual, from a kind your other numbers don't show" : "Update your server to see this one"}</span
						></span
					>
				</label>

				{#if !catalog}
					<!-- An older server lists its measures without their days. -->
					<div class="group">Numbers</div>
					{#each data.available as a (a.key)}
						{@const on = picked.includes(a.key)}
						<label class="pick" class:full={!on && full}>
							<input type="checkbox" checked={on} disabled={!on && full} onchange={(e) => toggle(a.key, e.currentTarget.checked)} />
							<span>{label(a.label)}</span>
						</label>
					{/each}
				{/if}

				{#if inRecord.length}
					<div class="group">In your record</div>
					{#each inRecord as { m, days } (m.key)}
						{@const on = picked.includes(m.key)}
						<label class="pick" class:full={!on && full}>
							<input type="checkbox" checked={on} disabled={!on && full} onchange={(e) => toggle(m.key, e.currentTarget.checked)} />
							<span>{label(m.label)}<span class="from">From {sentence(m.source)}{days < 31 ? ` · ${days} of the last 31 days` : ""}</span></span>
							{@render bars(m, 64, 16, false)}
						</label>
					{/each}
				{/if}

				{#if missing.length}
					<div class="group">Not in your record yet</div>
					{#each missing as m (m.key)}
						{@const on = picked.includes(m.key)}
						{@const w = why(m)}
						<div class="pick missing">
							{#if on}
								<input type="checkbox" checked aria-label="Show {label(m.label)}" onchange={(e) => toggle(m.key, e.currentTarget.checked)} />
							{:else}
								<span class="info" aria-hidden="true">ⓘ</span>
							{/if}
							<span>{label(m.label)}<span class="how">{w.line}{#if w.connect} · <button type="button" class="link" onclick={openSources}>Connect it</button>{/if}</span></span>
						</div>
					{/each}
				{/if}
			</div>
		</FloatingContent>
	</div>
{/if}

<style>
	/* The strip sits in the text column; its edit button hangs in the margin
	   beside it (the article's 2.25rem gutter), and under it when there is no
	   margin. */
	.nums-row {
		position: relative;
		max-width: 40rem;
		margin: 1.4rem 0 1.75rem;
		container-type: inline-size;
		font-family: var(--font-sans);
	}

	.nums {
		display: grid;
		grid-template-columns: repeat(5, minmax(0, 1fr));
		gap: 1.25rem;
	}

	@container (max-width: 34rem) {
		.nums {
			grid-template-columns: repeat(3, minmax(0, 1fr));
		}
	}

	@container (max-width: 20rem) {
		.nums {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}
	}

	.num {
		position: relative;
		display: flex;
		flex-direction: column;
		min-width: 0;
	}

	.k {
		display: flex;
		flex-direction: column;
		justify-content: flex-end;
		min-height: 2rem;
		font-size: 0.75rem;
		line-height: 1.3;
		color: var(--color-foreground-subtle);
	}

	.hot {
		font-size: 0.6875rem;
		color: var(--color-secondary);
	}

	.v {
		margin-top: 0.2rem;
		font-family: var(--font-serif);
		font-size: 1.625rem;
		line-height: 1.15;
		font-variant-numeric: lining-nums tabular-nums;
		color: var(--color-foreground);
		white-space: nowrap;
	}

	.v small {
		margin-left: 0.05em;
		font-size: 0.9rem;
		color: var(--color-foreground-muted);
	}

	.v.none {
		font-family: var(--font-sans);
		font-size: 0.8125rem;
		line-height: calc(1.625rem * 1.15);
		color: var(--color-foreground-subtle);
	}

	svg {
		display: block;
		flex: none;
		overflow: visible;
	}

	.num svg {
		margin: 0.35rem 0 0.25rem;
	}

	.band {
		fill: color-mix(in srgb, var(--color-foreground) 7%, transparent);
	}

	.day {
		fill: color-mix(in srgb, var(--color-foreground) 22%, transparent);
	}

	.tick {
		fill: color-mix(in srgb, var(--color-foreground) 14%, transparent);
	}

	.today {
		fill: var(--color-foreground);
	}

	.today.hot {
		fill: var(--color-secondary);
	}

	.u {
		font-size: 0.75rem;
		line-height: 1.3;
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

	.err {
		margin: 0.5rem 0 0;
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
	}

	/* The padding spans the gutter, so the pointer crossing from the strip to
	   the margin never leaves the row and the button stays shown. Unshown, it
	   takes no pointer, so the margin notes under it keep their clicks. */
	.edit {
		position: absolute;
		top: 0;
		left: 100%;
		padding: 0.25rem 0 0.25rem 2.25rem;
		font: inherit;
		font-size: 0.75rem;
		white-space: nowrap;
		color: var(--color-foreground-subtle);
		background: none;
		border: none;
		cursor: pointer;
		opacity: 0;
		pointer-events: none;
		transition: opacity 0.15s;
	}

	.nums-row:hover .edit,
	.edit:focus-visible,
	.edit[aria-expanded="true"] {
		opacity: 1;
		pointer-events: auto;
	}

	.edit:hover,
	.edit[aria-expanded="true"] {
		color: var(--color-foreground);
	}

	@media (hover: none) {
		.edit {
			opacity: 1;
			pointer-events: auto;
		}
	}

	/* No strip to hover: the button is the only way back to your numbers. */
	.edit.bare {
		position: static;
		padding: 0.25rem 0;
		opacity: 1;
		pointer-events: auto;
	}

	@media (max-width: 56rem) {
		.edit {
			position: static;
			display: block;
			margin-top: 0.625rem;
			padding: 0.25rem 0;
			opacity: 1;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.edit {
			transition: none;
		}
	}

	/* ── The picker ── */
	.panel {
		width: min(23rem, calc(100vw - 32px));
		max-height: min(32rem, 70vh);
		overflow-y: auto;
		overscroll-behavior: contain;
		padding: 0.875rem 0.75rem 0.875rem;
		font-family: var(--font-sans);
		font-size: 0.8125rem;
	}

	.head {
		display: flex;
		justify-content: space-between;
		align-items: baseline;
		padding: 0 0.35rem;
	}

	.title {
		font-family: var(--font-serif);
		font-size: 1.125rem;
		color: var(--color-foreground);
	}

	.count,
	.sub {
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
	}

	.sub {
		margin: 0.1rem 0 0;
		padding: 0 0.35rem;
	}

	.unusual-pick {
		margin-top: 0.75rem;
		background: color-mix(in srgb, var(--color-secondary) 6%, transparent);
	}

	.group {
		margin: 0.9rem 0.35rem 0.35rem;
		font-size: 0.6875rem;
		letter-spacing: 0.05em;
		text-transform: uppercase;
		color: var(--color-foreground-subtle);
	}

	.pick {
		display: grid;
		grid-template-columns: 1.1rem minmax(0, 1fr) auto;
		align-items: center;
		column-gap: 0.55rem;
		padding: 0.4rem 0.35rem;
		border-radius: 6px;
		color: var(--color-foreground);
		cursor: pointer;
	}

	.pick:not(.missing):hover {
		background: var(--color-surface-elevated);
	}

	.pick.full {
		cursor: default;
	}

	.pick.full > span {
		opacity: 0.55;
	}

	.pick input {
		margin: 0;
		accent-color: var(--color-primary);
	}

	.from,
	.how {
		display: block;
		font-size: 0.6875rem;
		line-height: 1.4;
		color: var(--color-foreground-subtle);
	}

	.pick.missing {
		grid-template-columns: 1.1rem minmax(0, 1fr);
		color: var(--color-foreground-muted);
		cursor: default;
	}

	.info {
		justify-self: center;
		color: var(--color-foreground-subtle);
	}

	.link {
		font: inherit;
		padding: 0;
		color: var(--color-primary);
		background: none;
		border: none;
		cursor: pointer;
	}

	.link:hover {
		text-decoration: underline;
	}
</style>
