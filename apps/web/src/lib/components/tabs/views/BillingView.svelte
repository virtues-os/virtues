<script lang="ts">
	/**
	 * Settings → What AI has cost: this month's total, the month day by day,
	 * what it went to, and every call (UsageView).
	 *
	 * All four come from your server's own call log (`app_ai_calls`, each
	 * call priced by the gateway's `usage.cost`), so they add up to each
	 * other. The balance is the wallet's and lives on Billing; this page names
	 * it in one line with a link there. Calls on your own AI key are counted
	 * and never priced, since only your provider knows the price.
	 */
	import SettingsPage from '$lib/components/settings/SettingsPage.svelte';
	import Button from '$lib/components/Button.svelte';
	import UsageView from '$lib/components/tabs/views/UsageView.svelte';
	import { formatMicrosPrecise, formatMicrosUSD } from '$lib/utils/currency';
	import { featureLabel } from '$lib/utils/aiFeatures';
	import { openSettingsPage } from '$lib/settings/focus';
	import { DEFAULT_ASSISTANT_NAME } from '$lib/settings';
	import { getBillingUsage, getUsageSummary, ApiError } from '$lib/api/client';

	// ─── The month, from the call log ──────────────────────────────────────
	type SpendBucket = { label: string; calls: number; cost_micros: number; byo_calls: number };
	type DaySpend = { day: string; calls: number; cost_micros: number };
	type Summary = {
		month_start: string;
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
			summaryError = e instanceof ApiError ? e.message : "Your server couldn't read its call log.";
		}
	}
	$effect(() => {
		void loadSummary();
	});

	// ─── The balance, from the wallet, for one line ─────────────────────────
	// Only a balance that loaded is named; Billing says why one is missing.
	let balance = $state<number | null>(null);
	$effect(() => {
		getBillingUsage<{ balance_micros?: number; error?: string; unlinked?: boolean }>()
			.then((u) => (balance = !u.error && !u.unlinked && typeof u.balance_micros === 'number' ? u.balance_micros : null))
			.catch(() => {
				// The month stands without the balance.
				balance = null;
			});
	});

	const monthStart = $derived(summary ? new Date(summary.month_start) : new Date());
	const monthName = $derived(monthStart.toLocaleDateString(undefined, { month: 'long', timeZone: 'UTC' }));
	const lastMonthName = $derived(
		new Date(Date.UTC(monthStart.getUTCFullYear(), monthStart.getUTCMonth() - 1, 1)).toLocaleDateString(undefined, {
			month: 'long',
			timeZone: 'UTC',
		}),
	);

	/** What the wallet paid this month; own-key calls add nothing they can't price. */
	const monthMicros = $derived((summary?.by_feature ?? []).reduce((a, b) => a + b.cost_micros, 0));
	const byoCalls = $derived((summary?.by_feature ?? []).reduce((a, b) => a + b.byo_calls, 0));

	/** Every day of the month as a slot; days to come are empty. */
	const days = $derived.by(() => {
		const byDay = new Map((summary?.by_day ?? []).map((d) => [d.day, d]));
		const year = monthStart.getUTCFullYear();
		const month = monthStart.getUTCMonth();
		const length = new Date(Date.UTC(year, month + 1, 0)).getUTCDate();
		const today = new Date().toISOString().slice(0, 10);
		return Array.from({ length }, (_, i) => {
			const date = new Date(Date.UTC(year, month, i + 1));
			const key = date.toISOString().slice(0, 10);
			const row = byDay.get(key);
			return {
				key,
				cost: row?.cost_micros ?? 0,
				calls: row?.calls ?? 0,
				today: key === today,
				future: key > today,
				label: date.toLocaleDateString(undefined, { month: 'short', day: 'numeric', timeZone: 'UTC' }),
			};
		});
	});
	const todayIndex = $derived(days.findIndex((d) => d.today));
	const dayMax = $derived(Math.max(1, ...days.map((d) => d.cost)));

	// Chart geometry: one slot per day across 680, bars 64% of a slot, a plot
	// 80 tall over the baseline at y=100.
	const W = 680;
	const slot = $derived(W / Math.max(1, days.length));
	const barW = $derived(Math.max(4, slot * 0.64));
	const barX = (i: number) => i * slot + (slot - barW) / 2;
	const barH = (cost: number) => (cost > 0 ? Math.max(2, (cost / dayMax) * 80) : 0);

	// The kinds: the six that cost most, then the rest as one line.
	const KINDS_SHOWN = 6;
	const kinds = $derived.by(() => {
		const all = [...(summary?.by_feature ?? [])].sort((a, b) => b.cost_micros - a.cost_micros || b.calls - a.calls);
		if (all.length <= KINDS_SHOWN + 1) return all.map((b) => ({ name: featureLabel(b.label), ...b }));
		const rest = all.slice(KINDS_SHOWN);
		return [
			...all.slice(0, KINDS_SHOWN).map((b) => ({ name: featureLabel(b.label), ...b })),
			{
				name: 'Everything else',
				label: 'rest',
				calls: rest.reduce((a, b) => a + b.calls, 0),
				cost_micros: rest.reduce((a, b) => a + b.cost_micros, 0),
				byo_calls: rest.reduce((a, b) => a + b.byo_calls, 0),
			},
		];
	});
	const kindMax = $derived(Math.max(1, ...kinds.map((k) => k.cost_micros)));
