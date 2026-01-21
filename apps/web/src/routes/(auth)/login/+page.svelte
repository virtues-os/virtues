<script lang="ts">
	import { page } from "$app/stores";
	import { Button } from "$lib";

	let email = $state("");
	let isLoading = $state(false);
	let emailSent = $state(false);
	let error = $state<string | null>(null);

	// Check for error from auth callback
	const authError = $derived($page.url.searchParams.get("error"));

	async function handleSubmit(e: SubmitEvent) {
		e.preventDefault();
		error = null;
		isLoading = true;

		try {
			// Call Rust auth API
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
</script>

<div class="w-full max-w-sm">
	{#if emailSent}
		<!-- Success state -->
		<div class="text-center">
			<div
				class="w-16 h-16 mx-auto mb-6 rounded-full bg-success-subtle flex items-center justify-center"
			>
				<iconify-icon
					icon="lucide:mail-check"
					class="text-success text-3xl"
				></iconify-icon>
			</div>
			<h1 class="font-serif text-3xl font-normal text-foreground mb-3">
				Check your email
			</h1>
			<p class="text-foreground-muted text-sm mb-6">
				We sent a magic link to <span
					class="font-medium text-foreground">{email}</span
				>. Click the link to sign in.
			</p>
			<Button
				variant="ghost"
				onclick={() => {
					emailSent = false;
					email = "";
				}}
			>
				Use a different email
			</Button>
		</div>
	{:else}
		<!-- Login form -->
		<h1 class="font-serif text-3xl font-normal text-foreground mb-2">
			Welcome back
		</h1>
		<p class="text-foreground-muted text-sm mb-8">
			Enter your email to receive a magic link
		</p>

		{#if authError === "AccessDenied"}
			<div
				class="mb-6 p-4 rounded-lg bg-error-subtle border border-error/20 text-error text-sm"
			>
				Access denied. This email is not authorized to sign in.
			</div>
		{:else if authError}
			<div
				class="mb-6 p-4 rounded-lg bg-error-subtle border border-error/20 text-error text-sm"
			>
				{authError === "Verification"
					? "The magic link has expired. Please request a new one."
					: "An error occurred. Please try again."}
			</div>
		{/if}

		{#if error}
			<div
				class="mb-6 p-4 rounded-lg bg-error-subtle border border-error/20 text-error text-sm"
			>
				{error}
			</div>
		{/if}

		<form onsubmit={handleSubmit} class="space-y-4">
			<div>
				<label for="email" class="sr-only">Email address</label>
				<input
					id="email"
					type="email"
					bind:value={email}
					required
					placeholder="you@example.com"
					disabled={isLoading}
					class="w-full px-4 py-3 rounded-lg bg-surface-alt border border-border text-foreground placeholder:text-foreground-muted focus:outline-none focus:ring-2 focus:ring-accent/50 focus:border-accent transition-colors disabled:opacity-50"
				/>
			</div>

			<Button
				type="submit"
				variant="primary"
				disabled={isLoading || !email}
				class="w-full"
			>
				{#if isLoading}
					<iconify-icon icon="lucide:loader-2" class="animate-spin"
					></iconify-icon>
					Sending...
				{:else}
					Continue with Email
				{/if}
			</Button>
		</form>
	{/if}
</div>
