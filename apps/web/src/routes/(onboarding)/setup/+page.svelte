<!--
  /setup — the setup wizard (docs/onboarding.md).

  Where a freshly-paired browser lands when the box isn't set up yet. Renders
  the REQUIRED core only — account → name — then hands off to /onboarding
  (the "next wins" screen) and the dashboard. Progress is read from the
  derived state machine (GET /api/setup/state), so this page is a pure
  renderer: refresh, switch devices, or abandon mid-way and it resumes
  exactly where the box actually is. No wizard-session state anywhere.

  The account step drives the box-side device-link (Stripe checkout or
  magic-link email) via POST /api/setup/{subscribe,login}/start and polls
  POST /api/setup/link/poll — poll-only by design: no Stripe redirect-back
  to a LAN address, the wizard tab just notices the link flipped to ready.
-->
<script lang="ts">
	import { goto } from "$app/navigation";
	import { Button } from "$lib";
	import Icon from "$lib/components/Icon.svelte";
	import { onMount, onDestroy } from "svelte";

	type Step = { id: string; title: string; done: boolean; detail?: string };
	type SetupState = { setup: Step[]; setup_complete: boolean; onboarding: Step[] };

	let state_ = $state<SetupState | null>(null);
	let loading = $state(true);

	// ── account step ──
	type AccountMode = "choose" | "subscribe" | "login" | "waiting" | "done";
	let accountMode = $state<AccountMode>("choose");
	let checkoutUrl = $state<string | null>(null);
	let email = $state("");
	let accountError = $state<string | null>(null);
	let pollTimer: ReturnType<typeof setInterval> | null = null;

	// ── name step ──
	let boxName = $state("");
	let nameError = $state<string | null>(null);
	let savingName = $state(false);
	let newMdns = $state<string | null>(null);

	function stepDone(id: string): boolean {
		return state_?.setup.find((s) => s.id === id)?.done ?? false;
	}

	async function refreshState() {
		try {
			const r = await fetch("/api/setup/state");
			if (r.ok) {
				state_ = await r.json();
				if (stepDone("account") && (accountMode === "waiting" || accountMode === "choose")) {
					accountMode = "done";
					stopPolling();
				}
			}
		} catch {
			/* box briefly unreachable — keep last state */
		} finally {
			loading = false;
		}
	}

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
					accountMode = "done";
					await refreshState();
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
			accountError = "Couldn't reach the Virtues billing service. Check the box's internet connection and try again.";
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
			} else if (data.status === "no_account") {
				accountError = "No Virtues subscription on that email — create a new account instead.";
			} else if (data.status === "rate_limited") {
				accountError = "Too many attempts for that email — try again in an hour.";
			}
		} catch {
			accountError = "Couldn't reach the Virtues billing service. Check the box's internet connection and try again.";
		}
	}

	async function saveName() {
		nameError = null;
		savingName = true;
		try {
			const r = await fetch("/api/setup/name", {
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({ name: boxName }),
			});
			const data = await r.json();
			if (r.ok) {
				newMdns = data.mdns;
				await refreshState();
			} else {
				nameError =
					data.detail ??
					(data.error === "invalid_name"
						? "Only lowercase letters, digits, and hyphens."
						: "Couldn't rename the box.");
			}
		} catch {
			nameError = "Couldn't reach the box.";
		} finally {
			savingName = false;
		}
	}

	async function finish() {
		await goto("/onboarding", { replaceState: true });
	}

	onMount(() => {
		void refreshState();
		// Light background refresh so steps completed elsewhere (another
		// device, the CLI) tick over here too — the panel-mirror behavior.
		const t = setInterval(refreshState, 5000);
		return () => clearInterval(t);
	});
	onDestroy(stopPolling);
</script>

