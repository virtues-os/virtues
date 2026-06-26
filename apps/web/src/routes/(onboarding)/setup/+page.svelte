<!--
  /setup — onboarding as ONE editorial document.

  Not a wizard of card-screens: a single page you read top to bottom, where
  each completed action streams the next chapter in. Progress is a left-margin
  table of contents (Welcome · Account · Your world · You), not a dot-stepper.
  The form is the argument — Virtues writes a document from your life, and this
  makes you watch one write itself.

  Two doors: the guided document (default) and a quiet "Set up manually →" that
  satisfies only the account gate (the one thing that blocks the app) and drops
  you into the app shell.

  Chapters:
    ① Welcome  — the threshold + the privacy pact          (always)
    ② Account  — link the wallet (the one required gate)
    ③ Your world — connect sources (M2)                     (after account)
    ④ You      — the narrative-identity reveal (M3)         (after first connect)

  All step state is read from the derived /api/setup/state so the flow survives
  refreshes and the OAuth round-trip.
-->
<script lang="ts">
	import { goto } from "$app/navigation";
	import { onMount } from "svelte";
	import { fade } from "svelte/transition";
	import { Button } from "$lib";
	import Icon from "$lib/components/Icon.svelte";
	import { getProfile, updateProfile } from "$lib/api/client";
	import OnboardingToc from "$lib/components/onboarding/document/OnboardingToc.svelte";
	import OnboardingSection from "$lib/components/onboarding/document/OnboardingSection.svelte";
	import Marginalia from "$lib/components/onboarding/document/Marginalia.svelte";
	import TypesetLines from "$lib/components/onboarding/document/TypesetLines.svelte";
	import AccountGate from "$lib/components/onboarding/document/AccountGate.svelte";
	import ConnectWorld from "$lib/components/onboarding/document/ConnectWorld.svelte";
	import RevealSection from "$lib/components/onboarding/document/RevealSection.svelte";
	import Modal from "$lib/components/Modal.svelte";

	type Step = { id: string; title: string; done: boolean; detail?: string; kind?: string };
	type SetupState = { setup: Step[]; setup_complete: boolean; onboarding: Step[] };

	let state_ = $state<SetupState | null>(null);
	let loading = $state(true);
	let mode = $state<"document" | "manual">("document");
	let advancedOpen = $state(false);
	let scrollEl = $state<HTMLElement | null>(null);
	let reduced = $state(false);

	// Optimistic local flags — flip a step the instant the local signal fires,
	// before the next server poll confirms it (used by the world chapter).
	let deviceReady = $state(false);
	let phonePaired = $state(false);
	// The user chose to move on to the reveal without (or before) connecting.
	let proceeded = $state(false);

	function setupDone(id: string): boolean {
		return state_?.setup.find((s) => s.id === id)?.done ?? false;
	}
	function onboardingDone(id: string): boolean {
		return state_?.onboarding.find((s) => s.id === id)?.done ?? false;
	}

	const accountDone = $derived(setupDone("account"));
	const worldEnough = $derived(
		onboardingDone("first_source") ||
			onboardingDone("living_source") ||
			onboardingDone("first_phone") ||
			onboardingDone("chat_imported") ||
			deviceReady,
	);
	const narrativeReady = $derived(onboardingDone("narrative_identity_ready"));
	const narrativeGenerating = $derived(
		state_?.onboarding.find((s) => s.id === "narrative_identity_ready")?.kind === "generating",
	);
	// The reveal is reachable once you've connected something, or chosen to move on.
	const showReveal = $derived(worldEnough || proceeded);

	// The TOC mirrors the document: chapters appear as they become reachable, so
	// nothing in the rail points at a section that isn't there yet.
	const chapters = $derived([
		{ id: "welcome", label: "Welcome" },
		{ id: "account", label: "Account" },
		...(accountDone ? [{ id: "world", label: "Your world" }] : []),
		...(showReveal ? [{ id: "reveal", label: "You" }] : []),
	]);
	const completedIds = $derived(
		[
			accountDone && "welcome",
			accountDone && "account",
			worldEnough && "world",
			narrativeReady && "reveal",
		].filter(Boolean) as string[],
	);

	async function refreshState() {
		try {
			const r = await fetch("/api/setup/state");
			if (r.ok) state_ = await r.json();
		} catch {
			/* box briefly unreachable — keep last state */
		} finally {
			loading = false;
		}
	}

	// Cloud/onboarding cross-check for home_timezone (the box's location).
	// The box normally seeds this from its own system clock; but a datacenter box
	// reads "UTC", which is wrong. So only fall back to the onboarding browser's
	// zone when the server value is unset or UTC — a real appliance configured at
	// home keeps its server-detected zone. See docs/timezone-model.md.
	async function captureTimezone() {
		try {
			const tz = Intl.DateTimeFormat().resolvedOptions().timeZone;
			if (!tz) return;
			const p = await getProfile();
			const current = p.home_timezone;
			if (!current || current === "UTC") {
				if (current !== tz) await updateProfile({ home_timezone: tz });
			}
		} catch {
			/* non-essential */
		}
	}

	onMount(() => {
		reduced = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
		void refreshState();
		void captureTimezone();
		// Light poll so steps completed elsewhere (OAuth round-trip, the collector
		// daemon, the CLI) tick over here too.
		const t = setInterval(refreshState, 4000);
		return () => clearInterval(t);
	});

	function enterApp() {
		void goto("/");
	}
	function proceedToReveal() {
		proceeded = true;
		setTimeout(() => {
			scrollEl
				?.querySelector("#reveal")
				?.scrollIntoView({ behavior: reduced ? "auto" : "smooth", block: "start" });
		}, 60);
	}
	function confirmAdvanced() {
		advancedOpen = false;
		mode = "manual";
	}
