<script lang="ts">
	import { onMount } from "svelte";
	import { goto } from "$app/navigation";
	import { page } from "$app/stores";
	import * as api from "$lib/api/client";

	let error = $state<string | null>(null);
	let processing = $state(true);

	onMount(async () => {
		const params = $page.url.searchParams;

		// Check if this is a Plaid OAuth redirect
		// Plaid returns oauth_state_id when coming back from bank OAuth
		if (params.has("oauth_state_id")) {
			// Redirect to add source page with OAuth params preserved
			// The PlaidLink component there will detect and handle the OAuth continuation
			const redirectUrl = `/data/sources/add?type=plaid&${params.toString()}`;
			await goto(redirectUrl);
			return;
		}

		try {
			// Extract all OAuth callback parameters
			const callbackParams: any = {
				provider: params.get("provider"),
			};

			// Optional parameters
			if (params.has("code")) callbackParams.code = params.get("code");
			if (params.has("access_token"))
				callbackParams.access_token = params.get("access_token");
			if (params.has("refresh_token"))
				callbackParams.refresh_token = params.get("refresh_token");
			if (params.has("expires_in"))
				callbackParams.expires_in = parseInt(
					params.get("expires_in")!,
				);
			if (params.has("state")) callbackParams.state = params.get("state");
			if (params.has("workspace_id"))
				callbackParams.workspace_id = params.get("workspace_id");
			if (params.has("workspace_name"))
				callbackParams.workspace_name = params.get("workspace_name");
			if (params.has("bot_id"))
				callbackParams.bot_id = params.get("bot_id");

			if (!callbackParams.provider) {
				throw new Error("Missing provider parameter");
			}

			console.log("OAuth callback params:", callbackParams);

			// Send the callback to the backend - response includes return_url extracted from state
			const response = await api.handleOAuthCallback(callbackParams);

			console.log("OAuth callback response:", response);

			// Use return_url from backend response (extracted from signed state token)
			if (response.return_url?.startsWith("/onboarding")) {
				// Return to onboarding flow with connected source info
				await goto(`${response.return_url}?source_id=${response.id}&connected=true`);
			} else {
				// Default: redirect to add page to configure streams
				await goto(`/data/sources/add?source_id=${response.id}`);
			}
		} catch (e) {
			console.error("OAuth callback error:", e);
			error = e instanceof Error ? e.message : "OAuth callback failed";
			processing = false;
		}
	});
</script>

<div class="min-h-screen flex items-center justify-center">
	<div class="max-w-md w-full p-8 text-center">
		{#if processing}
			<div class="mb-4">
				<div
					class="inline-block w-12 h-12 border-4 border-border border-t-foreground rounded-full animate-spin"
				></div>
			</div>
			<h1 class="text-2xl font-serif text-foreground mb-2">
				Completing Authorization
			</h1>
			<p class="text-foreground-muted">
				Please wait while we finish setting up your connection...
			</p>
		{:else if error}
			<div class="mb-6">
				<svg
					class="w-16 h-16 text-foreground-subtle mx-auto"
					fill="none"
					stroke="currentColor"
					viewBox="0 0 24 24"
				>
					<path
						stroke-linecap="round"
						stroke-linejoin="round"
						stroke-width="2"
						d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
					/>
				</svg>
			</div>
			<h1 class="text-2xl font-serif text-foreground mb-2">
				Authorization Failed
			</h1>
			<p class="text-foreground-muted mb-6">{error}</p>
			<a
				href="/data/sources/add"
				class="inline-block px-6 py-3 btn-primary transition-colors"
			>
				Try Again
			</a>
		{/if}
	</div>
</div>
