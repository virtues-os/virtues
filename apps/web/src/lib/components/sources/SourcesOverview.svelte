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
	import { getStreamHealth, type StreamHealth } from '$lib/api/client';
	import { sourcesStore } from '$lib/stores/sources.svelte';
	import { connectFlow } from '$lib/stores/connectFlow.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { relativeTime } from '$lib/applets/palette';

	const store = sourcesStore;

	let streams = $state<StreamHealth[]>([]);
	let streamsErr = $state<string | null>(null);
	let refreshing = $state(false);

	async function loadStreams() {
		refreshing = true;
		try {
			streams = await getStreamHealth();
			streamsErr = null;
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

	// Two questions, and the page used to answer them in one column, which is
	// why it read as noise:
	//
	//   1. CONNECTEDNESS — is anything hooked up that could produce this at all?
	//   2. STREAMING     — given it is, has anything arrived, and when?
	//
	// Only (2) is answerable from stream health. `total == 0` means "no rows
	// ever", which could equally be "nothing provides this" or "an iPhone is
	// paired with location switched off" — so the old `never` → "not connected"
	// was a claim the box could not actually substantiate. Answering (1) needs a
	// map from ontology to the sources that write it; until that exists these
	// rows are listed as simply not having arrived yet, which is true.
	const COPY: Record<string, string> = {
		live: 'flowing',
		stalled: 'stopped',
		idle: 'quiet this week'
	};

	/**
	 * Streams grouped by the life-domain already encoded in their name — a
	 * stream is `<domain>_<thing>` (`health_sleep`, `financial_account`), so the
	 * buckets exist without inventing a taxonomy.
	 *
	 * Grouping matters more than it looks. A flat list of nineteen rows answers
	 * "is this one flowing" and nothing else; grouped, it answers the question
	 * people actually have — *what does the record know about my life* — and
	 * makes an entirely dark domain visible as a fact rather than as fourteen
	 * greyed rows you scroll past.
	 *
	 * Deliberately not a score. "Health 2/5" reads as a deficit to close, and
	 * completionism on this product means pressuring someone into more
	 * self-surveillance to fill a bar. A domain nobody connected may be a
	 * decision, not a gap — so it is stated once, quietly, with no number.
	 */
	const DOMAIN_LABEL: Record<string, string> = {
		health: 'Health',
		location: 'Location',
		communication: 'Communication',
		calendar: 'Calendar',
		activity: 'Activity',
		content: 'Content',
		financial: 'Finance',
		audio: 'Audio'
	};

	// Fixed order, not worst-first. The vitals above already lead with what needs
	// you; this is the reference list, and a reference list that reorders itself
	// under you is harder to read than one that sits still.
	const DOMAIN_ORDER = [
		'health',
		'location',
		'communication',
		'calendar',
		'activity',
		'content',
		'financial',
		'audio'
	];

	// `or` for the negative half: "no email and transcriptions" reads as one
	// missing pair, "no email or transcriptions" as two missing things.
	function joinNames(items: string[], conj: 'and' | 'or' = 'and'): string {
		if (items.length === 1) return items[0];
		if (items.length === 2) return `${items[0]} ${conj} ${items[1]}`;
		return `${items.slice(0, -1).join(', ')}, ${conj} ${items[items.length - 1]}`;
	}

	const domains = $derived.by(() => {
		const by = new Map<string, StreamHealth[]>();
		for (const s of streams) {
			const domain = s.name.split('_')[0];
			const list = by.get(domain);
			if (list) list.push(s);
			else by.set(domain, [s]);
		}
		const known = DOMAIN_ORDER.filter((d) => by.has(d));
		// Anything the registry grows that this list hasn't heard of still shows.
		const rest = [...by.keys()].filter((d) => !DOMAIN_ORDER.includes(d)).sort();
		return [...known, ...rest].map((d) => {
			const group = by.get(d) ?? [];
			// The split that makes the page legible: a stream that has delivered
			// gets a liveness state, one that never has gets named once and left
			// alone. Applying "stopped"/"quiet" to something that never started
			// is what made nineteen rows read as nineteen problems.
			const arrived = group.filter((s) => s.total > 0);
			const none = group.filter((s) => s.total === 0);
			// The three reasons a stream is empty, which the old single "not
			// connected" label ran together. Only the middle one is actionable,
			// and only the middle one should read like an offer.
			// `?? []` is not defensive programming for its own sake: a box running
			// a build older than these fields returns neither, and the page must
			// degrade to "nothing arrived" rather than throw on a missing array.
			const providers = (s: StreamHealth) => s.provided_by ?? [];
			const silent = none.filter((s) => s.connected === true);
			const available = none.filter((s) => !s.connected && providers(s).length > 0);
			const unsupported = none.filter((s) => !s.connected && providers(s).length === 0);
			// One offer line per source, so "Email or Calendar — connect Google"
			// rather than a line per stream.
			const offers = new Map<string, string[]>();
			for (const s of available) {
				for (const p of providers(s)) {
					const list = offers.get(p);
					if (list) list.push(s.display_name);
					else offers.set(p, [s.display_name]);
				}
			}
			return {
				id: d,
				label: DOMAIN_LABEL[d] ?? d.charAt(0).toUpperCase() + d.slice(1),
				arrived,
				silent,
				offers: [...offers].map(([source, names]) => ({ source, names })),
				unsupported,
				dark: arrived.length === 0,
				attention: group.some((s) => s.status === 'stalled')
			};
		});
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

	/** Start the connect flow from a source's display name, which is what the
	 *  stream map hands back. */
	async function connectNamed(displayName: string) {
		const source = store.catalog.find((s) => s.name === displayName);
		if (source) await connectFlow.start(source);
	}

	function openSource(sourceId: string) {
		windowShellStore.navigate(`/sources/${sourceId}`, { label: store.sourceLabel(sourceId) });
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

<Page title="Sources" description="The state of supply." maxWidth="wide">
	{#snippet actions()}
		<div class="head-actions">
			<span class="live" class:on={flowing.length > 0}>
				<span class="dot"></span>{lastSeen ? relativeTime(lastSeen) : '—'}
			</span>
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
		<h2 class="chapter-title">Vitals</h2>
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
			<h2 class="chapter-title">Needs you</h2>
			<ul class="attention">
				{#each store.broken as c (c.id)}
					<li>
						<Icon icon="ri:error-warning-line" width="16" />
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

	<!-- ─── DATA FLOW ───────────────────────────────────────────────────── -->
	<section class="chapter">
		<h2 class="chapter-title">
			Data flow
			<button
				type="button"
				class="refresh"
				onclick={() => void loadStreams()}
				aria-label="Refresh"
				disabled={refreshing}
			>
				<Icon icon="ri:refresh-line" width="14" />
			</button>
		</h2>

		{#if streamsErr}
			<div class="error">{streamsErr}</div>
		{:else if streams.length === 0}
			<p class="muted">No streams registered.</p>
		{:else}
			{#each domains as d (d.id)}
				<div class="domain" class:dark={d.dark}>
					<div class="domain-head">
						<span class="domain-name">{d.label}</span>
						{#if d.attention}
							<span class="domain-flag">stopped</span>
						{/if}
					</div>
					{#if d.arrived.length > 0}
						<div class="ledger">
							{#each d.arrived as s (s.name)}
								<div class="ledger-row {s.status}">
									<span class="ledger-label">{s.display_name}</span>
									<span class="leader"></span>
									<span class="ledger-state">{COPY[s.status] ?? s.status}</span>
									<span class="ledger-value mono">
										{s.last_ingest ? relativeTime(s.last_ingest) : '—'}
									</span>
									<span class="ledger-count mono">{s.count_24h > 0 ? s.count_24h : ''}</span>
								</div>
							{/each}
						</div>
					{/if}
					<!-- Connected and capable, nothing arrived. Usually a permission
					     on the device, so no "connect" offer would help. -->
					{#if d.silent.length > 0}
						<p class="aside">
							Waiting on {joinNames(
								d.silent.map((s) => s.display_name),
								'and'
							)} — connected, nothing delivered yet.
						</p>
					{/if}

					<!-- The one actionable line: something in the catalog fills this. -->
					{#each d.offers as o (o.source)}
						<p class="aside offer">
							{joinNames(o.names, 'and')} —
							<button type="button" class="inline" onclick={() => connectNamed(o.source)}>
								connect {o.source}
							</button>
						</p>
					{/each}

					{#if d.unsupported.length > 0}
						<p class="aside quiet">
							No source for {joinNames(
								d.unsupported.map((s) => s.display_name),
								'or'
							)} yet.
						</p>
					{/if}
				</div>
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
		margin-top: 1.75rem;
	}
	.chapter:first-of-type {
		margin-top: 1rem;
	}
	.chapter-title {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin: 0 0 0.75rem;
		font-size: 0.6875rem;
		font-weight: 600;
		color: var(--color-foreground-subtle, #9ca3af);
	}
	.refresh {
		display: inline-flex;
		border: none;
		background: none;
		padding: 0;
		color: inherit;
		cursor: pointer;
		opacity: 0.7;
	}
	.refresh:hover:not(:disabled) {
		opacity: 1;
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
		font-size: 0.75rem;
		color: var(--color-foreground-muted, #6b7280);
	}
	.vital-figure {
		display: flex;
		align-items: baseline;
		gap: 0.25rem;
		margin-top: 0.375rem;
	}
	.vital-big {
		font-family: var(--font-serif, ui-serif, Georgia, serif);
		font-size: 1.875rem;
		line-height: 1;
		/* Lining + tabular so 0 and 10 occupy the same width and the four cards
		   agree on a baseline grid. Optical sizing keeps the serif from going
		   spindly at display size. */
		font-variant-numeric: lining-nums tabular-nums;
		letter-spacing: -0.015em;
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
		margin-top: 0.25rem;
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle, #9ca3af);
	}
	.mono {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-variant-numeric: tabular-nums;
		font-feature-settings: 'ss01';
	}

	/* ── Domains ──────────────────────────────────────────────────────── */
	.domain {
		margin-bottom: 1.25rem;
	}
	.domain:last-child {
		margin-bottom: 0;
	}
	.domain-head {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
	}
	.domain-name {
		font-size: 0.8125rem;
		font-weight: 600;
		color: var(--color-foreground, #111827);
	}
	.domain-flag {
		font-size: 0.6875rem;
		color: var(--color-error);
	}
	/* A domain nobody has connected is a fact, not a failure — quieter, but
	   still listed, because "you could have this" is half the point. */
	.domain.dark .domain-name {
		color: var(--color-foreground-muted, #6b7280);
		font-weight: 500;
	}

	/* The three not-yet lines. Same size, decreasing ink: an offer you can act
	   on, a wait you cannot, and a limit that is ours rather than yours. */
	.aside {
		margin: 0.25rem 0 0;
		font-size: 0.75rem;
		line-height: 1.5;
		color: var(--color-foreground-muted, #6b7280);
	}
	.aside.quiet {
		color: var(--color-foreground-subtle, #9ca3af);
	}
	.inline {
		border: none;
		background: none;
		padding: 0;
		font: inherit;
		color: var(--color-primary);
		cursor: pointer;
	}
	.inline:hover {
		text-decoration: underline;
		text-underline-offset: 2px;
	}

	/* ── Ledger ───────────────────────────────────────────────────────── */
	.ledger {
		display: flex;
		flex-direction: column;
	}
	.ledger-row {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
		padding: 0.3125rem 0;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted, #6b7280);
	}
	.ledger-label {
		color: var(--color-foreground, #111827);
	}
	/* The dot leader — same device the CLI uses to tie a name to its value. */
	/* The dot leader, as a printed index does it: dots fine enough to read as
	   texture rather than as a dashed border, sitting on the x-height rather
	   than the baseline so the eye tracks along the middle of the line. */
	.leader {
		flex: 1;
		min-width: 2rem;
		height: 1px;
		align-self: center;
		background-image: radial-gradient(
			circle,
			color-mix(in srgb, var(--color-foreground) 26%, transparent) 0.5px,
			transparent 0.5px
		);
		background-size: 4px 1px;
		background-repeat: repeat-x;
		opacity: 0.85;
	}
	.ledger-state {
		flex-shrink: 0;
		width: 9rem;
		font-size: 0.75rem;
	}
	.ledger-row.stalled .ledger-state {
		color: var(--color-error);
		font-weight: 500;
	}
	.ledger-row.idle .ledger-state {
		color: var(--color-warning, #d97706);
	}
	.ledger-row.never .ledger-state,
	.ledger-row.never .ledger-label {
		color: var(--color-foreground-subtle, #9ca3af);
	}
	.ledger-value {
		flex-shrink: 0;
		width: 6rem;
		text-align: right;
		font-size: 0.75rem;
	}
	.ledger-count {
		flex-shrink: 0;
		width: 4rem;
		text-align: right;
		font-size: 0.75rem;
		color: var(--color-foreground-subtle, #9ca3af);
	}

	/* ── Attention ────────────────────────────────────────────────────── */
	.attention {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}
	.attention li {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		padding: 0.625rem 0.75rem;
		border-radius: 8px;
		border: 1px solid color-mix(in srgb, var(--color-error) 30%, transparent);
		background: var(--color-error-subtle);
		color: color-mix(in srgb, var(--color-error) 75%, #000);
	}
	.what {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
	}
	.who {
		font-size: 0.8125rem;
		font-weight: 600;
	}
	.why {
		font-size: 0.75rem;
		opacity: 0.85;
	}
	.act {
		flex-shrink: 0;
		padding: 0.3125rem 0.75rem;
		border-radius: 6px;
		border: 1px solid currentColor;
		background: transparent;
		color: inherit;
		font-size: 0.75rem;
		font-weight: 600;
		cursor: pointer;
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

	@media (max-width: 640px) {
		.ledger-count {
			display: none;
		}
	}
</style>
