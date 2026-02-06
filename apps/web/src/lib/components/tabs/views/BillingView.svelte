<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import { Page } from '$lib';
	import { subscriptionStore } from '$lib/stores/subscription.svelte';
	import { spaceStore } from '$lib/stores/space.svelte';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	let portalLoading = $state(false);
	let portalError = $state<string | null>(null);

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

	const statusLabel: Record<string, string> = {
		active: 'Active',
		trialing: 'Trial',
		past_due: 'Past Due',
		expired: 'Expired',
	};

	const statusColor: Record<string, string> = {
		active: 'text-green-500',
		trialing: 'text-blue-500',
		past_due: 'text-yellow-500',
		expired: 'text-red-500',
	};
</script>

<Page>
	<div class="max-w-3xl">
		<div class="mb-8">
			<h1 class="text-3xl font-serif font-medium text-foreground mb-2">Billing</h1>
			<p class="text-foreground-muted">Manage your subscription and payment method</p>
		</div>

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

		<!-- Manage Subscription -->
		<div class="border border-border rounded-lg p-6 mb-6">
			<h2 class="text-lg font-medium text-foreground mb-2">Payment</h2>
			<p class="text-foreground-muted text-sm mb-4">
				Manage your payment method, view invoices, and change your plan through Stripe.
			</p>

			{#if portalError}
				<p class="text-red-500 text-sm mb-3">{portalError}</p>
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
	</div>
</Page>
