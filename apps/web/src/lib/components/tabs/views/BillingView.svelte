<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import { Button, Input, Badge, SudoModal } from '$lib';
	import { subscriptionStore } from '$lib/stores/subscription.svelte';
	import { openExternal } from '$lib/tauri/bridge';
	import { formatMicrosUSD, formatMicrosPrecise } from '$lib/utils/currency';
	import { formatDate } from '$lib/utils/dateUtils';
	import {
		getBillingLinkStatus,
		startBillingLink,
		openBillingPortal as requestBillingPortal,
		getBillingState,
		setBillingAutoTopup,
		getBillingUsage,
		getByoKey,
		setByoKey,
		deleteByoKey,
		ApiError,
	} from '$lib/api/client';
	import Icon from '$lib/components/Icon.svelte';
	import { toast } from 'svelte-sonner';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	let portalLoading = $state(false);
	let portalError = $state<string | null>(null);

	// Device-authorization link flow (connect a paid subscription). The box
	// never holds a Stripe key: we start a link, open the Atlas-hosted checkout
	// URL, and poll the box until it has picked up the billing token.
	type LinkInfo = {
		user_code: string;
		verification_uri: string;
		verification_uri_complete: string;
		interval: number;
	};
	let linkLoading = $state(false);
	let linkError = $state<string | null>(null);
	let linkInfo = $state<LinkInfo | null>(null);
	let linkPolling = $state(false);
	let linkDone = $state(false);
	let pollTimer: ReturnType<typeof setInterval> | null = null;

	const isSubscribed = $derived(
		subscriptionStore.status === 'active' || subscriptionStore.status === 'trialing'
	);

	function stopPolling() {
		if (pollTimer) {
			clearInterval(pollTimer);
			pollTimer = null;
		}
	}

	function startPolling(intervalMs: number) {
		linkPolling = true;
		stopPolling();
		pollTimer = setInterval(async () => {
			try {
				const data = await getBillingLinkStatus<{ status: string }>();
				if (data.status === 'ready') {
					stopPolling();
					linkPolling = false;
					linkDone = true;
					// Re-fetch subscription state now that the box is linked.
					setTimeout(() => location.reload(), 1200);
				} else if (data.status === 'expired' || data.status === 'none') {
					stopPolling();
					linkPolling = false;
					linkInfo = null;
					linkError = 'The link expired before checkout completed — please try again.';
				}
			} catch {
				// transient; keep polling
			}
		}, intervalMs);
	}

	// Host the checkout tab will hand off to (e.g. atlas.virtues.com), surfaced
	// up front so the redirect isn't a surprise.
	const checkoutHost = $derived.by(() => {
		if (!linkInfo) return '';
		try {
			return new URL(linkInfo.verification_uri).host;
		} catch {
			return '';
		}
	});

	// Step 1: ask the box to start a link. We do NOT open the checkout tab here —
	// we show the code + destination first, then the user explicitly continues.
	async function connectSubscription() {
		linkLoading = true;
		linkError = null;
		linkDone = false;
		try {
			const data = await startBillingLink<LinkInfo & { error?: unknown }>();
			if (data.error) {
				linkError = typeof data.error === 'string' ? data.error : 'Failed to start subscription link';
				return;
			}
			linkInfo = data;
			// Poll from now on: covers both the "continue" button and the manual
			// enter-the-code path. The link self-expires (15 min) if unused.
			startPolling(Math.max((data.interval || 5) * 1000, 2000));
		} catch (e) {
			// Surface any server-provided error body (ApiError.message extracts it),
			// mirroring the old code that read data.error even on non-2xx.
			linkError = e instanceof ApiError ? e.message : 'Failed to connect to billing service';
		} finally {
			linkLoading = false;
		}
	}

	// Step 2: explicit hand-off to the Stripe-backed checkout, on user action.
	function proceedToCheckout() {
		if (linkInfo) openExternal(linkInfo.verification_uri_complete);
	}

	$effect(() => () => stopPolling());

	async function openBillingPortal() {
		portalLoading = true;
		portalError = null;
		try {
			const data = await requestBillingPortal<{ url?: string; error?: { message?: string } | string }>();
			if (data.url) {
				openExternal(data.url);
			} else if (data.error) {
				portalError = typeof data.error === 'string' ? data.error : data.error.message || 'Failed to open billing portal';
			}
		} catch (e) {
			portalError = e instanceof ApiError ? e.message : 'Failed to connect to billing service';
		} finally {
			portalLoading = false;
		}
	}

	// ─── Local billing-state (auto-top-up + BYO) ──────────────────────────
	type AutoTopupState = {
		enabled: boolean;
		failures_24h: number;
		disabled_at: string | null;
	};
	type ByoState = {
		configured: boolean;
		endpoint_url: string | null;
		/** Legacy label on credentials saved before the picker was removed. */
		provider: string | null;
		default_model: string | null;
	};
	type LocalBillingState = { auto_topup: AutoTopupState; byo: ByoState };

	let local = $state<LocalBillingState | null>(null);
	let localLoading = $state(true);

	async function loadLocal() {
		localLoading = true;
		try {
			local = await getBillingState<LocalBillingState>();
		} catch { /* swallow */ }
		localLoading = false;
	}

	async function setAutoTopup(enabled: boolean) {
		try {
			await setBillingAutoTopup(enabled);
			await loadLocal();
		} catch { /* swallow */ }
	}

	$effect(() => { void loadLocal(); });

	// ─── BYO AI (inline management, formerly ByoKeyView) ───────────────────
	// The escape hatch from the Virtues wallet, and as of 2026-08-05 a whole
	// door rather than half of one: `BearerClient` diverts every `/v1/ai/*`
	// call — streaming and not — so chat, compaction, day summaries, image
	// generation and transcription all leave by the user's endpoint.
	//
	// The copy here is load-bearing and has been wrong in both directions
	// within a single day. It claimed "Skip the Virtues wallet" while seven
	// callers still billed it, then got rewritten to say nothing read the key
	// at all — from grepping `BYO_SOURCE_ID`, which the routing reaches
	// through `load_byo_credential` and never names. If you change this text,
	// verify against api/settings_byo.rs and virtues_api/client.rs, not grep.
	//
	// A credential is a URL and a key. There is no provider picker: one
	// contract (OpenAI-style /chat/completions + bearer), so only the address
	// varies. Example URLs live in the markup as *copy*, never as <option>s —
	// a stale doc line is wrong, a stale option is a broken feature, which is
	// precisely how `anthropic` and `google` shipped pointing at APIs we
	// cannot call. Plan of record: agents/plan/byo-ai-plan.md.
	//
	// Save and Delete are both sudo-gated (`change_byo_key` is one of the four
	// locked sensitive actions); the SudoModal handles the prompt + CLI
	// approval round-trip.
	type ByoStatus = {
		configured: boolean;
		/** Legacy label on credentials saved before the picker was removed. */
		provider: string | null;
		/** Slot name → the model id on the user's endpoint. */
		models: Record<string, string>;
		default_model: string | null;
		endpoint_url: string | null;
		created_at: string | null;
	};

	/** Host of an endpoint URL, for display. Falls back to the raw string. */
	function byoHost(url: string): string {
		try {
			return new URL(url).host;
		} catch {
			return url;
		}
	}

	let byoOpen = $state(false);
	let byoStatus = $state<ByoStatus | null>(null);
	let byoLoading = $state(false);
	let byoLoadError = $state<string | null>(null);

	// Form state. No provider field — a credential is a URL and a key.
	let byoApiKey = $state('');
	let byoEndpointUrl = $state('');
	let byoModels = $state<Record<string, string>>({});

	/**
	 * The slot map form. Keys match the box's slot names exactly.
	 *
	 * `note` is where judgment about *which* model suits a role lives —
	 * deliberately here and not in the schema or the resolver, both of which
	 * treat all five slots identically. Model landscapes shift faster than we
	 * ship boxes, so guidance belongs somewhere a copy edit can fix.
	 */
	const SLOT_FIELDS = [
		{ key: 'chat', label: 'Chat', placeholder: 'x-ai/grok-4.5', note: '' },
		{ key: 'coding', label: 'Coding', placeholder: 'x-ai/grok-4.5', note: '' },
		{
			key: 'lite',
			label: 'Lite',
			placeholder: 'z-ai/glm-4.7',
			note: 'Titles, summaries, background jobs — high volume, so favor something cheap and quick.',
		},
		{ key: 'image', label: 'Image', placeholder: 'google/gemini-3-pro-image', note: '' },
		{
			key: 'omni',
			label: 'Audio',
			placeholder: 'google/gemini-3.5-flash',
			note: 'Transcription, and usually the largest line item. In practice Gemini 3 flash or flash-lite is the only workable choice: it has to hear muffled, in-pocket audio and reason about it, and the alternatives either reject audio outright or return words with no sense of the scene. Speech-to-text models will not do.',
		},
	] as const;

	// Sudo modal coordination — we mint a sudo request, then on approval we
	// fire the actual save/delete with the approved request id.
	let showSudoSave = $state(false);
	let showSudoDelete = $state(false);

	async function loadByo() {
		byoLoading = true;
		byoLoadError = null;
		try {
			byoStatus = await getByoKey<ByoStatus>();
			// Prefill so "Manage" edits the map instead of silently clearing it.
			if (byoStatus?.models) byoModels = { ...byoStatus.models };
		} catch (e) {
			byoLoadError = e instanceof Error ? e.message : 'Failed to load BYO status';
		} finally {
			byoLoading = false;
		}
	}

	function toggleByo() {
		byoOpen = !byoOpen;
		if (byoOpen && !byoStatus && !byoLoading) void loadByo();
	}

	function startByoSave() {
		if (!byoEndpointUrl.trim()) {
			toast.error('Paste the endpoint URL to send chat to');
			return;
		}
		if (!byoApiKey.trim()) {
			toast.error('Paste an API key first');
			return;
		}
		showSudoSave = true;
	}

	async function performByoSave(sudoRequestId: string) {
		try {
			// No `provider` — the box takes the URL as given. The field still
			// exists server-side to resolve credentials saved before the picker
			// was removed; nothing new should send it.
			// Blank rows are omitted, not sent as "": an absent slot means
			// "your endpoint uses our id", which is the right default.
			const models = Object.fromEntries(
				Object.entries(byoModels)
					.map(([k, v]) => [k, (v ?? '').trim()])
					.filter(([, v]) => v.length > 0),
			);
			await setByoKey({
				sudo_request_id: sudoRequestId,
				api_key: byoApiKey,
				endpoint_url: byoEndpointUrl,
				models,
			});
			toast.success('BYO key saved');
			byoApiKey = '';
			await Promise.all([loadByo(), loadLocal()]);
		} catch (e) {
			toast.error('Save failed', {
				description: e instanceof Error ? e.message : 'Unknown error',
			});
		}
	}

	function startByoDelete() {
		showSudoDelete = true;
	}

	async function performByoDelete(sudoRequestId: string) {
		try {
			await deleteByoKey(sudoRequestId);
			toast.success('BYO key removed — chat is back on your Virtues wallet');
			await Promise.all([loadByo(), loadLocal()]);
		} catch (e) {
			toast.error('Delete failed', {
				description: e instanceof Error ? e.message : 'Unknown error',
			});
		}
	}

	// ─── Wallet balance + recent ledger (proxied from virtues-api) ─────────
	type LedgerEntry = { ts: string; micros: number; kind: string; real_micros: number | null };
	type Usage = {
		balance_micros: number;
		month_to_date_micros: number;
		expires_at: string | null;
		entries: LedgerEntry[];
		error?: string;
	};
	let usage = $state<Usage | null>(null);

	async function loadUsage() {
		try {
			const data = await getBillingUsage<Usage>();
			// Ignore error payloads; never trust `entries` to be present.
			usage = data.error ? null : { ...data, entries: data.entries ?? [] };
		} catch { /* swallow — balance panel just hides */ }
	}

	$effect(() => { void loadUsage(); });

	const kindLabel: Record<string, string> = {
		grant: 'Monthly credit',
		topup: 'Top-up',
		charge: 'Usage',
		refund: 'Refund',
		adjust: 'Adjustment',
	};
	const kindIcon: Record<string, string> = {
		grant: 'ri:refresh-line',
		topup: 'ri:add-circle-line',
		charge: 'ri:sparkling-2-line',
		refund: 'ri:arrow-go-back-line',
		adjust: 'ri:equalizer-line',
	};

	// Human "renews" date from expiry.
	const renewsLabel = $derived(
		usage?.expires_at
			? formatDate(usage.expires_at, { month: 'long', day: 'numeric' })
			: null
	);
	function entryDate(ts: string): string {
		const d = new Date(ts);
		const today = new Date();
		const sameDay = d.toDateString() === today.toDateString();
		return sameDay
			? d.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' })
			: formatDate(ts, { month: 'short', day: 'numeric' });
	}

	const statusLabel: Record<string, string> = {
		active: 'Active',
		trialing: 'Trial',
		past_due: 'Past Due',
		expired: 'Expired',
	};

	const statusColor: Record<string, string> = {
		active: 'text-success',
		trialing: 'text-info',
		past_due: 'text-warning',
		expired: 'text-error',
	};
