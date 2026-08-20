<!--
	Sources → Overview. The state of supply, examined.

	Modelled on the System page: chapters, a row of vitals with one big figure
	each, and a dot-leader ledger underneath. Same reason it works there — the
	numbers you check repeatedly want to be readable at a glance and in the same
	place every time, and the detail wants to read like a log rather than a
	dashboard.

	The page this replaces opened with three lines defining the word "source",
	which you need once, above the two things you came back for. Attention leads,
	because a stalled stream is a fact to read but a locked-out credential is a
	job only you can do.
-->
<script lang="ts">
	import { Page } from '$lib';
	import Icon from '$lib/components/Icon.svelte';
	import { getStreamHealth, getStreamDays, type StreamHealth, type StreamDays } from '$lib/api/client';
	import StreamGrid, { type GridRow } from './StreamGrid.svelte';
	import { domainOf, domainLabel, domainRank } from '$lib/sources/domains';
	import { sourcesStore } from '$lib/stores/sources.svelte';
	import { connectFlow } from '$lib/stores/connectFlow.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { relativeTime } from '$lib/applets/palette';

	const store = sourcesStore;

	let streams = $state<StreamHealth[]>([]);
	let dayRows = $state<StreamDays[]>([]);
	/** UTC date `days[0]` refers to, from the server — see StreamGrid. */
	let gridStart = $state<string | null>(null);
	// The grid endpoint is newer than the health one; a box that predates it
	// must not make the page claim nothing ever arrived.
	let daysUnavailable = $state(false);
	let streamsErr = $state<string | null>(null);
	let refreshing = $state(false);

	async function loadStreams() {
		refreshing = true;
		try {
			// Two endpoints, two failure modes. Settled rather than all-or-nothing
			// so a box that predates /streams/days still renders its vitals
			// instead of reporting one error for both.
			const [health, days] = await Promise.allSettled([
				getStreamHealth(),
				getStreamDays(84)
			]);
			if (health.status === 'fulfilled') streams = health.value;
			if (days.status === 'fulfilled') {
				dayRows = days.value.streams;
				gridStart = days.value.start;
			} else {
				dayRows = [];
				gridStart = null;
			}
			daysUnavailable = days.status === 'rejected';
			streamsErr =
				health.status === 'rejected'
					? health.reason instanceof Error
						? health.reason.message
						: String(health.reason)
					: null;
		} catch (e) {
			streamsErr = e instanceof Error ? e.message : String(e);
		} finally {
			refreshing = false;
		}
	}

	$effect(() => {
		void store.load();
		void loadStreams();
	});

	// Sections are life-domains, not vendors. This page answers "what does the
	// box hold, and is it still arriving" — a question asked in the vocabulary
	// of a life, where Health is a category and Google is not. Each row still
	// names the provider that wrote it (see StreamGrid), so the "go look at that
	// device" reading that grouping-by-source gave for free survives.
	//
	// What is *missing* is the opposite question with the opposite answer
	// ("connect something") and lives in the catalog — which is why no prose
	// about absence remains on this page.
	const byDomain = $derived.by(() => {
		const groups = new Map<string, GridRow[]>();
		for (const row of dayRows) {
			const domain = domainOf(row.name);
			// The provider is a source id on the data; name it the way the
			// catalog does when we know it, and leave it verbatim when we don't —
			// a row written by something the catalog never heard of is still a
			// true row, and hiding it would be the bigger lie.
			const enriched: GridRow = {
				...row,
				providerLabel: store.catalogById.get(row.provider)?.name ?? row.provider
			};
			const list = groups.get(domain);
			if (list) list.push(enriched);
			else groups.set(domain, [enriched]);
		}
		return [...groups]
			.map(([domain, rows]) => ({
				domain,
				label: domainLabel(domain),
				rows: rows
					.slice()
					.sort(
						(a, b) =>
							a.display_name.localeCompare(b.display_name) ||
							(a.providerLabel ?? '').localeCompare(b.providerLabel ?? '')
					)
			}))
			.sort((a, b) => domainRank(a.domain) - domainRank(b.domain) || a.label.localeCompare(b.label));
	});

	const connected = $derived(streams.filter((s) => s.total > 0));
	const flowing = $derived(streams.filter((s) => s.status === 'live'));
	const stalled = $derived(streams.filter((s) => s.status === 'stalled'));
	const recordsToday = $derived(streams.reduce((n, s) => n + s.count_24h, 0));
	const attention = $derived(store.broken.length + stalled.length);

	const lastSeen = $derived.by(() => {
		const stamps = streams.map((s) => s.last_ingest).filter((v): v is string => v !== null);
		if (stamps.length === 0) return null;
		return stamps.reduce((a, b) => (a > b ? a : b));
	});

	// The OAuth callback returns as `?connected=<id>` or `?source=<id>&error=…`.
	// Nothing read either, so a round trip through a provider ended in silence
	// whichever way it went. Read once at mount and strip, so a refresh cannot
	// replay a stale verdict.
	const connectReturn = (() => {
		if (typeof window === 'undefined') return null;
		const p = new URLSearchParams(window.location.search);
		const ok = p.get('connected');
		const error = p.get('error');
		const source = p.get('source');
		if (!ok && !error) return null;
		for (const k of ['connected', 'error', 'source']) p.delete(k);
		const qs = p.toString();
		window.history.replaceState({}, '', window.location.pathname + (qs ? `?${qs}` : ''));
		return { ok, error, source };
	})();

	let noticeDismissed = $state(false);

	const notice = $derived.by(() => {
		if (!connectReturn || noticeDismissed) return null;
		if (connectReturn.ok) {
			return { good: true, text: `${store.sourceLabel(connectReturn.ok)} is connected.` };
		}
		const who = connectReturn.source ? store.sourceLabel(connectReturn.source) : 'That source';
		return {
			good: false,
			text:
				connectReturn.error === 'connect_cancelled'
					? `${who} wasn't connected — the flow was closed before it finished.`
					: `Couldn't finish connecting ${who}. Nothing was connected.`
		};
	});

	async function reconnect(sourceId: string) {
		const source = store.catalogById.get(sourceId);
		if (source) await connectFlow.start(source);
	}

	function openCatalog() {
		windowShellStore.navigate('/sources/catalog', { label: 'Sources · Catalog' });
	}

