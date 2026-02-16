<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import LoginInput from "$lib/components/LoginInput.svelte";
	import { authMatrixMessage } from "$lib/stores/authMatrix.svelte";
	import { page } from "$app/stores";
	import { Button } from "$lib";
	import { onDestroy } from "svelte";

	let email = $state("");
	let isLoading = $state(false);
	let loginInput: ReturnType<typeof LoginInput>;
	let emailSent = $state(false);
	let error = $state<string | null>(null);

	// Check for error from auth callback
	const authError = $derived($page.url.searchParams.get("error"));

	// Update matrix message when emailSent changes, auto-revert after 3s
	let revertTimeout: ReturnType<typeof setTimeout> | null = null;

	$effect(() => {
		authMatrixMessage.set(emailSent ? "SENT" : undefined);

		if (emailSent) {
			revertTimeout = setTimeout(() => {
				emailSent = false;
			}, 5000);
		}

		return () => {
			if (revertTimeout) clearTimeout(revertTimeout);
		};
	});

	// Clear matrix message on unmount
	onDestroy(() => {
		authMatrixMessage.set(undefined);
	});

	async function submitLogin() {
		console.log('[Auth Debug] Submitting email:', JSON.stringify(email));
		if (!email.trim() || isLoading) return;

		loginInput?.vanish();

		error = null;
		isLoading = true;

		try {
			const response = await fetch("/auth/signin", {
				method: "POST",
				headers: {
					"Content-Type": "application/json",
				},
				body: JSON.stringify({ email }),
			});

			if (response.status === 429) {
				const data = await response.json();
				error = data.error || "Too many attempts. Please try again later.";
			} else if (!response.ok) {
				error = "Unable to send magic link. Please try again.";
			} else {
				emailSent = true;
			}
		} catch (err) {
			error = "An unexpected error occurred. Please try again.";
		} finally {
			isLoading = false;
		}
	}

	function handleFormSubmit(e: SubmitEvent) {
		e.preventDefault();
		submitLogin();
	}
</script>

<div class="w-full">
	{#if authError === "AccessDenied"}
		<div
			class="mb-4 p-3 rounded-lg bg-surface-alt border border-border text-foreground-muted text-sm"
		>
			This is a private instance.
		</div>
	{:else if authError}
		<div
			class="mb-4 p-3 rounded-lg bg-error-subtle border border-error/20 text-error text-sm"
		>
			{authError === "Verification"
				? "The magic link has expired. Please request a new one."
				: "An error occurred. Please try again."}
		</div>
	{/if}

	{#if error}
		<div
			class="mb-4 p-3 rounded-lg bg-error-subtle border border-error/20 text-error text-sm"
		>
			{error}
		</div>
	{/if}

	<form onsubmit={handleFormSubmit} class="space-y-3">
		<LoginInput
			bind:this={loginInput}
			bind:value={email}
			placeholder="you@example.com"
			disabled={isLoading}
			onsubmit={submitLogin}
		/>

		<Button
			type="submit"
			variant="primary"
			disabled={isLoading || !email}
			class="w-full"
		>
			{#if isLoading}
				<Icon icon="ri:loader-4-line" class="animate-spin" />
				Sending...
			{:else}
				Send magic link
			{/if}
		</Button>
	</form>
</div>
