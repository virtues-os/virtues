<script lang="ts">
	/**
	 * OAuthConnectModal - Initiates OAuth flow for cloud services (Google, Notion, etc.)
	 * Shows a brief loading state before redirecting to the OAuth provider.
	 */
	import { onMount } from "svelte";
	import Modal from "$lib/components/Modal.svelte";
	import Icon from "$lib/components/Icon.svelte";
	import * as api from "$lib/api/client";

	interface Props {
		provider: string;
		displayName: string;
		open: boolean;
		onClose: () => void;
	}

	let { provider, displayName, open, onClose }: Props = $props();

	let error = $state<string | null>(null);
	let isRedirecting = $state(false);

	onMount(() => {
		if (open) {
			initiateOAuth();
		}
	});

	async function initiateOAuth() {
		isRedirecting = true;
		error = null;

		try {
			// Return to sources page after OAuth completes
			const returnUrl = `${window.location.origin}/source`;
			const response = await api.initiateOAuth(provider, returnUrl);
			
			// Redirect to OAuth provider
			window.location.href = response.authorization_url;
		} catch (e) {
			error = e instanceof Error ? e.message : "Failed to connect";
			isRedirecting = false;
		}
	}
</script>

<Modal {open} {onClose} title="Connect {displayName}" width="sm">
	<div class="text-center py-6">
		{#if error}
			<div class="mb-6">
				<Icon icon="ri:error-warning-line" class="text-4xl text-error mb-3" />
				<p class="text-sm text-error mb-4">{error}</p>
				<button
					class="text-sm text-primary hover:underline"
					onclick={initiateOAuth}
				>
					Try again
				</button>
			</div>
		{:else if isRedirecting}
			<div class="space-y-4">
				<div class="animate-spin h-8 w-8 border-2 border-primary border-t-transparent rounded-full mx-auto"></div>
				<p class="text-foreground-muted">
					Redirecting to {displayName}...
				</p>
				<p class="text-xs text-foreground-subtle">
					You'll be asked to authorize access to your account.
				</p>
			</div>
		{/if}
	</div>
</Modal>