</script>

<!-- No <Page>: this is a SECTION of Plan now, not a page. See PlanView. -->
<div class="space-y-8">

		<!-- Subscription Status -->
		<div class="border border-border rounded-lg p-6 mb-6">
			<h2 class="settings-label">Subscription</h2>
			<div class="space-y-3">
				<div class="flex justify-between items-center">
					<span class="text-foreground-muted">Status</span>
					<span class="font-medium {statusColor[subscriptionStore.status] || 'text-foreground'}">
						{statusLabel[subscriptionStore.status] || subscriptionStore.status}
					</span>
				</div>

				{#if subscriptionStore.status === 'trialing' && subscriptionStore.daysRemaining !== null}
					<div class="flex justify-between items-center">
						<span class="text-foreground-muted">Trial ends</span>
						<span class="font-medium text-foreground">
							{subscriptionStore.daysRemaining} day{subscriptionStore.daysRemaining === 1 ? '' : 's'} remaining
						</span>
					</div>
				{/if}

				{#if subscriptionStore.trialExpiresAt}
					<div class="flex justify-between items-center">
						<span class="text-foreground-muted">Expiry date</span>
						<span class="text-foreground">
							{formatDate(subscriptionStore.trialExpiresAt, { year: 'numeric', month: 'long', day: 'numeric' })}
						</span>
					</div>
				{/if}
			</div>
		</div>

		<!-- Wallet balance + recent activity -->
		{#if usage}
			<div class="border border-border rounded-lg p-6 mb-6">
				<div class="flex items-baseline justify-between mb-1">
					<h2 class="settings-label">Balance</h2>
					{#if renewsLabel}
						<span class="text-xs text-foreground-muted">Renews {renewsLabel}</span>
					{/if}
				</div>
				<div class="flex items-baseline gap-2 mb-5">
					<span class="text-3xl font-semibold text-foreground tabular-nums">{formatMicrosUSD(usage.balance_micros)}</span>
					<span class="text-foreground-muted text-sm">available</span>
				</div>

				<div class="flex justify-between text-sm">
					<span class="text-foreground-muted">Spent this month</span>
					<span class="text-foreground tabular-nums">{formatMicrosUSD(usage.month_to_date_micros)}</span>
				</div>

				{#if usage.entries.length > 0}
					<div class="mt-5 border-t border-border-subtle pt-4">
						<div class="text-xs uppercase tracking-wide text-foreground-muted mb-2">Recent activity</div>
						<div class="divide-y divide-border-subtle">
							{#each usage.entries.slice(0, 15) as e}
								<div class="flex justify-between items-center py-2 text-sm">
									<div class="flex items-center gap-2.5 min-w-0">
										<Icon
											icon={kindIcon[e.kind] ?? 'ri:circle-line'}
											width="15"
											class={e.micros < 0 ? 'text-foreground-muted shrink-0' : 'text-success shrink-0'}
										/>
										<span class="text-foreground truncate">{kindLabel[e.kind] ?? e.kind}</span>
									</div>
									<div class="flex items-center gap-3 shrink-0">
										<span class="text-foreground-muted text-xs tabular-nums">{entryDate(e.ts)}</span>
										<span class="tabular-nums {e.micros < 0 ? 'text-foreground' : 'text-success'}">
											{e.micros < 0 ? '−' : '+'}{formatMicrosPrecise(Math.abs(e.micros))}
										</span>
									</div>
								</div>
							{/each}
						</div>
					</div>
				{:else}
					<div class="mt-5 border-t border-border-subtle pt-4 text-sm text-foreground-muted">
						No activity yet — your usage will show up here.
					</div>
				{/if}
			</div>
		{/if}

		<!-- Connect subscription (device-authorization link flow) -->
		{#if !isSubscribed}
			<div class="border border-border rounded-lg p-6 mb-6">
				<h2 class="settings-label">Connect your subscription</h2>
				<p class="text-foreground-muted text-sm mb-4">
					Link this box to your Virtues subscription to enable AI. Checkout happens
					securely on Stripe — your box never sees a payment key.
				</p>

				{#if linkError}
					<p class="text-error text-sm mb-3">{linkError}</p>
				{/if}

				{#if linkDone}
					<p class="text-success text-sm mb-3">Linked! Finishing setup…</p>
				{:else if linkInfo}
					<!-- Hand-off announced: what opens, where, and the pairing code,
					     shown before any tab opens. Continue is an explicit click. -->
					<div class="space-y-3 mb-4 text-sm">
						<p class="text-foreground-muted">
							Continue opens a new tab at
							<span class="font-medium text-foreground">{checkoutHost || 'the Virtues billing page'}</span>
							to complete checkout on Stripe, then sends you back here automatically.
						</p>
						<div class="rounded-md border border-border bg-surface-alt p-3">
							<div class="text-xs text-foreground-muted mb-1">Your pairing code</div>
							<div class="font-mono text-lg font-medium tracking-wider text-foreground">
								{linkInfo.user_code}
							</div>
							<div class="text-xs text-foreground-muted mt-1">
								It should match the code shown on the checkout page. Expires in ~15 min.
							</div>
						</div>
						<button
							onclick={proceedToCheckout}
							class="px-4 py-2 bg-accent text-on-accent rounded-md text-sm font-medium hover:opacity-90 transition-opacity"
						>
							Continue to checkout →
						</button>
						<p class="text-foreground-muted text-xs">
							Prefer to do it by hand? Go to
							<span class="font-mono">{linkInfo.verification_uri}</span> and enter the code above.
						</p>
						{#if linkPolling}
							<p class="text-foreground-muted">Waiting for checkout to complete…</p>
						{/if}
					</div>
				{:else}
					<button
						onclick={connectSubscription}
						disabled={linkLoading}
						class="px-4 py-2 bg-accent text-on-accent rounded-md text-sm font-medium hover:opacity-90 disabled:opacity-50 transition-opacity"
					>
						{linkLoading ? 'Starting…' : 'Connect subscription'}
					</button>
				{/if}
			</div>
		{/if}

		<!-- Auto-top-up + BYO status — local box state, no virtues-api call -->
		{#if !localLoading && local}
			<div class="border border-border rounded-lg p-6 mb-6">
				<h2 class="settings-label">Wallet & top-up</h2>

				<div class="flex items-center justify-between mb-3">
					<div>
						<div class="font-medium text-sm">Auto top-up</div>
						<div class="text-xs text-foreground-muted">
							When the wallet hits $0, charge $10 to your card and continue chatting.
						</div>
					</div>
					<button
						onclick={() => setAutoTopup(!local!.auto_topup.enabled)}
						class="px-3 py-1.5 rounded-md text-xs font-medium {local.auto_topup.enabled ? 'bg-accent text-on-accent' : 'bg-surface-alt border border-border text-foreground'}"
					>
						{local.auto_topup.enabled ? 'On' : 'Off'}
					</button>
				</div>

				{#if !local.auto_topup.enabled && local.auto_topup.disabled_at}
					<div class="rounded bg-error-subtle border border-error/20 p-3 text-xs text-error">
						Auto top-up disabled itself after {local.auto_topup.failures_24h} failed
						charges in 24h. Update your payment method in the Stripe portal below,
						then flip the toggle back on.
					</div>
				{:else if local.auto_topup.failures_24h > 0}
					<div class="rounded bg-warning-subtle border border-warning/20 p-3 text-xs text-warning">
						{local.auto_topup.failures_24h} failed top-up{local.auto_topup.failures_24h === 1 ? '' : 's'}
						in the last 24h. {3 - local.auto_topup.failures_24h} more before the breaker trips.
					</div>
				{/if}

				<div class="mt-4 pt-4 border-t border-border">
					<div class="flex items-center justify-between">
						<div>
							<div class="font-medium text-sm">Bring your own AI</div>
							<div class="text-xs text-foreground-muted">
								{#if local.byo.configured}
									Active: {local.byo.endpoint_url
										? byoHost(local.byo.endpoint_url)
										: (local.byo.provider ?? 'your endpoint')}
									{#if local.byo.default_model}· {local.byo.default_model}{/if}.
									Every AI call leaves by your endpoint.
								{:else}
									Send every AI call to your own endpoint instead of the Virtues
									wallet.
								{/if}
							</div>
						</div>
						<button
							onclick={toggleByo}
							class="px-3 py-1.5 rounded-md text-xs font-medium bg-surface-alt border border-border text-foreground hover:bg-surface"
						>
							{byoOpen ? 'Close' : local.byo.configured ? 'Manage' : 'Set up'}
						</button>
					</div>

					{#if byoOpen}
						<div class="mt-4 pt-4 border-t border-border-subtle">
							<p class="text-xs text-foreground-muted mb-4">
								Point your box at any endpoint that speaks OpenAI-style
								<code class="bg-surface-alt px-1 rounded">/chat/completions</code>
								with a bearer token. The key is stored on your box, encrypted,
								behind a sudo approval at the CLI.
							</p>
							<p class="text-xs text-foreground-muted mb-4">
								<strong>This covers every AI call</strong> — chat, compaction, day
								summaries, image generation and transcription all leave by your
								endpoint. The wallet stays live for the things that aren't AI and
								that your key can't pay for: maps, web search, photos, and bank
								connections.
							</p>
							<p class="text-xs text-warning mb-4">
								<strong>Consider whose key it is.</strong> Routing through an
								employer's account means your personal life passes through
								infrastructure they can read. Bringing your own key gives you
								control of the vendor and the bill; it does not, by itself, give
								you more privacy.
							</p>

							{#if byoLoading}
								<p class="text-sm text-foreground-muted">Loading…</p>
							{:else if byoLoadError}
								<p class="text-error text-sm">{byoLoadError}</p>
							{:else if byoStatus?.configured}
								<div class="rounded-lg border border-border bg-surface p-5 mb-4">
									<div class="flex items-start gap-3">
										<div class="flex-shrink-0 w-10 h-10 rounded-lg bg-success/10 border border-success/30 flex items-center justify-center">
											<Icon icon="ri:key-line" class="text-success" />
										</div>
										<div class="flex-1 min-w-0">
											<div class="font-medium">
												AI is going to your own endpoint
											</div>
											<div class="text-xs text-foreground-muted mt-1 flex flex-wrap gap-x-3 gap-y-1">
												{#if byoStatus.endpoint_url}
													<!-- The host, not a chosen label: it says where traffic
													     actually goes, which a slug never did. -->
													<span
														>Sending to <Badge>{byoHost(byoStatus.endpoint_url)}</Badge
														></span
													>
													<span><code class="text-xs">{byoStatus.endpoint_url}</code></span>
												{/if}
												{#if byoStatus.default_model}
													<span>Default model: <code class="text-xs">{byoStatus.default_model}</code></span>
												{/if}
											</div>
											<p class="text-xs text-foreground-muted mt-2">
												Every AI call goes box → your endpoint, so Virtues is not in
												the path and your wallet isn't charged for it. The wallet
												stays live for maps, web search, photos and bank connections.
											</p>
										</div>
										<Button variant="ghost" onclick={startByoDelete}>
											<Icon icon="ri:close-circle-line" />
											Remove
										</Button>
									</div>
								</div>

								<div class="rounded-lg border border-border bg-surface p-5">
									<div class="font-medium mb-3">Replace the key</div>
									{@render byoKeyForm()}
									<div class="flex justify-end mt-4">
										<Button variant="primary" onclick={startByoSave}>Save new key</Button>
									</div>
								</div>
							{:else}
								<div class="rounded-lg border border-border bg-surface p-5">
									<div class="font-medium mb-1">No BYO key set</div>
									<p class="text-xs text-foreground-muted mb-4">
										Everything routes through the Virtues wallet ($20/mo subscription
										+ usage). Set a key to send chat direct instead.
									</p>
									{@render byoKeyForm()}
									<div class="flex justify-end mt-4">
										<Button variant="primary" onclick={startByoSave}>Save key</Button>
									</div>
								</div>
							{/if}

							<div class="text-xs text-foreground-muted mt-4 space-y-2">
								<p>
									<strong>An AI gateway is usually the best answer.</strong> Vercel
									AI Gateway, OpenRouter, LiteLLM or your work's proxy each reach
									every provider through one key — including models we pick that
									yours may not carry otherwise. Point this at theirs.
								</p>
								<p>
									Provider APIs work directly too: OpenAI, xAI, Groq, DeepSeek,
									Mistral, and Anthropic and Google on their OpenAI-compatible
									endpoints. So does a local Ollama, LM Studio or llama.cpp on
									<code class="bg-surface-alt px-1 rounded">http://localhost</code>.
								</p>
								<p>
									<strong>AWS Bedrock needs a gateway in front.</strong> It signs
									requests rather than taking a bearer token, so a pasted key
									can't reach it — but every gateway above can.
								</p>
							</div>
						</div>
					{/if}
				</div>
			</div>
		{/if}

		<!-- Manage Subscription -->
		<div class="border border-border rounded-lg p-6 mb-6">
			<h2 class="settings-label">Payment</h2>
			<p class="text-foreground-muted text-sm mb-4">
				Manage your payment method, view invoices, and change your plan through Stripe.
			</p>

			{#if portalError}
				<p class="text-error text-sm mb-3">{portalError}</p>
			{/if}

			<button
				onclick={openBillingPortal}
				disabled={portalLoading}
				class="px-4 py-2 bg-accent text-on-accent rounded-md text-sm font-medium hover:opacity-90 disabled:opacity-50 transition-opacity"
			>
				{portalLoading ? 'Opening...' : 'Manage Subscription'}
			</button>
		</div>
</div>

<!--
	Endpoint URL, not a provider picker. There is no provider taxonomy: we
	speak one contract, so the only thing that varies is the address. The
	examples below are copy, deliberately — when a vendor moves a path, stale
	help text is wrong, whereas a stale <option> is a broken shipped feature.
	That is exactly how `anthropic` and `google` came to point at APIs we
	cannot call. See agents/plan/byo-ai-plan.md.
-->
{#snippet byoKeyForm()}
	<div class="space-y-3">
		<div>
			<label class="block text-xs text-foreground-muted mb-1" for="byo-url"
				>Endpoint URL</label
			>
			<Input
				id="byo-url"
				bind:value={byoEndpointUrl}
				placeholder="https://ai-gateway.vercel.sh/v1/chat/completions"
			/>
			<p class="text-xs text-foreground-muted mt-1.5">
				Any endpoint speaking OpenAI-style <code class="bg-surface-alt px-1 rounded"
					>/chat/completions</code
				> with a bearer token.
			</p>
		</div>
		<div>
			<label class="block text-xs text-foreground-muted mb-1" for="byo-key"
				>API key</label
			>
			<Input
				id="byo-key"
				type="password"
				bind:value={byoApiKey}
				placeholder="sk-…"
			/>
		</div>
		<div class="pt-1">
			<div class="text-xs font-medium mb-1">What your endpoint calls each model</div>
			<p class="text-xs text-foreground-muted mb-3">
				Optional. A model id is an address on one gateway, not a portable
				name — the same model we call <code class="bg-surface-alt px-1 rounded"
					>spacexai/grok-4.5</code
				>
				is <code class="bg-surface-alt px-1 rounded">x-ai/grok-4.5</code> on
				OpenRouter. Leave a row blank if your endpoint uses the same ids we do,
				which is true for Vercel AI Gateway. Blank rows that your endpoint
				doesn't carry will fail with its own error naming the model.
			</p>
			<div class="space-y-2">
				{#each SLOT_FIELDS as slot (slot.key)}
					<div class="flex flex-col sm:flex-row sm:items-center gap-1 sm:gap-3">
						<label
							class="text-xs text-foreground-muted sm:w-28 sm:shrink-0 sm:text-right"
							for="byo-model-{slot.key}">{slot.label}</label
						>
						<div class="flex-1">
							<Input
								id="byo-model-{slot.key}"
								bind:value={byoModels[slot.key]}
								placeholder={slot.placeholder}
							/>
						</div>
					</div>
					{#if slot.note}
						<p class="text-xs text-foreground-muted sm:ml-[7.75rem] -mt-1">
							{slot.note}
						</p>
					{/if}
				{/each}
			</div>
		</div>
	</div>
{/snippet}

<SudoModal
	bind:show={showSudoSave}
	action="change_byo_key"
	title="Save BYO AI key"
	description="Sensitive action — chat will route through this key instead of the Virtues wallet. Background AI still bills the wallet. Confirm at the server's CLI."
	onApproved={performByoSave}
/>

<SudoModal
	bind:show={showSudoDelete}
	action="change_byo_key"
	title="Remove BYO AI key"
	description="Sensitive action — chat will switch back to the Virtues wallet. Confirm at the server's CLI."
	onApproved={performByoDelete}
/>
