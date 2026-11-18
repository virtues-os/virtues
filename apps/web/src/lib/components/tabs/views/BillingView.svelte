<script lang="ts">
	/**
	 * Settings → Billing → What AI has cost: this month, the last two weeks
	 * by day, the month by kind of work, the wallet's money in and out, and
	 * every call (UsageView). Where your subscription stands, the balance,
	 * auto top-up and the spending limits are rows on the Settings page
	 * (settings/BillingSettings.svelte); bringing your own AI is its own page
	 * (settings/ByoPage.svelte).
	 */
	import { TextAction } from '$lib';
	import SettingsPage from '$lib/components/settings/SettingsPage.svelte';
	import UsageView from '$lib/components/tabs/views/UsageView.svelte';
	import { formatMicrosPrecise } from '$lib/utils/currency';
	import { formatDate } from '$lib/utils/dateUtils';
	import { getBillingUsage, getUsageSummary, ApiError } from '$lib/api/client';

	// ─── Wallet balance + recent ledger (proxied from virtues-api) ─────────
	type LedgerEntry = { ts: string; micros: number; kind: string; real_micros: number | null };
	type Usage = {
		balance_micros: number;
		month_to_date_micros: number;
		expires_at: string | null;
		entries: LedgerEntry[];
		error?: string;
		unlinked?: boolean;
	};
	let usage = $state<Usage | null>(null);
	let usageLoading = $state(true);
	/**
	 * Why the balance is missing, in the words the server used.
	 *
	 * This panel used to hide itself on any failure — which removed the one
	 * number the page exists to show and said nothing about why. A box whose
	 * wallet call is failing then looks identical to a box with no wallet, and
	 * the reader's only clue is an absence they have to notice.
	 */
	let usageError = $state<string | null>(null);
	/** No Virtues account is connected, so there is no wallet to read. */
	let unlinked = $state(false);

	async function loadUsage() {
		usageLoading = true;
		usageError = null;
		unlinked = false;
		try {
			const data = await getBillingUsage<Usage>();
			// The server answers 200 with an `error` body when it cannot reach
			// the wallet, so a thrown error is not the only failure shape.
			if (data.unlinked) {
				unlinked = true;
				usage = null;
			} else if (data.error) {
				usageError = data.error;
				usage = null;
			} else {
				// Never trust `entries` to be present.
				usage = { ...data, entries: data.entries ?? [] };
			}
		} catch (e) {
			usageError = e instanceof ApiError ? e.message : 'Could not reach the wallet.';
			usage = null;
		}
		usageLoading = false;
	}

	$effect(() => { void loadUsage(); });

	// ─── Local spend summary: the chart and the by-kind tracks ─────────────
	// `app_ai_calls`, box-local, no egress. The wallet above is what the
	// gateway says the money is; this is where it went, by day and by kind.
	type SpendBucket = { label: string; calls: number; cost_micros: number; byo_calls: number };
	type DaySpend = { day: string; calls: number; cost_micros: number };
	type Summary = {
		by_feature: SpendBucket[];
		by_day: DaySpend[];
		last_month_to_date_micros: number | null;
	};
	let summary = $state<Summary | null>(null);
	let summaryError = $state<string | null>(null);
	async function loadSummary() {
		summaryError = null;
		try {
			summary = await getUsageSummary<Summary>();
		} catch (e) {
			summaryError = e instanceof ApiError ? e.message : 'Could not read your server’s call log.';
		}
	}
	$effect(() => { void loadSummary(); });

	/** The last 14 UTC days as bar slots, absent days filled with zero. */
	const days = $derived.by(() => {
		const byDay = new Map((summary?.by_day ?? []).map((d) => [d.day, d]));
		const out: { day: string; cost: number; calls: number; label: string }[] = [];
		const today = new Date();
		for (let i = 13; i >= 0; i--) {
			const d = new Date(Date.UTC(today.getUTCFullYear(), today.getUTCMonth(), today.getUTCDate() - i));
			const key = d.toISOString().slice(0, 10);
			const row = byDay.get(key);
			out.push({
				day: key,
				cost: row?.cost_micros ?? 0,
				calls: row?.calls ?? 0,
				label: d.toLocaleDateString(undefined, { month: 'short', day: 'numeric', timeZone: 'UTC' }),
			});
		}
		return out;
	});
	const dayMax = $derived(Math.max(1, ...days.map((d) => d.cost)));
	// Chart geometry: 14 slots of 45 in a 640 box, bars 34 wide; plot 106 tall
	// from y=12 to the baseline at y=118. One scale, from the tallest day.
	const barX = (i: number) => 6 + i * 45;
	const barH = (cost: number) => Math.max(cost > 0 ? 2 : 0, (cost / dayMax) * 106);

	const monthLocalMicros = $derived((summary?.by_feature ?? []).reduce((a, b) => a + b.cost_micros, 0));
	const totalCalls = $derived((summary?.by_feature ?? []).reduce((a, b) => a + b.calls, 0));
	/** Percent change against last month to the same date; null when there is nothing to compare to. */
	const monthDelta = $derived.by(() => {
		const prev = summary?.last_month_to_date_micros;
		if (prev == null || prev <= 0) return null;
		return Math.round(((monthLocalMicros - prev) / prev) * 100);
	});
	const kindMax = $derived(Math.max(1, ...(summary?.by_feature ?? []).map((b) => b.cost_micros)));
	const kindLabelOf = (label: string) =>
		label === 'other' ? 'Other' : label.replace(/_/g, ' ').replace(/^\w/, (c) => c.toUpperCase());

	/** `$12<span class="c">.40</span>` — the cents recede. Escaped: only digits reach the markup. */
	function moneyFigure(micros: number): string {
		const neg = micros < 0;
		const cents = Math.round(Math.abs(micros) / 10_000);
		const dollars = Math.floor(cents / 100);
		const rest = String(cents % 100).padStart(2, '0');
		return `${neg ? '−' : ''}$${dollars.toLocaleString()}<span class="c">.${rest}</span>`;
	}

	const kindLabel: Record<string, string> = {
		grant: 'Monthly credit',
		topup: 'Top-up',
		charge: 'Usage',
		refund: 'Refund',
		adjust: 'Adjustment',
	};

	function entryDate(ts: string): string {
		const d = new Date(ts);
		const today = new Date();
		const sameDay = d.toDateString() === today.toDateString();
		return sameDay
			? d.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' })
			: formatDate(ts, { month: 'short', day: 'numeric' });
	}