</script>

{#snippet vital(name: string, figure: string, unit: string, sub: string, tone = '')}
	<div class="vital">
		<div class="vital-head"><span class="vital-name">{name}</span></div>
		<div class="vital-figure">
			<span class="vital-big {tone}">{figure}</span>
			{#if unit}<span class="vital-unit">{unit}</span>{/if}
		</div>
		<div class="vital-sub">{sub}</div>
	</div>
{/snippet}

<Page
	title="Sources"
	description="Where your data comes from, and whether it is still arriving."
	maxWidth="wide"
>
	{#snippet actions()}
		<div class="head-actions">
			<span class="live" class:on={flowing.length > 0}>
				<span class="dot"></span>{lastSeen ? relativeTime(lastSeen) : '—'}
			</span>
			<button
				type="button"
				class="ghost icon-only"
				onclick={() => void loadStreams()}
				aria-label="Refresh"
				disabled={refreshing}
			>
				<Icon icon="ri:refresh-line" width="15" />
			</button>
			<button type="button" class="ghost" onclick={openCatalog}>
				<Icon icon="ri:apps-line" width="15" /> Catalog
			</button>
		</div>
	{/snippet}

	{#if store.error}<div class="error">{store.error}</div>{/if}
	{#if connectFlow.error}<div class="error">{connectFlow.error}</div>{/if}

	{#if notice}
		<div class="notice" class:ok={notice.good}>
			<Icon icon={notice.good ? 'ri:check-line' : 'ri:information-line'} width="16" />
			<span>{notice.text}</span>
			<button type="button" class="x" onclick={() => (noticeDismissed = true)} aria-label="Dismiss">
				<Icon icon="ri:close-line" width="15" />
			</button>
		</div>
	{/if}

	<!-- ─── VITALS ──────────────────────────────────────────────────────── -->
	<section class="chapter">
		<div class="vitals-grid">
			{@render vital(
				'Connections',
				String(store.connections.length),
				'',
				store.bySource.size === 1 ? 'across 1 source' : `across ${store.bySource.size} sources`
			)}
			{@render vital(
				'Streams flowing',
				String(flowing.length),
				`/ ${connected.length}`,
				connected.length === 0 ? 'nothing connected yet' : 'delivered in the last 24h'
			)}
			{@render vital(
				'Records today',
				recordsToday.toLocaleString(),
				'',
				'written in the last 24h'
			)}
			{@render vital(
				'Needs you',
				String(attention),
				'',
				attention === 0 ? 'nothing to do' : 'connections and streams below',
				attention > 0 ? 'crit' : 'ok'
			)}
		</div>
	</section>

	<!-- ─── ATTENTION ───────────────────────────────────────────────────── -->
	{#if store.broken.length > 0}
		<section class="chapter">
			<h2 class="section">Needs you</h2>
			<ul class="attention">
				{#each store.broken as c (c.id)}
					<li>
						<span class="mark" aria-hidden="true"></span>
						<div class="what">
							<span class="who">{store.sourceLabel(c.sourceId)} · {c.name}</span>
							<span class="why">{c.statusReason ?? 'This connection stopped working.'}</span>
						</div>
						{#if c.kind === 'credential'}
							<button type="button" class="act" onclick={() => void reconnect(c.sourceId)}>
								Reconnect
							</button>
						{/if}
					</li>
				{/each}
			</ul>
		</section>
	{/if}

	<!-- ─── ARRIVALS ────────────────────────────────────────────────────── -->
	<section class="chapter">
		{#if streamsErr}
			<div class="error">{streamsErr}</div>
		{:else if daysUnavailable}
			<p class="muted">This box is running a build without the arrivals grid yet.</p>
		{:else if byDomain.length === 0}
			<p class="muted">
				Nothing has arrived yet. The <button type="button" class="inline" onclick={openCatalog}
					>catalog</button
				> lists everything Virtues can draw from.
			</p>
		{:else}
			{#each byDomain as g (g.domain)}
				<StreamGrid title={g.label} streams={g.rows} start={gridStart} />
			{/each}
		{/if}
	</section>

</Page>

<style>
	/* ── Head ─────────────────────────────────────────────────────────── */
	.head-actions {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}
	.live {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		font-size: 0.6875rem;
		font-family: var(--font-mono, ui-monospace, monospace);
		color: var(--color-foreground-subtle, #9ca3af);
	}
	.live .dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--color-foreground-subtle, #9ca3af);
	}
	.live.on .dot {
		background: var(--color-success, #16a34a);
	}
	.ghost {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0.3125rem 0.625rem;
		border-radius: 6px;
		border: 1px solid var(--color-border, #d1d5db);
		background: var(--color-background, #fff);
		color: var(--color-foreground, #111827);
		font-size: 0.75rem;
		font-weight: 500;
		cursor: pointer;
	}
	.ghost:hover {
		background: var(--color-muted, #f3f4f6);
	}

	/* ── Chapters ─────────────────────────────────────────────────────── */
	.chapter {
		margin-top: 2rem;
	}
	.chapter:first-of-type {
		margin-top: 1rem;
	}
	/* A section heading must outrank the rows under it. These were 11px serif —
	   smaller than their own body text — which is why nothing read as
	   structure. */
	.section {
		margin: 0;
		font-size: 0.9375rem;
		font-weight: 600;
		letter-spacing: -0.006em;
		color: var(--color-foreground, #111827);
	}
	.section {
		margin-bottom: 0.625rem;
	}
	.icon-only {
		padding: 0.3125rem 0.4375rem;
	}

	/* ── Vitals ───────────────────────────────────────────────────────── */
	.vitals-grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(11rem, 1fr));
		gap: 0.75rem;
	}
	.vital {
		padding: 0.9375rem 1.0625rem 1rem;
		border: 1px solid color-mix(in srgb, var(--color-foreground) 9%, transparent);
		border-radius: 9px;
	}
	@media (min-resolution: 2dppx) {
		/* Where the display can render it, take the rule below 1px so the card
		   reads as an edge rather than as a drawn box. */
		.vital {
			border-width: 0.5px;
			border-color: color-mix(in srgb, var(--color-foreground) 14%, transparent);
		}
	}
	.vital-name {
		font-size: 0.8125rem;
		color: var(--color-foreground-muted, #6b7280);
	}
	.vital-figure {
		display: flex;
		align-items: baseline;
		gap: 0.25rem;
		margin-top: 0.375rem;
	}
	.vital-big {
		font-size: 1.75rem;
		font-weight: 500;
		line-height: 1;
		/* Lining + tabular so 0 and 10 occupy the same width and the four cards
		   agree on a baseline grid. Optical sizing keeps the serif from going
		   spindly at display size. */
		font-variant-numeric: lining-nums tabular-nums;
		letter-spacing: -0.02em;
		color: var(--color-foreground, #111827);
	}
	.vital-big.crit {
		color: var(--color-error);
	}
	.vital-unit {
		font-variant-numeric: lining-nums tabular-nums;
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle, #9ca3af);
	}
	.vital-sub {
		margin-top: 0.3125rem;
		font-size: 0.75rem;
		color: var(--color-foreground-subtle, #9ca3af);
	}

	/* ── Attention ────────────────────────────────────────────────────── */
	/* One card with hairline seams, built like the vitals above it — not a
	   stack of red boxes.

	   The old treatment gave every row a red border, a red fill and red text:
	   severity stated three times, which is how a list of two broken
	   connections came to be the loudest thing on a page otherwise made of
	   hairlines and one accent. Colour now appears exactly once per row, as a
	   small mark, and carries all of it. */
	.attention {
		list-style: none;
		margin: 0;
		padding: 0;
		border: 1px solid color-mix(in srgb, var(--color-foreground) 9%, transparent);
		border-radius: 9px;
		overflow: hidden;
	}
	@media (min-resolution: 2dppx) {
		.attention {
			border-width: 0.5px;
			border-color: color-mix(in srgb, var(--color-foreground) 14%, transparent);
		}
	}
	.attention li {
		display: flex;
		align-items: center;
		gap: 0.6875rem;
		padding: 0.75rem 0.9375rem;
	}
	.attention li + li {
		border-top: 1px solid color-mix(in srgb, var(--color-foreground) 8%, transparent);
	}
	/* The one piece of colour. A dot rather than a warning glyph: the sentence
	   beside it already says what is wrong, and an icon repeating "warning"
	   next to the words is the third statement of the same fact. */
	.mark {
		flex-shrink: 0;
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--color-error);
	}
	.what {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
	}
	.who {
		font-size: 0.8125rem;
		font-weight: 500;
		color: var(--color-foreground, #111827);
	}
	.why {
		font-size: 0.75rem;
		line-height: 1.45;
		color: var(--color-foreground-muted, #6b7280);
	}
	/* The remedy, so it reads as the thing to do rather than as the damage:
	   the page's own quiet button, not an outline in the error colour. */
	.act {
		flex-shrink: 0;
		padding: 0.3125rem 0.75rem;
		border-radius: 6px;
		border: 1px solid var(--color-border, #d1d5db);
		background: var(--color-background, #fff);
		color: var(--color-foreground, #111827);
		font-size: 0.75rem;
		font-weight: 500;
		white-space: nowrap;
		cursor: pointer;
	}
	.act:hover {
		background: var(--color-muted, #f3f4f6);
	}

	/* ── Misc ─────────────────────────────────────────────────────────── */
	.error {
		padding: 0.5rem 0.75rem;
		margin-bottom: 0.75rem;
		border-radius: 6px;
		background: var(--color-error-subtle);
		color: color-mix(in srgb, var(--color-error) 75%, #000);
		font-size: 0.8125rem;
	}
	.muted {
		margin: 0;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted, #6b7280);
	}
	.notice {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.5rem 0.75rem;
		margin-bottom: 0.75rem;
		border-radius: 6px;
		font-size: 0.8125rem;
		background: var(--color-muted, #f3f4f6);
		color: var(--color-foreground, #111827);
	}
	.notice.ok {
		background: var(--color-success-subtle, #dcfce7);
		color: var(--color-success, #166534);
	}
	.notice span {
		flex: 1;
	}
	.x {
		display: inline-flex;
		border: none;
		background: transparent;
		color: inherit;
		opacity: 0.7;
		cursor: pointer;
		padding: 0;
	}

</style>
