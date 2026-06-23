<!--
  AccountGate — the one required step: link a Virtues subscription (the wallet).

  A small state machine (choose → subscribe | login → waiting) over the setup
  endpoints. It owns its own poll loop and calls `onLinked` once the box reports
  a billing token, so the shell can refresh derived state. Self-contained so it
  serves both the document flow and the "Set up manually" door.
-->
<script lang="ts">
	import { onDestroy } from "svelte";
	import { Button } from "$lib";
	import Icon from "$lib/components/Icon.svelte";

	interface Props {
		done: boolean;
		onLinked: () => void;
	}

	let { done, onLinked }: Props = $props();

	type AccountMode = "choose" | "subscribe" | "login" | "waiting";
	let accountMode = $state<AccountMode>("choose");
	let checkoutUrl = $state<string | null>(null);
	let email = $state("");
	let accountError = $state<string | null>(null);
	let pollTimer: ReturnType<typeof setInterval> | null = null;

	function stopPolling() {
		if (pollTimer) {
			clearInterval(pollTimer);
			pollTimer = null;
		}
	}
	function startPolling() {
		stopPolling();
		pollTimer = setInterval(async () => {
			try {
				const r = await fetch("/api/setup/link/poll", { method: "POST" });
				const data = await r.json();
				if (data.status === "ready") {
					stopPolling();
					onLinked();
				} else if (data.status === "expired" || data.status === "none") {
					stopPolling();
					accountMode = "choose";
					accountError = "That link expired — start again.";
				}
			} catch {
				/* transient; next tick retries */
			}
		}, 3000);
	}

	async function startSubscribe() {
		accountError = null;
		try {
			const r = await fetch("/api/setup/subscribe/start", { method: "POST" });
			if (!r.ok) throw new Error();
			const data = await r.json();
			checkoutUrl = data.verification_uri_complete || data.verification_uri;
			accountMode = "subscribe";
			startPolling();
		} catch {
			accountError =
				"Couldn't reach the Virtues billing service. Check the box's internet connection and try again.";
		}
	}
	async function startLogin() {
		accountError = null;
		if (!email.includes("@") || !email.includes(".")) {
			accountError = "That doesn't look like an email.";
			return;
		}
		try {
			const r = await fetch("/api/setup/login/start", {
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({ email }),
			});
			if (!r.ok) throw new Error();
			const data = await r.json();
			if (data.status === "sent") {
				accountMode = "waiting";
				startPolling();
			} else if (data.status === "no_account")
				accountError = "No Virtues subscription on that email — create a new account instead.";
			else if (data.status === "rate_limited")
				accountError = "Too many attempts for that email — try again in an hour.";
		} catch {
			accountError =
				"Couldn't reach the Virtues billing service. Check the box's internet connection and try again.";
		}
	}

	onDestroy(stopPolling);
</script>

{#if done}
	<div class="flex items-center gap-3 text-sm text-foreground-muted">
		<span class="flex h-9 w-9 items-center justify-center rounded-full bg-success-subtle text-success">
			<Icon icon="ri:check-line" width="20" />
		</span>
		Your Virtues account is connected.
	</div>
{:else}
	<details class="group mb-5 rounded-xl border border-border-subtle bg-surface-elevated/40 text-sm">
		<summary
			class="flex cursor-pointer list-none items-center justify-between px-4 py-3 text-foreground-muted transition-colors hover:text-foreground"
		>
			What stays on your box, and what we see
			<Icon icon="ri:arrow-down-s-line" width="18" class="transition-transform duration-200 group-open:rotate-180" />
		</summary>
		<div class="space-y-3 px-4 pb-4 text-foreground-muted">
			<div>
				<div class="mb-1 font-medium text-foreground">Stays on your box</div>
				<p>Every message, photo, file, note, and prompt. Your encryption keys. Anything semantic about who you are.</p>
			</div>
			<div>
				<div class="mb-1 font-medium text-foreground">What we see — the strict minimum</div>
				<p>A Stripe customer ID, token counts on AI calls (for billing), and OAuth callbacks for ~200ms. Never content, conversations, or who you talk to.</p>
			</div>
		</div>
	</details>

	{#if accountError}
		<div class="mb-3 rounded-xl border border-error/20 bg-error-subtle p-3 text-sm text-error">{accountError}</div>
	{/if}

	{#if accountMode === "choose"}
		<div class="flex flex-col gap-3">
			<Button type="button" variant="primary" class="w-full justify-center py-2.5" onclick={startSubscribe}>
				Create a Virtues account · $20/mo
			</Button>
			<button
				type="button"
				class="py-2 text-sm text-foreground-muted transition-colors hover:text-foreground"
				onclick={() => {
					accountMode = "login";
					accountError = null;
				}}
			>
				I already have an account
			</button>
		</div>
	{:else if accountMode === "subscribe"}
		<div class="space-y-4">
			<a
				href={checkoutUrl}
				target="_blank"
				rel="noopener"
				class="inline-flex w-full items-center justify-center gap-2 rounded-lg bg-foreground px-4 py-2.5 text-sm font-medium text-surface transition-opacity hover:opacity-90"
			>
				<Icon icon="ri:external-link-line" /> Open checkout
			</a>
			<p class="flex items-center gap-2 text-xs text-foreground-muted">
				<Icon icon="ri:loader-4-line" class="animate-spin" /> Waiting for checkout — this advances on its own.
			</p>
		</div>
	{:else if accountMode === "login"}
		<div class="space-y-3">
			<input
				type="email"
				bind:value={email}
				placeholder="Email on your Virtues subscription"
				class="w-full rounded-lg border border-border bg-surface-elevated/50 px-3.5 py-2.5 text-sm outline-none transition-colors focus:border-foreground-muted"
			/>
			<Button type="button" variant="primary" class="w-full justify-center py-2.5" onclick={startLogin}>
				Email me a sign-in link
			</Button>
			<button
				type="button"
				class="w-full py-1 text-sm text-foreground-muted transition-colors hover:text-foreground"
				onclick={() => {
					accountMode = "choose";
					accountError = null;
				}}
			>
				Back
			</button>
		</div>
	{:else if accountMode === "waiting"}
		<p class="flex items-center gap-2 text-sm text-foreground-muted">
			<Icon icon="ri:mail-send-line" /> Check your email and click the link — this advances on its own.
		</p>
	{/if}
{/if}
