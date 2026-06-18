<!--
  /setup — THE single onboarding flow. One page, one progress rail, every step:

    1. Account   — sign in / link your Virtues subscription (atlas + virtues-api)   [required]
    2. This Mac  — collector + Full Disk Access + Accessibility                     [skippable]
    3. Phone     — pair your iPhone                                                 [skippable]
    4. Sources   — connect any catalog source (ConnectionsPanel, same as /sources) [skippable]
    5. Import    — one-time chat-history import                                     [skippable]

  No second wizard, no dashboard hand-off: everything lives here, behind one rail.
  Required steps (1) can't be skipped — they gate `setup_complete` and the app.
  (The box keeps its default `virtues.local` name — there's no rename step: the
  name is cosmetic and reachability is WireGuard/SPKI + localhost, not mDNS.)
  Skippable steps show a small corner "Skip" with an "I know what I'm doing"
  confirm, so people don't bail by accident. The sidebar "Finish setup" entry
  re-opens this page at the first unfinished step.

  All step state is read from the derived /api/setup/state (setup[] + onboarding[]),
  so the flow survives refreshes and the OAuth round-trip.
-->
<script lang="ts">
	import { goto } from "$app/navigation";
	import { Button } from "$lib";
	import Icon from "$lib/components/Icon.svelte";
	import Modal from "$lib/components/Modal.svelte";
	import Stepper from "$lib/components/Stepper.svelte";
	import CollectorPermissionCard from "$lib/components/onboarding/CollectorPermissionCard.svelte";
	import ChatImportCard from "$lib/components/onboarding/ChatImportCard.svelte";
	import DevicePairModal from "$lib/components/sources/DevicePairModal.svelte";
	import ConnectionsPanel from "$lib/components/actions/ConnectionsPanel.svelte";
	import { onMount, onDestroy } from "svelte";

	type Step = { id: string; title: string; done: boolean; detail?: string };
	type SetupState = { setup: Step[]; setup_complete: boolean; onboarding: Step[] };

	let state_ = $state<SetupState | null>(null);
	let loading = $state(true);

	// ── the one rail ──
	type StepId = "account" | "device" | "phone" | "sources" | "import";
	const STEPS: { id: StepId; short: string; title: string; subtitle: string; required: boolean }[] = [
		{ id: "account", short: "Account", required: true,
		  title: "Sign in to Virtues",
		  subtitle: "Link your subscription. It covers the only two things that still need a server — OAuth callbacks and the AI wallet. Your data never leaves the box." },
		{ id: "device", short: "This Mac", required: false,
		  title: "Set up this Mac",
		  subtitle: "Let your box remember what happens on this machine. It all stays on your box." },
		{ id: "phone", short: "Phone", required: false,
		  title: "Add your iPhone",
		  subtitle: "Your richest source — where you go, who you message, your health." },
		{ id: "sources", short: "Sources", required: false,
		  title: "Connect calendar & email",
		  subtitle: "Living sources — they stay current on their own. Read-only." },
		{ id: "import", short: "Import", required: false,
		  title: "Bring your chat history",
		  subtitle: "A one-time import of your past Claude, ChatGPT, or Gemini conversations." },
	];

	let current = $state(0);
	const step = $derived(STEPS[current]);
	const isLast = $derived(current === STEPS.length - 1);

	// Optimistic local flag (flips the phone step the instant the modal succeeds,
	// before the next poll confirms it).
	let phonePaired = $state(false);

	function setupDone(id: string): boolean {
		return state_?.setup.find((s) => s.id === id)?.done ?? false;
	}
	function onboardingDone(id: string): boolean {
		return state_?.onboarding.find((s) => s.id === id)?.done ?? false;
	}
	function stepDone(id: StepId): boolean {
		switch (id) {
			case "account": return setupDone("account");
			case "device": return onboardingDone("device_collecting");
			case "phone": return phonePaired || onboardingDone("first_phone");
			case "sources": return onboardingDone("first_source") || onboardingDone("living_source");
			case "import": return onboardingDone("chat_imported");
		}
	}

	const railSteps = $derived(STEPS.map((s) => ({ id: s.id, label: s.short, done: stepDone(s.id) })));

	// ── account step machine ──
	type AccountMode = "choose" | "subscribe" | "login" | "waiting";
	let accountMode = $state<AccountMode>("choose");
	let checkoutUrl = $state<string | null>(null);
	let email = $state("");
	let accountError = $state<string | null>(null);
	let pollTimer: ReturnType<typeof setInterval> | null = null;

	// ── skip confirmation ──
	let skipModalOpen = $state(false);
	let pairModalOpen = $state(false);

	async function refreshState() {
		try {
			const r = await fetch("/api/setup/state");
			if (r.ok) {
				state_ = await r.json();
				if (stepDone("account") && (accountMode === "waiting" || accountMode === "choose")) {
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
		if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
	}
	function startPolling() {
		stopPolling();
		pollTimer = setInterval(async () => {
			try {
				const r = await fetch("/api/setup/link/poll", { method: "POST" });
				const data = await r.json();
				if (data.status === "ready") {
					stopPolling();
					await refreshState();
				} else if (data.status === "expired" || data.status === "none") {
					stopPolling();
					accountMode = "choose";
					accountError = "That link expired — start again.";
				}
			} catch { /* transient; next tick retries */ }
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
			if (data.status === "sent") { accountMode = "waiting"; startPolling(); }
			else if (data.status === "no_account") accountError = "No Virtues subscription on that email — create a new account instead.";
			else if (data.status === "rate_limited") accountError = "Too many attempts for that email — try again in an hour.";
		} catch {
			accountError = "Couldn't reach the Virtues billing service. Check the box's internet connection and try again.";
		}
	}

	// ── navigation ──
	let landed = false;
	onMount(() => {
		void refreshState().then(() => {
			if (landed) return;
			landed = true;
			const firstUndone = STEPS.findIndex((s) => !stepDone(s.id));
			current = firstUndone === -1 ? STEPS.length - 1 : firstUndone;
		});
		// Light poll so steps completed elsewhere (CLI, OAuth round-trip, the
		// collector daemon) tick over here too.
		const t = setInterval(refreshState, 4000);
		return () => clearInterval(t);
	});
	onDestroy(stopPolling);

	function next() {
		if (isLast) { void goto("/"); return; }
		current += 1;
	}
	function back() { if (current > 0) current -= 1; }
	function requestSkip() { skipModalOpen = true; }
	function confirmSkip() { skipModalOpen = false; next(); }
</script>

<div class="min-h-screen flex items-center justify-center px-6 py-12">
	<div class="w-full max-w-md">
		{#if loading}
			<div class="flex items-center justify-center gap-2 text-foreground-muted text-sm">
				<Icon icon="ri:loader-4-line" class="animate-spin" />
				<span>Checking your box…</span>
			</div>
		{:else if !state_}
			<div class="p-3 rounded-lg bg-error-subtle border border-error/20 text-error text-sm">
				Couldn't reach the box. Make sure you're on the same network, then refresh.
			</div>
		{:else}
			<!-- One rail, every step -->
			<div class="mb-10">
				<Stepper steps={railSteps} {current} />
			</div>

			<div class="space-y-2 mb-5 text-center">
				<h1 class="text-2xl font-semibold tracking-tight">{step.title}</h1>
				<p class="text-foreground-muted text-sm">{step.subtitle}</p>
			</div>

			<div class="mb-6">
				{#if step.id === "account"}
					{#if stepDone("account")}
						<p class="text-sm text-success text-center">Your Virtues account is connected.</p>
					{:else}
						<details class="rounded-lg bg-surface-alt border border-border text-sm mb-4">
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
									<p>A Stripe customer ID, token counts on AI calls (for billing), and OAuth callbacks for ~200ms. Never content, conversations, or who you talk to.</p>
								</div>
							</div>
						</details>

						{#if accountError}
							<div class="p-3 rounded-lg bg-error-subtle border border-error/20 text-error text-sm mb-3">{accountError}</div>
						{/if}

						{#if accountMode === "choose"}
							<div class="flex flex-col gap-3">
								<Button type="button" variant="primary" class="w-full" onclick={startSubscribe}>
									Create a Virtues account · $20/mo
								</Button>
								<button type="button" class="text-sm text-foreground-muted hover:text-foreground transition-colors py-2"
									onclick={() => { accountMode = "login"; accountError = null; }}>
									I already have an account
								</button>
							</div>
						{:else if accountMode === "subscribe"}
							<div class="space-y-4 text-center">
								<a href={checkoutUrl} target="_blank" rel="noopener"
									class="inline-flex items-center gap-2 justify-center w-full px-4 py-2.5 rounded-lg bg-foreground text-surface font-medium text-sm">
									<Icon icon="ri:external-link-line" /> Open checkout
								</a>
								<p class="text-foreground-muted text-xs flex items-center justify-center gap-2">
									<Icon icon="ri:loader-4-line" class="animate-spin" /> Waiting for checkout — this advances on its own.
								</p>
							</div>
						{:else if accountMode === "login"}
							<div class="space-y-3">
								<input type="email" bind:value={email} placeholder="Email on your Virtues subscription"
									class="w-full px-3 py-2.5 rounded-lg bg-surface-alt border border-border text-sm outline-none focus:border-foreground-muted" />
								<Button type="button" variant="primary" class="w-full" onclick={startLogin}>Email me a sign-in link</Button>
								<button type="button" class="w-full text-sm text-foreground-muted hover:text-foreground transition-colors py-1"
									onclick={() => { accountMode = "choose"; accountError = null; }}>Back</button>
							</div>
						{:else if accountMode === "waiting"}
							<p class="text-foreground-muted text-sm text-center flex items-center justify-center gap-2">
								<Icon icon="ri:mail-send-line" /> Check your email and click the link — this advances on its own.
							</p>
						{/if}
					{/if}
				{:else if step.id === "device"}
					<CollectorPermissionCard onComplete={() => refreshState()} />
				{:else if step.id === "phone"}
					<div class="rounded-lg border border-border p-4 space-y-3">
						{#if stepDone("phone")}
							<p class="text-sm text-success">Your iPhone is connected.</p>
						{:else}
							<p class="text-sm text-foreground-muted">Install the Virtues app on your iPhone, then scan the code to pair.</p>
							<Button variant="primary" onclick={() => (pairModalOpen = true)}>Pair iPhone</Button>
							<p class="text-xs text-foreground-subtle">Android is coming soon.</p>
						{/if}
					</div>
				{:else if step.id === "sources"}
					<!-- The real sources UI (also at /sources) — lists every source
					     from the catalog with live sync_state and the right connect
					     flow per auth kind. No onboarding-only Google shim. -->
					<ConnectionsPanel />
				{:else if step.id === "import"}
					<ChatImportCard />
				{/if}
			</div>

			<!-- Footer: Back · (Skip for optional) · Continue/Finish -->
			<div class="flex items-center justify-between border-t border-border pt-4">
				<div>
					{#if current > 0}
						<button class="text-sm text-foreground-muted hover:text-foreground" onclick={back}>Back</button>
					{/if}
				</div>
				<div class="flex items-center gap-4">
					{#if !step.required && !stepDone(step.id)}
						<button class="text-sm text-foreground-subtle hover:text-foreground" onclick={requestSkip}>Skip</button>
					{/if}
					{#if stepDone(step.id)}
						<Button variant="primary" onclick={next}>{isLast ? "Finish" : "Continue"}</Button>
					{/if}
				</div>
			</div>
		{/if}
	</div>
</div>

<!-- Skip-confirmation: discourage bailing on an optional step by accident -->
<Modal open={skipModalOpen} onClose={() => (skipModalOpen = false)} title="Skip this step?" width="sm">
	{#snippet children()}
		<div class="space-y-4 text-sm">
			<p class="text-foreground-muted">
				You can finish this later from the sidebar's <strong>Finish setup</strong> entry — but the
				more you connect now, the more your box knows about your life. Skip <strong>{step.title}</strong> for now?
			</p>
			<div class="flex justify-end gap-3">
				<button class="text-sm text-foreground-muted hover:text-foreground px-3 py-2" onclick={() => (skipModalOpen = false)}>
					Keep going
				</button>
				<Button variant="primary" onclick={confirmSkip}>Skip — I know what I'm doing</Button>
			</div>
		</div>
	{/snippet}
</Modal>

<DevicePairModal
	deviceType="ios"
	displayName="iPhone"
	open={pairModalOpen}
	onClose={() => (pairModalOpen = false)}
	onSuccess={() => { pairModalOpen = false; phonePaired = true; void refreshState(); }}
/>
