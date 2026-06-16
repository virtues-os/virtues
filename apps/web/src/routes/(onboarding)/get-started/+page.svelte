<!--
  /get-started — the guided onboarding stepper.

  Picks up where /setup (the required gate: account → name → network) ends and
  walks the user through connecting the platform, one focused step at a time:

      this device (collector) → your phone → calendar + email → chat history

  This is the handholding first-run vehicle. Every step is SKIPPABLE (power-user
  bypass + "I don't own a Mac" reality) — we never block forward motion. Whatever
  is skipped or still syncing lands in the dashboard's NextWinsChecklist, which is
  the persistent "what's left" backlog. Step completion is read from the derived
  /api/setup/state, so the stepper survives refreshes and the OAuth round-trip.
-->
<script lang="ts">
	import { goto } from "$app/navigation";
	import { onMount, onDestroy } from "svelte";
	import Icon from "$lib/components/Icon.svelte";
	import { Button } from "$lib";
	import { setupStateStore } from "$lib/stores/setupState.svelte";
	import { oauthStart } from "$lib/api/client";
	import CollectorPermissionCard from "$lib/components/onboarding/CollectorPermissionCard.svelte";
	import ChatImportCard from "$lib/components/onboarding/ChatImportCard.svelte";
	import DevicePairModal from "$lib/components/sources/DevicePairModal.svelte";

	type StepId = "device" | "phone" | "sources" | "import";

	const steps: { id: StepId; title: string; subtitle: string }[] = [
		{
			id: "device",
			title: "Set up this device",
			subtitle: "Let Virtues remember what happens on this machine. It all stays on your box.",
		},
		{
			id: "phone",
			title: "Add your phone",
			subtitle: "Your richest source — where you go, who you message, your health.",
		},
		{
			id: "sources",
			title: "Connect calendar & email",
			subtitle: "These keep flowing on their own. The backfill runs in the background — you don't have to wait.",
		},
		{
			id: "import",
			title: "Bring your chat history",
			subtitle: "A one-time import of your past Claude, ChatGPT, or Gemini conversations.",
		},
	];

	let current = $state(0);
	const step = $derived(steps[current]);
	const isLast = $derived(current === steps.length - 1);

	// Local completion flags for steps with no precise derived signal.
	let phonePaired = $state(false);
	let imported = $state(false);
	let pairModalOpen = $state(false);
	let connectingGoogle = $state(false);
	let sourcesError = $state<string | null>(null);

	// Done-state per step, derived from /api/setup/state where possible.
	function stepDone(id: StepId): boolean {
		const get = (sid: string) =>
			setupStateStore.onboarding.find((s) => s.id === sid)?.done ?? false;
		switch (id) {
			case "device":
				return get("device_collecting");
			case "phone":
				return phonePaired || get("first_device");
			case "sources":
				return get("first_source") || get("living_source");
			case "import":
				return imported;
		}
	}

	onMount(() => {
		void setupStateStore.check();
		// Mirror panel behavior: pick up steps completed elsewhere / after the
		// OAuth round-trip. Cheap, and stops when the user leaves.
		const t = setInterval(() => setupStateStore.check(), 4000);
		return () => clearInterval(t);
	});
	onDestroy(() => setupStateStore.stop?.());

	function next() {
		if (isLast) {
			void goto("/");
			return;
		}
		current += 1;
	}
	function back() {
		if (current > 0) current -= 1;
	}
	function skipToDashboard() {
		void goto("/");
	}

	async function connectGoogle() {
		connectingGoogle = true;
		sourcesError = null;
		try {
			const { redirect_url } = await oauthStart("google", {
				// Return to the stepper so the guided flow resumes after OAuth.
				return_url: `${window.location.origin}/get-started`,
			});
			window.location.assign(redirect_url);
		} catch (e) {
			sourcesError = e instanceof Error ? e.message : String(e);
			connectingGoogle = false;
		}
	}
</script>

<div class="min-h-screen flex items-center justify-center px-6 py-12">
	<div class="w-full max-w-md">
		<!-- Progress rail -->
		<div class="mb-10 flex items-center justify-center gap-2">
			{#each steps as s, i (s.id)}
				<div
					class="flex items-center gap-1.5 text-xs {i === current
						? 'text-foreground'
						: 'text-foreground-muted'}"
					title={s.title}
				>
					<Icon
						icon={stepDone(s.id)
							? "ri:checkbox-circle-fill"
							: i === current
								? "ri:circle-fill"
								: "ri:checkbox-blank-circle-line"}
						class={stepDone(s.id) ? "text-success" : i === current ? "text-primary" : ""}
					/>
					<span class="hidden sm:inline">{s.title}</span>
				</div>
			{/each}
		</div>

		<!-- Active step -->
		<div class="space-y-2 mb-5">
			<h1 class="text-2xl font-semibold tracking-tight">{step.title}</h1>
			<p class="text-foreground-muted text-sm">{step.subtitle}</p>
		</div>

		<div class="mb-6">
			{#if step.id === "device"}
				<CollectorPermissionCard onComplete={() => setupStateStore.check()} />
			{:else if step.id === "phone"}
				<div class="rounded-lg border border-border p-4 space-y-3">
					{#if stepDone("phone")}
						<p class="text-sm text-success">Your phone is connected.</p>
					{:else}
						<p class="text-sm text-foreground-muted">
							Install the Virtues app on your phone, then scan the code to pair.
						</p>
						<Button variant="primary" onclick={() => (pairModalOpen = true)}>Pair iPhone</Button>
					{/if}
				</div>
			{:else if step.id === "sources"}
				<div class="rounded-lg border border-border p-4 space-y-3">
					{#if stepDone("sources")}
						<p class="text-sm text-success">Google is connected and backfilling.</p>
					{:else}
						<p class="text-sm text-foreground-muted">
							Connect Google to sync your Calendar and Mail (read-only).
						</p>
						<Button variant="primary" onclick={connectGoogle} disabled={connectingGoogle}>
							{connectingGoogle ? "Redirecting…" : "Connect Google"}
						</Button>
						{#if sourcesError}
							<p class="text-sm text-error">{sourcesError}</p>
						{/if}
					{/if}
				</div>
			{:else if step.id === "import"}
				<ChatImportCard />
			{/if}
		</div>

		<!-- Footer: never block — Skip is first-class. -->
		<div class="flex items-center justify-between border-t border-border pt-4">
			<div>
				{#if current > 0}
					<button class="text-sm text-foreground-muted hover:text-foreground" onclick={back}>
						Back
					</button>
				{/if}
			</div>
			<div class="flex items-center gap-3">
				{#if !isLast}
					<button class="text-sm text-foreground-muted hover:text-foreground" onclick={next}>
						Skip
					</button>
				{/if}
				<Button variant="primary" onclick={next}>
					{isLast ? "Finish" : stepDone(step.id) ? "Next" : "Continue"}
				</Button>
			</div>
		</div>

		<div class="mt-4 text-center">
			<button class="text-xs text-foreground-subtle hover:text-foreground-muted" onclick={skipToDashboard}>
				Skip setup — I'll finish from the dashboard
			</button>
		</div>
	</div>
</div>

<DevicePairModal
	deviceType="ios"
	displayName="iPhone"
	open={pairModalOpen}
	onClose={() => (pairModalOpen = false)}
	onSuccess={() => {
		pairModalOpen = false;
		phonePaired = true;
		void setupStateStore.check();
	}}
/>
