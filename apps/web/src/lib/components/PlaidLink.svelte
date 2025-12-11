<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { Button } from '$lib';
	import * as api from '$lib/api/client';
	import type { ConnectedAccountSummary } from '$lib/api/client';

	interface Props {
		onSuccess: (sourceId: string, institutionName?: string, connectedAccounts?: ConnectedAccountSummary[]) => void;
		onCancel?: () => void;
	}

	let { onSuccess, onCancel }: Props = $props();

	// State
	let isLoading = $state(false);
	let isLinkReady = $state(false);
	let error = $state<string | null>(null);
	let plaidHandler: any = null;

	// Track if we're resuming from OAuth redirect
	let isOAuthRedirect = $state(false);

	async function initializePlaidLink() {
		isLoading = true;
		error = null;

		try {
			// 1. Get link token from backend
			const { link_token } = await api.createPlaidLinkToken();

			// 2. Check if Plaid SDK is loaded
			if (typeof (window as any).Plaid === 'undefined') {
				throw new Error('Plaid SDK not loaded');
			}

			// 3. Check if this is an OAuth redirect
			const currentUrl = new URL(window.location.href);
			const hasOAuthParams = currentUrl.searchParams.has('oauth_state_id');

			// 4. Initialize Plaid Link
			plaidHandler = (window as any).Plaid.create({
				token: link_token,
				// Pass current URL if resuming OAuth, otherwise undefined
				receivedRedirectUri: hasOAuthParams ? window.location.href : undefined,
				onSuccess: async (public_token: string, metadata: any) => {
					isLoading = true;
					try {
						// Exchange token via backend
						const result = await api.exchangePlaidToken({
							public_token,
							institution_id: metadata.institution?.institution_id,
							institution_name: metadata.institution?.name
						});
						onSuccess(result.source_id, result.institution_name, result.connected_accounts);
					} catch (err) {
						error = err instanceof Error ? err.message : 'Failed to connect account';
						isLoading = false;
					}
				},
				onExit: (err: any, metadata: any) => {
					if (err) {
						// User didn't complete flow, but there was an error
						error = err.display_message || err.error_message || 'Connection failed';
					}
					// If no error, user just cancelled - don't show error
					isLoading = false;
				},
				onEvent: (eventName: string, metadata: any) => {
					console.log('Plaid event:', eventName, metadata);
				}
			});

			isLinkReady = true;

			// If OAuth redirect, automatically open Link to continue
			if (hasOAuthParams) {
				isOAuthRedirect = true;
				plaidHandler.open();
			}
		} catch (err) {
			error = err instanceof Error ? err.message : 'Failed to initialize Plaid';
		} finally {
			isLoading = false;
		}
	}

	function openLink() {
		if (plaidHandler) {
			plaidHandler.open();
		}
	}

	function handleCancel() {
		if (plaidHandler) {
			plaidHandler.exit();
		}
		onCancel?.();
	}

	// Load Plaid SDK script
	onMount(() => {
		// Check if already loaded
		if (typeof (window as any).Plaid !== 'undefined') {
			initializePlaidLink();
			return;
		}

		// Load the script
		const script = document.createElement('script');
		script.src = 'https://cdn.plaid.com/link/v2/stable/link-initialize.js';
		script.async = true;
		script.onload = () => initializePlaidLink();
		script.onerror = () => {
			error = 'Failed to load Plaid SDK';
			isLoading = false;
		};
		document.head.appendChild(script);
	});

	onDestroy(() => {
		if (plaidHandler) {
			plaidHandler.exit();
		}
	});
</script>

<div class="space-y-6">
	{#if error}
		<div class="p-4 border border-red-300 bg-red-50">
			<p class="text-sm font-serif text-red-900">{error}</p>
			<button
				onclick={() => {
					error = null;
					initializePlaidLink();
				}}
				class="mt-2 text-sm text-red-700 underline hover:text-red-900"
			>
				Try again
			</button>
		</div>
	{/if}

	{#if isOAuthRedirect && isLoading}
		<div class="text-center py-8">
			<div
				class="animate-spin h-8 w-8 border-2 border-neutral-400 border-t-transparent rounded-full mx-auto mb-4"
			></div>
			<p class="text-neutral-600">Completing bank connection...</p>
		</div>
	{:else}
		<div class="space-y-6">
			<div class="text-center">
				<p class="text-neutral-900 font-serif text-lg mb-2">Connect Your Bank Account</p>
				<p class="text-neutral-600 text-sm">
					Securely connect your bank account using Plaid. Your credentials are never shared with us.
				</p>
			</div>

			<div class="flex flex-col items-center gap-4">
				<Button onclick={openLink} disabled={isLoading || !isLinkReady}>
					{#if isLoading}
						<span class="flex items-center gap-2">
							<span
								class="animate-spin h-4 w-4 border-2 border-white border-t-transparent rounded-full"
							></span>
							Loading...
						</span>
					{:else}
						Connect Bank Account
					{/if}
				</Button>

				{#if onCancel}
					<Button variant="ghost" onclick={handleCancel}>Cancel</Button>
				{/if}
			</div>

			<!-- Security note -->
			<div class="border-t border-neutral-200 pt-6">
				<div class="flex items-start gap-3 text-sm text-neutral-600">
					<svg
						class="w-5 h-5 text-green-600 flex-shrink-0 mt-0.5"
						fill="none"
						stroke="currentColor"
						viewBox="0 0 24 24"
					>
						<path
							stroke-linecap="round"
							stroke-linejoin="round"
							stroke-width="2"
							d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"
						/>
					</svg>
					<div>
						<p class="font-medium text-neutral-900">Secure Connection</p>
						<p>
							Plaid uses bank-level encryption to securely connect your accounts. We never see your
							bank login credentials.
						</p>
					</div>
				</div>
			</div>
		</div>
	{/if}
</div>
