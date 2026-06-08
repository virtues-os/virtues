<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import { Page } from '$lib';
	import { subscriptionStore } from '$lib/stores/subscription.svelte';
	import { spaceStore } from '$lib/stores/space.svelte';

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
				const res = await fetch('/api/billing/link/status');
				const data = await res.json();
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

	async function connectSubscription() {
		linkLoading = true;
		linkError = null;
		linkDone = false;
		try {
			const res = await fetch('/api/billing/link/start', { method: 'POST' });
			const data = await res.json();
			if (data.error) {
				linkError = typeof data.error === 'string' ? data.error : 'Failed to start subscription link';
				return;
			}
			linkInfo = data;
			window.open(data.verification_uri_complete, '_blank');
			startPolling(Math.max((data.interval || 5) * 1000, 2000));
		} catch {
			linkError = 'Failed to connect to billing service';
		} finally {
			linkLoading = false;
		}
	}

	$effect(() => () => stopPolling());

	async function openBillingPortal() {
		portalLoading = true;
		portalError = null;
		try {
			const res = await fetch('/api/billing/portal', { method: 'POST' });
			const data = await res.json();
			if (data.url) {
				window.open(data.url, '_blank');
			} else if (data.error) {
				portalError = typeof data.error === 'string' ? data.error : data.error.message || 'Failed to open billing portal';
			}
		} catch (e) {
			portalError = 'Failed to connect to billing service';
		} finally {
			portalLoading = false;
		}
	}

	function openUsage() {
		spaceStore.openTabFromRoute('/virtues/usage', { label: 'Usage', preferEmptyPane: true });
	}

	function openByoKey() {
		spaceStore.openTabFromRoute('/virtues/byo-key', { label: 'AI Provider Key', preferEmptyPane: true });
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
			const r = await fetch('/api/billing/state');
			if (r.ok) local = await r.json();
		} catch { /* swallow */ }
		localLoading = false;
	}

	async function setAutoTopup(enabled: boolean) {
		try {
			const r = await fetch('/api/billing/auto-topup', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ enabled })
			});
			if (r.ok) await loadLocal();
		} catch { /* swallow */ }
	}

	$effect(() => { void loadLocal(); });

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
							{new Date(subscriptionStore.trialExpiresAt).toLocaleDateString(undefined, { year: 'numeric', month: 'long', day: 'numeric' })}
						</span>
					</div>
				{/if}
			</div>
		</div>

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
					<div class="space-y-2 mb-4 text-sm">
						<p class="text-foreground-muted">
							A checkout tab opened. If it didn't,
							<a
								href={linkInfo.verification_uri_complete}
								target="_blank"
								rel="noopener"
								class="text-accent hover:underline">open it here</a
							>.
						</p>
						<p class="text-foreground-muted">
							Or go to <span class="font-mono">{linkInfo.verification_uri}</span> and enter code
							<span class="font-mono font-medium text-foreground">{linkInfo.user_code}</span>.
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
							onclick={openByoKey}
							class="px-3 py-1.5 rounded-md text-xs font-medium bg-surface-alt border border-border text-foreground hover:bg-surface"
						>
							{local.byo.configured ? 'Manage' : 'Set up'}
						</button>
					</div>
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

		<!-- Quick Links -->
		<div class="border border-border rounded-lg p-6">
			<h2 class="text-lg font-medium text-foreground mb-4">Related</h2>
			<div class="space-y-2">
				<button
					onclick={openUsage}
					class="text-sm text-accent hover:underline"
				>
					View usage limits and quotas
				</button>
			</div>
		</div>
</Page>
