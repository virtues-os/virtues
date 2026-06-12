<!--
  First-run onboarding wizard.

  Where the user lands after `virtues init` → setup-token handshake on
  `/pair#t=…`. Single-page, single-tenant — this box has one
  owner, this is their first time signing in. Goal: orient + get them to
  the first useful thing (connect a source) in the smallest number of
  clicks possible.

  Intentionally NOT the main app shell. No sidebar, no tabs, no chrome
  competing for attention. Once the user clicks past this screen, they
  enter the full UI and never see this page again.
-->
<script lang="ts">
	import { goto } from "$app/navigation";
	import { onMount } from "svelte";
	import { Button } from "$lib";
	import Icon from "$lib/components/Icon.svelte";
	import { getSetupState, type SetupStep } from "$lib/api/client";

	let isContinuing = $state(false);

	// One-shot fetch — this is a splash, not a dashboard; no polling.
	// On fetch failure `onboarding` stays null and the page renders the
	// static splash exactly as before.
	let onboarding = $state<SetupStep[] | null>(null);

	onMount(async () => {
		try {
			const s = await getSetupState();
			onboarding = s.onboarding ?? null;
		} catch {
			/* box briefly unreachable — render static splash */
		}
	});

	function stepDone(id: string): boolean {
		return onboarding?.find((s) => s.id === id)?.done ?? false;
	}

	const firstSourceDone = $derived(stepDone("first_source"));
	const firstSyncDone = $derived(stepDone("first_sync"));

	async function continueToSources() {
		isContinuing = true;
		// The Sources tab is where the user actually connects their first
		// integration (Google, Notion, Strava, Plaid). Landing there with a
		// short tooltip is the v1 onboarding endgame; v1.1 will replace this
		// with an embedded source picker right inside the wizard.
		await goto("/sources?welcome=1");
	}

	async function skip() {
		await goto("/");
	}
</script>

<div class="min-h-screen flex items-center justify-center px-6 py-12">
	<div class="w-full max-w-xl">
		<!-- Header -->
		<div class="mb-10 text-center">
			<div class="inline-flex items-center justify-center w-16 h-16 rounded-2xl bg-surface-alt border border-border mb-6">
				<Icon icon="ri:flashlight-line" class="text-3xl text-foreground" />
			</div>
			<h1 class="text-3xl font-semibold tracking-tight mb-3">
				Welcome to Virtues
			</h1>
			<p class="text-foreground-muted text-base">
				This box is yours. Your data, your hardware, your subscription.
			</p>
		</div>

		<!-- What's next -->
		<div class="mb-10 space-y-6">
			<div class="flex items-start gap-4">
				{#if firstSourceDone}
					<div class="flex-shrink-0 w-8 h-8 flex items-center justify-center">
						<Icon icon="ri:checkbox-circle-fill" class="text-2xl text-success" />
					</div>
				{:else}
					<div class="flex-shrink-0 w-8 h-8 rounded-full bg-surface-alt border border-border flex items-center justify-center text-sm font-medium">
						1
					</div>
				{/if}
				<div>
					<div class="font-medium mb-1">Connect a data source</div>
					<div class="text-foreground-muted text-sm">
						Google, Notion, Strava, Plaid, and more. Each one feeds your private knowledge graph.
					</div>
				</div>
			</div>

			<div class="flex items-start gap-4">
				{#if firstSyncDone}
					<div class="flex-shrink-0 w-8 h-8 flex items-center justify-center">
						<Icon icon="ri:checkbox-circle-fill" class="text-2xl text-success" />
					</div>
				{:else}
					<div class="flex-shrink-0 w-8 h-8 rounded-full bg-surface-alt border border-border flex items-center justify-center text-sm font-medium">
						2
					</div>
				{/if}
				<div>
					<div class="font-medium mb-1">Let Virtues organize</div>
					<div class="text-foreground-muted text-sm">
						The box ingests, indexes, and builds a coherent picture of your life. This runs in the background.
					</div>
				</div>
			</div>

			<div class="flex items-start gap-4">
				<div class="flex-shrink-0 w-8 h-8 rounded-full bg-surface-alt border border-border flex items-center justify-center text-sm font-medium">
					3
				</div>
				<div>
					<div class="font-medium mb-1">Ask, write, navigate</div>
					<div class="text-foreground-muted text-sm">
						Chat with an AI that has real context. Browse your life in the Wiki. Everything stays on your box.
					</div>
				</div>
			</div>
		</div>

		<!-- Privacy callout -->
		<div class="mb-8 p-4 rounded-lg bg-surface-alt border border-border text-sm text-foreground-muted">
			<div class="font-medium text-foreground mb-1">
				About AI privacy
			</div>
			<p>
				AI calls pass through Virtues servers in memory only — never logged, never stored. The code is open source. To take Virtues out of the AI path entirely, you can bring your own provider key in Settings later.
			</p>
		</div>

		<!-- Actions -->
		<div class="flex flex-col gap-3">
			<Button
				type="button"
				variant="primary"
				disabled={isContinuing}
				onclick={continueToSources}
				class="w-full"
			>
				{#if isContinuing}
					<Icon icon="ri:loader-4-line" class="animate-spin" />
					Opening…
				{:else if firstSourceDone}
					Continue to sources
				{:else}
					Connect your first source
				{/if}
			</Button>

			<button
				type="button"
				onclick={skip}
				class="text-sm text-foreground-muted hover:text-foreground transition-colors py-2"
			>
				Skip for now, take me to the app
			</button>
		</div>
	</div>
</div>