</script>

{#if loading}
	<div class="flex min-h-screen items-center justify-center gap-2.5 text-sm text-foreground-muted" in:fade>
		<Icon icon="ri:loader-4-line" class="animate-spin" />
		<span>Checking your box…</span>
	</div>
{:else if !state_}
	<div class="flex min-h-screen items-center justify-center px-6">
		<div class="rounded-xl border border-error/20 bg-error-subtle p-4 text-sm text-error" in:fade>
			Couldn't reach the box. Make sure you're on the same network, then refresh.
		</div>
	</div>
{:else if mode === "manual"}
	<!-- The advanced door: just the account gate, then into the app. -->
	<div class="flex min-h-screen items-center justify-center px-6 py-16">
		<div class="w-full max-w-md">
			<p class="mb-2 font-mono text-[0.6875rem] uppercase tracking-[0.16em] text-foreground-subtle">Skipped setup</p>
			<h1 class="mb-6 font-serif text-2xl tracking-tight text-foreground">Sign in to Virtues</h1>
			<AccountGate done={accountDone} onLinked={refreshState} />
			{#if accountDone}
				<div class="mt-8" in:fade>
					<Button variant="primary" class="w-full justify-center py-2.5" onclick={enterApp}>Enter Virtues →</Button>
				</div>
			{/if}
			<button class="mt-6 font-mono text-xs text-foreground-subtle transition-colors hover:text-foreground" onclick={() => (mode = "document")}>
				← Back to guided setup
			</button>
		</div>
	</div>
{:else}
	<div class="doc-scroll" bind:this={scrollEl}>
		<div class="doc-inner">
			<div class="doc-toc">
				<OnboardingToc {chapters} {completedIds} scrollContainer={scrollEl} {reduced} />
			</div>

			<div class="doc-column">
				<!-- ① Welcome -->
				<OnboardingSection id="welcome" kicker="Virtues" title="An honest record of your life" {reduced}>
					<TypesetLines
						lines={[
							"Virtues runs on the box in your home. Connect your accounts and devices, and it builds a private record of your life — your messages, calendar, places, health — that you can search, read, and reflect on.",
							"Nothing leaves the box. Not to a government, not to a tech company, not to us. We can't see what you connect or what it learns, because it's built so we can't.",
						]}
						{reduced}
					/>
					<p class="mt-10 flex items-center gap-1.5 text-sm text-foreground-subtle">
						<Icon icon="ri:arrow-down-line" width="15" /> Begin
					</p>
				</OnboardingSection>

				<!-- ② Account -->
				<OnboardingSection id="account" kicker="The one thing that needs a server" title="Sign in to Virtues" {reduced}>
					<p class="mb-6">
						Your subscription is the only part of Virtues that uses our servers — it handles sign-in and pays for the AI
						your box calls on. It's built in two separate halves: one knows you pay us, the other runs your AI requests.
						The two can't be connected, so we can't link who you are to anything you do. Everything else stays on the box.
					</p>
					<AccountGate done={accountDone} onLinked={refreshState} />
					<Marginalia tone="receipt">sign-in and billing use our servers · your data never does</Marginalia>
				</OnboardingSection>

				<!-- ③ Your world -->
				{#if accountDone}
					<OnboardingSection id="world" kicker="Your world" title="Where the record comes from" {reduced}>
						<ConnectWorld onConnected={refreshState} onDeviceReady={() => (deviceReady = true)} />
						{#if !showReveal}
							<div class="world-foot">
								<button class="continue" onclick={proceedToReveal}>
									{worldEnough ? "Continue →" : "Skip for now →"}
								</button>
								<span class="continue-note">Connecting is optional — you can add sources anytime, from the app.</span>
							</div>
						{/if}
					</OnboardingSection>
				{/if}

				<!-- ④ You — the reveal -->
				{#if showReveal}
					<OnboardingSection id="reveal" kicker="You" title="Meet yourself" {reduced}>
						<RevealSection ready={narrativeReady} generating={narrativeGenerating} {reduced} onEnter={enterApp} />
					</OnboardingSection>
				{/if}
			</div>

			<div class="doc-gutter"></div>
		</div>

		<button class="manual-link" onclick={() => (advancedOpen = true)}>Skip setup →</button>
	</div>
{/if}

<!-- The advanced door's confirm — the safe choice (Stay guided) is the loud one. -->
<Modal open={advancedOpen} onClose={() => (advancedOpen = false)} title="Skip the guided setup?" width="sm">
	{#snippet children()}
		<div class="space-y-5 text-sm">
			<p class="leading-relaxed text-foreground-muted">
				The guided path connects your life and ends with a first draft of your narrative identity. Skipping drops you
				straight into the raw box — sources, tables, and actions, with nothing drafted for you. You'll still link your
				account (the one thing the app needs), and you can come back to setup anytime from the sidebar.
			</p>
			<div class="flex items-center justify-end gap-4">
				<button class="text-sm text-foreground-subtle transition-colors hover:text-foreground" onclick={confirmAdvanced}>
					Skip anyway
				</button>
				<Button variant="primary" class="px-5 py-2.5" onclick={() => (advancedOpen = false)}>Stay guided</Button>
			</div>
		</div>
	{/snippet}
</Modal>

<style>
	@reference "../../../app.css";

	.doc-scroll {
		position: relative;
		height: 100vh;
		overflow-y: auto;
	}

	.doc-inner {
		display: grid;
		grid-template-columns: 9rem minmax(0, 34rem) 1fr;
		gap: 2.5rem;
		max-width: 72rem;
		margin-inline: auto;
		padding: 8vh 2rem 16vh;
	}

	.doc-toc {
		grid-column: 1;
	}
	.doc-column {
		grid-column: 2;
	}
	.doc-gutter {
		grid-column: 3;
	}

	/* Below the gutter width, collapse to a single centered reading column. */
	@media (max-width: 1200px) {
		.doc-inner {
			grid-template-columns: minmax(0, 1fr);
			max-width: 38rem;
		}
		.doc-toc,
		.doc-column,
		.doc-gutter {
			grid-column: 1;
		}
	}

	.world-foot {
		margin-top: 2.5rem;
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 0.25rem 1rem;
		border-top: 1px solid var(--color-border-subtle);
		padding-top: 1.5rem;
	}
	.continue {
		font-size: 0.95rem;
		font-weight: 500;
		color: var(--color-foreground);
		transition: color 0.15s ease;
	}
	.continue:hover {
		color: var(--color-primary);
	}
	.continue-note {
		font-size: 0.8rem;
		color: var(--color-foreground-subtle);
	}

	.manual-link {
		position: fixed;
		top: 1.5rem;
		right: 1.75rem;
		font-family: var(--font-mono);
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
		transition: color 0.15s ease;
		z-index: 20;
	}
	.manual-link:hover {
		color: var(--color-foreground);
	}
</style>