</script>

<SettingsPage
	title="What AI has cost"
	lede="The last two weeks day by day, each kind of work, and every call."
	back={{ route: '/virtues/billing', label: 'Billing' }}
	width="full"
>
<div class="plan-sections">
	<!-- ─── THE MONTH ─────────────────────────────────────────────────────
	     The two figures and the wallet's money in and out. Auto top-up, the
	     rule that refills the balance, is a row on the Settings page. -->
	<section class="chapter chapter-figures">
		{#if usageLoading && !usage && !usageError}
			<p class="chapter-lede">Reading your balance…</p>
		{:else if unlinked}
			<p class="chapter-lede">Connect your Virtues account in Billing to see your balance and what you've paid.</p>
		{:else if usageError}
			<!-- The report stands where the numbers would be: one plain line,
			     then the server's words small. -->
			<p class="note note-error note-figure">
				Your server couldn't read your balance.
				<span class="error-code">{usageError}</span>
				<TextAction inline onclick={() => void loadUsage()}>Check again</TextAction>
			</p>
		{:else if usage}
			<!-- Two figures, Apple-large, the cents receding. The delta is
			     against last month to the same date — the only fair comparison
			     on the 4th — and is absent rather than wrong when there is no
			     last month to compare to. -->
			<div class="figures">
				<div>
					<div class="figure-label">Balance</div>
					<div class="figure mono">{@html moneyFigure(usage.balance_micros)}</div>
				</div>
				<div>
					<div class="figure-label">This month</div>
					<div class="figure mono">{@html moneyFigure(usage.month_to_date_micros)}</div>
					{#if monthDelta !== null}
						<div class="delta" class:down={monthDelta < 0}>
							{monthDelta < 0 ? '↓' : '↑'} {Math.abs(monthDelta)}% on last month
						</div>
					{/if}
				</div>
			</div>

			<!-- Money in and out. The itemized AI calls that make up the
			     `Usage` lines are the last chapter of the page; this is the
			     account statement, not the receipt. -->
			<h3 class="subhead">Money in and out</h3>
			{#if usage.entries.length > 0}
				<div class="entries">
					{#each usage.entries.slice(0, 15) as e (e.ts + e.kind + e.micros)}
						<div class="entry">
							<span class="entry-kind">{kindLabel[e.kind] ?? e.kind}</span>
							<span class="leader"></span>
							<span class="entry-when mono">{entryDate(e.ts)}</span>
							<span class="entry-amount mono" class:credit={e.micros >= 0}>
								{e.micros < 0 ? '−' : '+'}{formatMicrosPrecise(Math.abs(e.micros))}
							</span>
						</div>
					{/each}
				</div>
			{:else}
				<p class="chapter-lede">Nothing has moved yet. Top-ups and charges will appear here.</p>
			{/if}
		{/if}
	</section>

	<!-- ─── USAGE ─────────────────────────────────────────────────────────
	     Where the month went: fourteen days as bars, then the same money by
	     kind. Box-local (`app_ai_calls`); the wallet figures above are the
	     gateway's. Own-key calls are counted and never priced. -->
	<section class="chapter">
		<div class="chapter-head">
			<div>
				<h2 class="settings-label">Usage</h2>
				<p class="chapter-lede">
					Each day of the last two weeks, and what the money went to. Calls on your own key
					are counted but not priced, since only your provider knows the price.
				</p>
			</div>
			{#if totalCalls > 0}
				<a class="see-all" href="#calls">Every call</a>
			{/if}
		</div>
		{#if summaryError}
			<p class="note note-error">
				Your server couldn't read the call log.
				<span class="error-code">{summaryError}</span>
			</p>
		{:else if summary}
			<svg class="chart" viewBox="0 0 640 146" role="img" aria-label="Spending per day for the last fourteen days">
				<g>
					{#each days as d, i (d.day)}
						<rect
							class="bar"
							class:today={i === 13}
							x={barX(i)}
							y={118 - barH(d.cost)}
							width="34"
							height={barH(d.cost)}
							rx="3"
						><title>{i === 13 ? 'Today' : d.label} · {formatMicrosPrecise(d.cost)}{d.calls ? ` · ${d.calls} call${d.calls === 1 ? '' : 's'}` : ''}</title></rect>
					{/each}
				</g>
				<line class="base" x1="6" x2="625" y1="118.5" y2="118.5" />
				{#if days[13]?.cost > 0}
					<text class="v" x={barX(13) + 17} y={118 - barH(days[13].cost) - 8} text-anchor="middle">{formatMicrosPrecise(days[13].cost)}</text>
				{/if}
				{#if monthLocalMicros === 0 && days.every((d) => d.cost === 0)}
					<text class="empty" x="316" y="72" text-anchor="middle">Nothing charged in the last two weeks</text>
				{/if}
				<text x="6" y="138">{days[0]?.label}</text>
				<text x="625" y="138" text-anchor="end">Today</text>
			</svg>

			{#if summary.by_feature.length > 0}
				<div class="kinds">
					{#each summary.by_feature as b (b.label)}
						<div class="kind">
							<span class="kind-n">{kindLabelOf(b.label)}</span>
							<div class="track"><div class="fill" style="width:{(b.cost_micros / kindMax) * 100}%"></div></div>
							<!-- A bucket that is all own-key has a count and no price;
							     "$0.00" there would be a number we invented. A mixed
							     bucket shows the wallet price and says how many calls
							     it leaves out, so the price is not read as the whole. -->
							{#if b.byo_calls === b.calls}
								<span class="kind-amt dim">on your key</span>
							{:else}
								<span class="kind-amt mono">
									{formatMicrosPrecise(b.cost_micros)}
									{#if b.byo_calls > 0}
										<span class="block font-sans text-[11px] leading-tight text-foreground-subtle">{b.byo_calls} on your key</span>
									{/if}
								</span>
							{/if}
						</div>
					{/each}
				</div>
			{/if}
		{/if}
	</section>

	<!-- The itemization, last. -->
	<div id="calls"><UsageView /></div>
</div>
</SettingsPage>

<style>
	/* ─── Chapters ──────────────────────────────────────────────────────────
	   The same chapter rule as System: a hairline under a heading separates
	   subjects. */
	.plan-sections {
		display: flex;
		flex-direction: column;
	}
	.chapter {
		padding-top: 28px;
		margin-top: 28px;
		border-top: 1px solid var(--color-border-subtle);
	}
	.chapter:first-child {
		border-top: none;
		margin-top: 8px;
		padding-top: 0;
	}
	/* A heading, then a link on the same line (Usage, "See all calls"). */
	.chapter-head {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 16px;
	}
	.chapter-lede {
		font-size: 13px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
		margin: 0;
		max-width: 60ch;
	}
	/* A step below `.settings-label`: still a heading, still roman serif, just
	   smaller — this names a part of Balance, not a chapter of the page. */
	.subhead {
		font-size: 0.875rem;
		font-weight: 400;
		color: var(--color-foreground);
		margin: 22px 0 4px;
	}

	.mono {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-variant-numeric: tabular-nums;
	}

	.figure {
		font-size: 40px;
		line-height: 1;
		font-weight: 500;
		color: var(--color-foreground);
	}
	.leader {
		flex: 1;
		border-bottom: 1px dotted var(--color-border);
		transform: translateY(-3px);
		min-width: 16px;
	}

	/* ─── Wallet activity ───────────────────────────────────────────────────
	   Was an icon + label + date + amount row. The kind icon is gone: the sign
	   and the color already say which direction the money went, and a column
	   of five different glyphs down the left edge was the loudest thing in a
	   chapter about small numbers. */
	.entries {
		margin-top: 4px;
	}
	.entry {
		display: flex;
		align-items: baseline;
		gap: 10px;
		padding: 5px 0;
		font-size: 13px;
		border-bottom: 1px solid var(--color-border-subtle);
	}
	.entry:last-child {
		border-bottom: none;
	}
	.entry-kind {
		color: var(--color-foreground);
		white-space: nowrap;
	}
	.entry-when {
		font-size: 11px;
		color: var(--color-foreground-subtle);
		white-space: nowrap;
	}
	.entry-amount {
		color: var(--color-foreground);
		white-space: nowrap;
		min-width: 5.5rem;
		text-align: right;
	}
	.entry-amount.credit {
		color: var(--color-success);
	}

	/* ─── Notes ─────────────────────────────────────────────────────────────
	   One shape for every thing the page has to tell you, tinted by weight.
	   The error variant stands where the figures would be when the wallet
	   cannot be read. */
	.note {
		font-size: 12px;
		line-height: 1.5;
		margin: 10px 0 0;
		padding: 9px 11px;
		border-radius: var(--radius-md);
		border: 1px solid transparent;
		max-width: 60ch;
	}
	.note-error {
		color: var(--color-error);
		border-color: color-mix(in srgb, var(--color-error) 22%, transparent);
		background: var(--color-error-subtle);
	}
	/* The branch, set quietly beside the sentence: an owner reads past it, and
	   reads it out loud when we ask what the screen said. */
	.error-code {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 0.85em;
		opacity: 0.72;
	}
	/* Holds the balance figure's slot when there is no balance to show. */
	.note-figure {
		margin-top: 2px;
	}

	@media (max-width: 640px) {
		.chapter-head { flex-direction: column; gap: 10px; }
		.figure { font-size: 34px; }
	}

	/* ─── The figures: Apple-large, mono, the cents recede ─────────────── */
	.chapter-figures { padding-top: 22px; margin-top: 22px; }
	.figures { display: grid; grid-template-columns: 1fr 1fr; gap: 24px; }
	.figure-label { font-size: 11.5px; letter-spacing: 0.06em; text-transform: uppercase; color: var(--color-foreground-subtle); }
	.figures .figure { font-size: 44px; letter-spacing: -0.02em; margin-top: 8px; }
	.figures .figure :global(.c) { color: var(--color-foreground-subtle); font-weight: 400; }
	.delta { font-size: 13px; color: var(--color-foreground-subtle); margin-top: 8px; }
	.delta.down { color: var(--color-success); }
	@media (max-width: 480px) { .figures { grid-template-columns: 1fr; } }

	/* ─── The chart: soft bars, today in the accent, one baseline ─────── */
	.see-all { font-size: 13px; color: var(--color-foreground-subtle); text-decoration: none; }
	.see-all:hover { color: var(--color-foreground); }
	.chart { width: 100%; height: auto; display: block; overflow: visible; margin-top: 6px; }
	.chart .bar { fill: var(--color-border); transition: fill 120ms ease; }
	.chart .bar.today { fill: var(--color-secondary); }
	.chart .bar:hover { fill: var(--color-foreground-subtle); }
	.chart .bar.today:hover { fill: var(--color-secondary); }
	.chart .base { stroke: var(--color-border); stroke-width: 1; }
	.chart text { font-family: var(--font-sans); font-size: 11px; fill: var(--color-foreground-subtle); }
	.chart .v { font-family: var(--font-mono); font-variant-numeric: tabular-nums; fill: var(--color-secondary); font-weight: 500; }
	.chart .empty { font-family: var(--font-sans); font-size: 12px; fill: var(--color-foreground-subtle); }

	/* By kind: a name, a track, a figure. */
	.kinds { margin-top: 22px; display: flex; flex-direction: column; gap: 12px; }
	.kind { display: grid; grid-template-columns: 120px 1fr 72px; align-items: center; gap: 16px; font-size: 14px; }
	.kind-n { color: var(--color-foreground); }
	.track { height: 6px; border-radius: var(--radius-sm); background: var(--color-border-subtle); overflow: hidden; }
	.fill { height: 100%; border-radius: var(--radius-sm); background: var(--color-border); }
	.kind:first-child .fill { background: var(--color-secondary); }
	.kind-amt { text-align: right; color: var(--color-foreground); }
	.kind-amt.dim { color: var(--color-foreground-subtle); font-size: 13px; }
	@media (max-width: 480px) { .kind { grid-template-columns: 96px 1fr 64px; } }

</style>
