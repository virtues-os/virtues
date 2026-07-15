<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import { Page, Button, Input, Badge, SudoModal } from '$lib';
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

	// ─── BYO AI provider key (inline management, formerly ByoKeyView) ──────
	// The escape hatch from the Virtues wallet. When a key is set here, every
	// chat call routes box → provider directly, bypassing virtues-api entirely.
	// Save and Delete are both sudo-gated (`change_byo_key` is one of the four
	// locked sensitive actions); the SudoModal handles the prompt + CLI
	// approval round-trip.
	type ByoStatus = {
		configured: boolean;
		provider: string | null;
		default_model: string | null;
		endpoint_url: string | null;
		created_at: string | null;
	};

	let byoOpen = $state(false);
	let byoStatus = $state<ByoStatus | null>(null);
	let byoLoading = $state(false);
	let byoLoadError = $state<string | null>(null);

	// Form state
	let byoProvider = $state<'openai' | 'anthropic' | 'xai' | 'google' | 'custom'>('openai');
	let byoApiKey = $state('');
	let byoEndpointUrl = $state('');
	let byoDefaultModel = $state('');

	// Sudo modal coordination — we mint a sudo request, then on approval we
	// fire the actual save/delete with the approved request id.
	let showSudoSave = $state(false);
	let showSudoDelete = $state(false);

	async function loadByo() {
		byoLoading = true;
		byoLoadError = null;
		try {
			byoStatus = await getByoKey<ByoStatus>();
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
		if (!byoApiKey.trim()) {
			toast.error('Paste an API key first');
			return;
		}
		if (byoProvider === 'custom' && !byoEndpointUrl.trim()) {
			toast.error('Custom provider requires an endpoint URL');
			return;
		}
		showSudoSave = true;
	}

	async function performByoSave(sudoRequestId: string) {
		try {
			await setByoKey({
				sudo_request_id: sudoRequestId,
				provider: byoProvider,
				api_key: byoApiKey,
				endpoint_url: byoEndpointUrl || null,
				default_model: byoDefaultModel || null,
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

<Page title="Billing" description="Manage your subscription and payment method" maxWidth="prose">

		<!-- Subscription Status -->
		<div class="border border-border rounded-lg p-6 mb-6">
			<h2 class="text-lg font-medium text-foreground mb-4">Subscription</h2>
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
					<h2 class="text-lg font-medium text-foreground">Balance</h2>
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
				<h2 class="text-lg font-medium text-foreground mb-2">Connect your subscription</h2>
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
				<h2 class="text-lg font-medium text-foreground mb-4">Wallet & top-up</h2>

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
							<div class="font-medium text-sm">BYO AI provider key</div>
							<div class="text-xs text-foreground-muted">
								{#if local.byo.configured}
									Active: {local.byo.provider ?? '?'}
									{#if local.byo.default_model}· {local.byo.default_model}{/if}.
									Virtues is out of your AI path.
								{:else}
									Skip the Virtues wallet and use your own provider key directly.
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
								Bring your own OpenAI / Anthropic / xAI / Google / custom key. When
								set, every chat call goes from your box directly to the provider —
								the Virtues cloud is out of the path entirely. The Virtues
								subscription still works in the background; you can switch back
								anytime by deleting the key here.
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
											<div class="font-medium">BYO key active</div>
											<div class="text-xs text-foreground-muted mt-1 flex flex-wrap gap-x-3 gap-y-1">
												<span>Provider: <Badge>{byoStatus.provider ?? '?'}</Badge></span>
												{#if byoStatus.default_model}
													<span>Model: <code class="text-xs">{byoStatus.default_model}</code></span>
												{/if}
												{#if byoStatus.endpoint_url}
													<span>Endpoint: <code class="text-xs">{byoStatus.endpoint_url}</code></span>
												{/if}
											</div>
											<p class="text-xs text-foreground-muted mt-2">
												Virtues is not in your inference path. Wallet & top-up are
												inactive while this key is set.
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
										Currently routing through the Virtues wallet ($20/mo subscription
										+ usage). Paste a provider key below to swap.
									</p>
									{@render byoKeyForm()}
									<div class="flex justify-end mt-4">
										<Button variant="primary" onclick={startByoSave}>Save key</Button>
									</div>
								</div>
							{/if}

							<div class="text-xs text-foreground-muted mt-4">
								<strong>v1 supports OpenAI-compatible APIs.</strong> For Anthropic-native
								or Google-native shapes, point BYO at a translation proxy like
								LiteLLM or OpenRouter and choose
								<code class="bg-surface-alt px-1 rounded">custom</code> with their
								endpoint URL.
							</div>
						</div>
					{/if}
				</div>
			</div>
		{/if}

		<!-- Manage Subscription -->
		<div class="border border-border rounded-lg p-6 mb-6">
			<h2 class="text-lg font-medium text-foreground mb-2">Payment</h2>
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
</Page>

{#snippet byoKeyForm()}
	<div class="space-y-3">
		<div>
			<label class="block text-xs text-foreground-muted mb-1" for="byo-provider"
				>Provider</label
			>
			<select
				id="byo-provider"
				bind:value={byoProvider}
				class="w-full rounded border border-border bg-surface px-3 py-2 text-sm"
			>
				<option value="openai">OpenAI</option>
				<option value="anthropic">Anthropic (via translation proxy)</option>
				<option value="xai">xAI</option>
				<option value="google">Google (via translation proxy)</option>
				<option value="custom">Custom (OpenAI-compatible)</option>
			</select>
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
		{#if byoProvider === 'custom'}
			<div>
				<label class="block text-xs text-foreground-muted mb-1" for="byo-url"
					>Endpoint URL</label
				>
				<Input
					id="byo-url"
					bind:value={byoEndpointUrl}
					placeholder="https://api.example.com/v1/chat/completions"
				/>
			</div>
		{/if}
		<div>
			<label
				class="block text-xs text-foreground-muted mb-1"
				for="byo-model">Default model (optional)</label
			>
			<Input
				id="byo-model"
				bind:value={byoDefaultModel}
				placeholder="gpt-4o, claude-3-5-sonnet-latest, …"
			/>
		</div>
	</div>
{/snippet}

<SudoModal
	bind:show={showSudoSave}
	action="change_byo_key"
	title="Save BYO AI key"
	description="Sensitive action — every future chat call will route through this key. Confirm at the box CLI."
	onApproved={performByoSave}
/>

<SudoModal
	bind:show={showSudoDelete}
	action="change_byo_key"
	title="Remove BYO AI key"
	description="Sensitive action — chat will switch back to the Virtues wallet. Confirm at the box CLI."
	onApproved={performByoDelete}
/>