<div class="min-h-screen flex items-center justify-center px-6 py-12">
	<div class="w-full max-w-md">
		<!-- Progress rail -->
		{#if state_}
			<div class="mb-10 flex items-center justify-center gap-2">
				{#each state_.setup as step (step.id)}
					<div
						class="flex items-center gap-1.5 text-xs {step.done
							? 'text-foreground'
							: 'text-foreground-muted'}"
						title={step.title}
					>
						<Icon
							icon={step.done ? "ri:checkbox-circle-fill" : "ri:checkbox-blank-circle-line"}
							class={step.done ? "text-success" : ""}
						/>
						<span class="hidden sm:inline">{step.title}</span>
					</div>
				{/each}
			</div>
		{/if}

		{#if loading}
			<div class="flex items-center justify-center gap-2 text-foreground-muted text-sm">
				<Icon icon="ri:loader-4-line" class="animate-spin" />
				<span>Checking your box…</span>
			</div>
		{:else if !state_}
			<div class="p-3 rounded-lg bg-error-subtle border border-error/20 text-error text-sm">
				Couldn't reach the box. Make sure you're on the same network, then refresh.
			</div>
		{:else if state_.setup_complete}
			<!-- ── All done ── -->
			<div class="text-center space-y-6">
				<div class="inline-flex items-center justify-center w-16 h-16 rounded-2xl bg-surface-alt border border-border">
					<Icon icon="ri:checkbox-circle-fill" class="text-3xl text-success" />
				</div>
				<div>
					<h1 class="text-2xl font-semibold tracking-tight mb-2">Your box is set up</h1>
					<p class="text-foreground-muted text-sm">
						{#if newMdns}
							From now on, find it at <span class="text-foreground font-medium">http://{newMdns}:8000</span>
						{:else}
							Everything required is done — the rest happens in the app.
						{/if}
					</p>
				</div>
				<Button type="button" variant="primary" class="w-full" onclick={finish}>
					Continue
				</Button>
			</div>
		{:else if !stepDone("account")}
			<!-- ── Step: account ──
			  Trust-pitch rules (moved here from the old `virtues init` TTY
			  intro — they're deliberate, not stylistic): first-person claims
			  only ("what stays on your box" / "what we see"); NO named
			  competitor comparisons (Lanham / trade-libel exposure); the
			  virtues-api sunset commitment is the closer; and every claim
			  must remain true in lockstep with shipped features.
			-->
			<div class="space-y-6">
				<div class="text-center">
					<h1 class="text-2xl font-semibold tracking-tight mb-2">Connect your account</h1>
					<p class="text-foreground-muted text-sm">
						Your data lives on this box — never our cloud. The account covers the two
						things that still need a server: OAuth callbacks and the AI wallet.
					</p>
				</div>

				<details class="rounded-lg bg-surface-alt border border-border text-sm">
					<summary class="px-4 py-3 cursor-pointer text-foreground-muted hover:text-foreground transition-colors">
						What stays on your box, and what we see
					</summary>
					<div class="px-4 pb-4 space-y-3 text-foreground-muted">
						<div>
							<div class="text-foreground font-medium mb-1">Stays on your box</div>
							<p>Every message, photo, file, note, and prompt. Your encryption keys. Anything semantic about who you are.</p>
						</div>
						<div>
							<div class="text-foreground font-medium mb-1">What we see — the strict minimum</div>
							<p>A Stripe customer ID (Stripe holds your card and email), token counts on AI calls (for billing), and OAuth callbacks for ~200ms so providers will talk to your box at all. Never content, conversations, or who you talk to.</p>
						</div>
						<p class="text-xs">
							Our north star is making this server layer extinct — every release that
							removes us from your data path ships harder than features.
						</p>
					</div>
				</details>

				{#if accountError}
					<div class="p-3 rounded-lg bg-error-subtle border border-error/20 text-error text-sm">
						{accountError}
					</div>
				{/if}

				{#if accountMode === "choose"}
					<div class="flex flex-col gap-3">
						<Button type="button" variant="primary" class="w-full" onclick={startSubscribe}>
							Create a Virtues account · $20/mo
						</Button>
						<button
							type="button"
							class="text-sm text-foreground-muted hover:text-foreground transition-colors py-2"
							onclick={() => { accountMode = "login"; accountError = null; }}
						>
							I already have an account
						</button>
					</div>
				{:else if accountMode === "subscribe"}
					<div class="space-y-4 text-center">
						<a
							href={checkoutUrl}
							target="_blank"
							rel="noopener"
							class="inline-flex items-center gap-2 justify-center w-full px-4 py-2.5 rounded-lg bg-foreground text-surface font-medium text-sm"
						>
							<Icon icon="ri:external-link-line" />
							Open checkout
						</a>
						<p class="text-foreground-muted text-xs flex items-center justify-center gap-2">
							<Icon icon="ri:loader-4-line" class="animate-spin" />
							Waiting for checkout to complete — this page advances on its own.
						</p>
					</div>
				{:else if accountMode === "login"}
					<div class="space-y-3">
						<input
							type="email"
							bind:value={email}
							placeholder="Email on your Virtues subscription"
							class="w-full px-3 py-2.5 rounded-lg bg-surface-alt border border-border text-sm outline-none focus:border-foreground-muted"
						/>
						<Button type="button" variant="primary" class="w-full" onclick={startLogin}>
							Email me a sign-in link
						</Button>
						<button
							type="button"
							class="w-full text-sm text-foreground-muted hover:text-foreground transition-colors py-1"
							onclick={() => { accountMode = "choose"; accountError = null; }}
						>
							Back
						</button>
					</div>
				{:else if accountMode === "waiting"}
					<p class="text-foreground-muted text-sm text-center flex items-center justify-center gap-2">
						<Icon icon="ri:mail-send-line" />
						Check your email and click the link — this page advances on its own.
					</p>
				{/if}
			</div>
		{:else if !stepDone("named")}
			<!-- ── Step: name your box ── -->
			<div class="space-y-6">
				<div class="text-center">
					<h1 class="text-2xl font-semibold tracking-tight mb-2">Name your box</h1>
					<p class="text-foreground-muted text-sm">
						This becomes its address on your network — e.g.
						<span class="text-foreground">adam-jace</span> →
						<span class="text-foreground">http://adam-jace.local:8000</span>
					</p>
				</div>

				{#if nameError}
					<div class="p-3 rounded-lg bg-error-subtle border border-error/20 text-error text-sm">
						{nameError}
					</div>
				{/if}

				<div class="space-y-3">
					<input
						type="text"
						bind:value={boxName}
						placeholder="adam-jace"
						autocapitalize="off"
						autocorrect="off"
						spellcheck="false"
						class="w-full px-3 py-2.5 rounded-lg bg-surface-alt border border-border text-sm outline-none focus:border-foreground-muted font-mono"
					/>
					<Button
						type="button"
						variant="primary"
						class="w-full"
						disabled={savingName || boxName.trim().length < 2}
						onclick={saveName}
					>
						{#if savingName}
							<Icon icon="ri:loader-4-line" class="animate-spin" />
							Renaming…
						{:else}
							Set name
						{/if}
					</Button>
					<button
						type="button"
						class="w-full text-sm text-foreground-muted hover:text-foreground transition-colors py-1"
						onclick={finish}
					>
						Skip — keep "virtues"
					</button>
				</div>
			</div>
		{:else}
			<!-- Steps the wizard can't drive (network) — show honestly, never block -->
			<div class="text-center space-y-6">
				<h1 class="text-2xl font-semibold tracking-tight">Almost there</h1>
				{#each state_.setup.filter((s) => !s.done) as step (step.id)}
					<div class="p-3 rounded-lg bg-error-subtle border border-error/20 text-error text-sm text-left">
						{step.detail ?? step.title}
					</div>
				{/each}
				<Button type="button" variant="primary" class="w-full" onclick={finish}>
					Continue anyway
				</Button>
			</div>
		{/if}
	</div>
</div>