</script>

<SettingsPage title="What AI has cost" lede="This month, what it went to, and every call." back={{ route: '/virtues/assistant/spending', label: 'Models and spending' }}>
	{#if summaryError}
		<div class="failed" role="alert">
			<p>{summaryError}</p>
			<Button variant="secondary" onclick={loadSummary}>Try again</Button>
		</div>
	{:else if !summary}
		<p class="quiet">Reading your server's call log…</p>
	{:else}
		<section class="month" aria-label="This month">
			<p class="figure"><!-- Precise, as the kinds and calls are, so a month under a cent doesn't read as nothing. -->
			<span class="amount">{formatMicrosPrecise(monthMicros)}</span> <span class="when">so far in {monthName}</span></p>
			<p class="beside">
				{#if summary.last_month_to_date_micros !== null}
					{summary.last_month_to_date_micros > 0
						? `${formatMicrosUSD(summary.last_month_to_date_micros)} by this day in ${lastMonthName}.`
						: `Nothing by this day in ${lastMonthName}.`}
				{/if}
				{#if balance !== null}
					Your balance is {formatMicrosUSD(balance)} on
					<!-- Billing's tab label has no assistant name in it, so the default stands in. -->
					<button type="button" class="link" onclick={() => openSettingsPage('/virtues/billing', DEFAULT_ASSISTANT_NAME)}>Billing</button>.
				{/if}
				{#if byoCalls > 0}
					Calls on your own key aren't priced here.
				{/if}
			</p>

			<svg class="chart" viewBox="0 0 {W} 124" role="img" aria-label="What AI cost each day of {monthName}">
				{#each days as d, i (d.key)}
					{#if !d.future && d.cost > 0}
						<rect class="bar" class:today={d.today} x={barX(i)} y={100 - barH(d.cost)} width={barW} height={barH(d.cost)} rx="3">
							<title>{d.today ? 'Today' : d.label} · {formatMicrosPrecise(d.cost)}</title>
						</rect>
					{/if}
				{/each}
				<line class="base" x1="0" x2={W} y1="100.5" y2="100.5" />
				{#if todayIndex >= 0 && days[todayIndex].cost > 0}
					<text class="value" x={barX(todayIndex) + barW / 2} y={100 - barH(days[todayIndex].cost) - 7} text-anchor="middle">
						{formatMicrosPrecise(days[todayIndex].cost)}
					</text>
				{/if}
				<text x="0" y="118">{days[0]?.label}</text>
				{#if todayIndex > 2 && todayIndex < days.length - 3}
					<text x={barX(todayIndex) + barW / 2} y="118" text-anchor="middle">Today</text>
				{/if}
				<text x={W} y="118" text-anchor="end">{todayIndex === days.length - 1 ? 'Today' : days[days.length - 1]?.label}</text>
			</svg>
		</section>

		{#if kinds.length}
			<section aria-labelledby="went-to">
				<h2 id="went-to">What it went to</h2>
				<ul class="kinds">
					{#each kinds as k (k.label)}
						<li>
							<span class="kind-name">{k.name}</span>
							<span class="track"><span class="fill" style:width="{(k.cost_micros / kindMax) * 100}%"></span></span>
							<!-- All on your key: a count of nothing priced, so no amount. -->
							<span class="kind-amount">{k.byo_calls === k.calls ? 'on your key' : formatMicrosPrecise(k.cost_micros)}</span>
						</li>
					{/each}
				</ul>
			</section>
		{/if}
	{/if}

	<section aria-labelledby="every-call">
		<h2 id="every-call">Every call</h2>
		<UsageView />
	</section>
</SettingsPage>

<style>
	.month {
		margin-top: 32px;
	}

	.figure {
		display: flex;
		align-items: baseline;
		gap: 16px;
		margin: 0;
	}

	.amount {
		font-family: var(--font-serif);
		font-size: 48px;
		line-height: 1;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground);
	}

	.when {
		font-size: 15px;
		color: var(--color-foreground-muted);
	}

	.beside {
		margin: 8px 0 0;
		font-size: 13px;
		line-height: 1.5;
		color: var(--color-foreground-subtle);
	}

	.link {
		padding: 0;
		border: none;
		background: none;
		font: inherit;
		color: var(--color-primary);
		cursor: pointer;
	}

	.link:hover {
		text-decoration: underline;
	}

	.chart {
		display: block;
		width: 100%;
		height: auto;
		margin-top: 28px;
		overflow: visible;
	}

	.bar {
		fill: color-mix(in srgb, var(--color-primary) 28%, var(--color-background));
	}

	.bar.today {
		fill: var(--color-primary);
	}

	.base {
		stroke: var(--color-border);
		stroke-width: 1;
	}

	.chart text {
		font-family: var(--font-sans);
		font-size: 12px;
		fill: var(--color-foreground-subtle);
	}

	.chart .value {
		fill: var(--color-foreground);
	}

	h2 {
		margin: 44px 0 12px;
		font-size: 22px;
		font-weight: 400;
		line-height: 1.3;
		color: var(--color-foreground);
	}

	.kinds {
		display: flex;
		flex-direction: column;
		gap: 12px;
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.kinds li {
		display: grid;
		grid-template-columns: 180px minmax(0, 1fr) 88px;
		align-items: center;
		gap: 16px;
	}

	.kind-name {
		font-size: 15px;
		color: var(--color-foreground);
	}

	.track {
		height: 6px;
		border-radius: 3px;
		background: var(--color-surface-elevated);
	}

	.fill {
		display: block;
		height: 6px;
		min-width: 2px;
		border-radius: 3px;
		background: var(--color-primary);
	}

	.kind-amount {
		text-align: right;
		font-size: 14px;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground);
	}

	.quiet {
		margin: 32px 0 0;
		font-size: 14px;
		color: var(--color-foreground-muted);
	}

	.failed {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 8px;
		margin-top: 32px;
		font-size: 14px;
		color: var(--color-foreground-muted);
	}

	.failed p {
		margin: 0;
	}

	@media (max-width: 640px) {
		.kinds li {
			grid-template-columns: minmax(0, 1fr) 88px;
		}

		.track {
			grid-column: 1 / -1;
			order: 3;
		}
	}
</style>
