<!--
	Connect AI's controls, as one row: create an account (Stripe, honestly
	priced), sign in to one (an email, inline), or an endpoint of your own
	(Billing holds that form; keys are entered there, never here). The same
	setup endpoints AccountGate uses; polls until the box holds a key. Not
	skippable from here — the door is the skip.
-->
<script lang="ts">
	import { onDestroy } from "svelte";
	import { openExternal } from "$lib/tauri/bridge";
	import { setupLinkPoll, setupSubscribeStart, setupLoginStart } from "$lib/api/client";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";

	type Mode = "choose" | "login" | "waiting" | "subscribe";
	let mode = $state<Mode>("choose");
	let email = $state("");
	let error = $state<string | null>(null);
	let checkoutUrl = $state<string | null>(null);
	let timer: ReturnType<typeof setInterval> | null = null;

	function stop() {
		if (timer) clearInterval(timer);
		timer = null;
	}
	function poll() {
		stop();
		timer = setInterval(async () => {
			try {
				const d = await setupLinkPoll<{ status: string }>();
				if (d.status === "ready") {
					stop();
					await gettingStarted.refresh();
				} else if (d.status === "expired" || d.status === "none") {
					stop();
					mode = "choose";
					error = "That link expired. Start again.";
				}
			} catch {
				/* next tick */
			}
		}, 3000);
	}
	async function subscribe() {
		error = null;
		try {
			const d = await setupSubscribeStart<{ verification_uri_complete?: string; verification_uri?: string }>();
			checkoutUrl = d.verification_uri_complete || d.verification_uri || null;
			mode = "subscribe";
			if (checkoutUrl) void openExternal(checkoutUrl);
			poll();
		} catch {
			error = "Couldn't reach the Virtues billing service. Check your server's internet connection and try again.";
		}
	}
	async function login() {
		error = null;
		if (!email.includes("@") || !email.includes(".")) {
			error = "That doesn't look like an email.";
			return;
		}
		try {
			const d = await setupLoginStart<{ status: string }>(email);
			if (d.status === "sent") {
				mode = "waiting";
				poll();
			} else if (d.status === "no_account") error = "No Virtues account on that email yet. Create one instead.";
			else if (d.status === "rate_limited") error = "Too many attempts for that email. Try again in an hour.";
		} catch {
			error = "Couldn't reach the Virtues billing service. Check your server's internet connection and try again.";
		}
	}
	onDestroy(stop);
</script>

{#if mode === "choose"}
	<button type="button" class="btn" onclick={subscribe}>Create a Virtues account · $20/mo</button>
	<button type="button" class="btn quiet" onclick={() => (mode = "login")}>Sign in to my account</button>
	<button
		type="button"
		class="btn quiet"
		onclick={() => windowShellStore.openRouteBeside("/virtues/billing", "Billing")}
	>
		Use an endpoint of my own
	</button>
{:else if mode === "login"}
	<input
		class="email"
		type="email"
		placeholder="you@example.com"
		bind:value={email}
		onkeydown={(e) => e.key === "Enter" && login()}
	/>
	<button type="button" class="btn" onclick={login}>Send the sign-in link</button>
	<button type="button" class="link" onclick={() => (mode = "choose")}>Back</button>
{:else if mode === "waiting"}
	<span class="note">Check your email for the link. This picks up on its own once you have signed in.</span>
{:else if mode === "subscribe"}
	<span class="note">
		Finish in the browser window that opened{checkoutUrl ? "" : " (no window? try again)"}. This picks up on its own after checkout.
	</span>
	<button type="button" class="link" onclick={() => (mode = "choose")}>Back</button>
{/if}
{#if error}<span class="error">{error}</span>{/if}

<style>
	.btn {
		font: inherit;
		font-size: 0.875rem;
		padding: 0.4rem 0.9rem;
		border-radius: 6px;
		border: 1px solid var(--color-foreground);
		background: var(--color-foreground);
		color: var(--color-background);
		cursor: pointer;
	}
	.btn.quiet {
		background: transparent;
		color: var(--color-foreground);
		border-color: var(--color-border);
	}
	.btn.quiet:hover {
		border-color: var(--color-foreground);
	}
	.link {
		background: none;
		border: 0;
		padding: 0.25rem 0;
		font: inherit;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
		text-decoration: underline;
		cursor: pointer;
	}
	.link:hover {
		color: var(--color-foreground);
	}
	.email {
		font: inherit;
		font-size: 0.875rem;
		padding: 0.4rem 0.6rem;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: var(--color-background);
		color: var(--color-foreground);
		min-width: 16rem;
	}
	.note,
	.error {
		font-size: 0.875rem;
		color: var(--color-foreground-muted);
		flex-basis: 100%;
	}
	.error {
		color: var(--color-error, #9a2b2e);
	}
</style>
