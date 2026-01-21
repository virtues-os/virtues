<script lang="ts">
	import { page } from "$app/stores";

	const errorCode = $derived($page.url.searchParams.get("error"));

	const errorMessages: Record<string, { title: string; message: string }> = {
		Configuration: {
			title: "Configuration Error",
			message: "There's a problem with the server configuration. Please contact support."
		},
		AccessDenied: {
			title: "Access Denied",
			message: "This email is not authorized to sign in to this instance."
		},
		Verification: {
			title: "Link Expired",
			message: "The magic link has expired or has already been used. Please request a new one."
		},
		Default: {
			title: "Authentication Error",
			message: "An error occurred during authentication. Please try again."
		}
	};

	const error = $derived(errorMessages[errorCode || ""] || errorMessages.Default);
</script>

<div class="w-full max-w-sm text-center">
	<div
		class="w-16 h-16 mx-auto mb-6 rounded-full bg-error-subtle flex items-center justify-center"
	>
		<iconify-icon icon="lucide:alert-circle" class="text-error text-3xl"></iconify-icon>
	</div>

	<h1 class="font-serif text-3xl font-normal text-foreground mb-3">
		{error.title}
	</h1>

	<p class="text-foreground-muted text-sm mb-8">
		{error.message}
	</p>

	<a
		href="/login"
		class="inline-flex items-center justify-center gap-2 px-6 py-3 rounded-lg bg-foreground text-background font-medium hover:bg-foreground/90 focus:outline-none focus:ring-2 focus:ring-accent/50 transition-colors"
	>
		<iconify-icon icon="lucide:arrow-left" class="text-lg"></iconify-icon>
		Back to Sign In
	</a>
</div>
