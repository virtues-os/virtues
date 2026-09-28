<!--
	Setup, in the app. The same steps as the full-screen stage, in a pane
	beside the sidebar, for someone who is already in the app and comes back
	to a step they skipped or want to change. Where `/setup` opens is decided
	before any page loads (setup/inApp.ts, src/hooks.ts): the stage the first
	time through, this tab after.

	WHAT IS HERE. The steps a person can do from inside the app: the letter
	(to read again), Names, Subscription, Devices, Chapters and the interview.
	Welcome's opening only plays full screen, so it is a link here. The
	first half (account, server, Wi-Fi) is finished for anyone who can see
	this, so it isn't listed.

	WHAT ISN'T. The stage's grammar: no mark overhead, no music, no close.
	Leaving is closing the tab. When the last open step is done the tab says
	so, and nothing else happens.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import { goto } from "$app/navigation";
	import type { Tab } from "$lib/tabs/types";
	import "$lib/components/setup/setup.css";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";
	import {
		setup,
		isSetupStep,
		intoLabel,
		labelOf,
		SERVER,
		type SetupStepId,
	} from "$lib/components/setup/setup.svelte";
	import StepFrame from "$lib/components/setup/StepFrame.svelte";
	import FoundersLetter from "$lib/components/onboarding/document/FoundersLetter.svelte";
	import StepNames from "$lib/components/setup/steps/StepNames.svelte";
	import StepSubscription from "$lib/components/setup/steps/StepSubscription.svelte";
	import StepConnections from "$lib/components/setup/steps/StepConnections.svelte";
	import StepTimeline from "$lib/components/setup/steps/StepTimeline.svelte";
	import StepInterview from "$lib/components/setup/steps/StepInterview.svelte";

	let { tab }: { tab: Tab; active?: boolean } = $props();

	/** The steps this tab offers, in Setup's order. */
	const HERE: SetupStepId[] = ["letter", "names", "subscription", "connections", "timeline", "interview"];

	const param = $derived(tab.route.split("/")[2] ?? null);
	const step = $derived<SetupStepId | null>(
		isSetupStep(param) && HERE.includes(param) ? param : null,
	);

	let loaded = $state(false);
	onMount(() => {
		void setup.refresh().finally(() => (loaded = true));
	});

	const steps = $derived(setup.steps.filter((s) => HERE.includes(s.id)));
	/** Coming back is not the first time through: any step opens once the
	 *  required ones before it are done. (The stage is strict, because each
	 *  step there assumes the last; here the person is choosing one.) */
	function open(id: SetupStepId): boolean {
		const all = setup.steps;
		const at = all.findIndex((s) => s.id === id);
		return all.slice(0, at).every((s) => s.optional || s.status === "done");
	}
	/** The first step here still to do. */
	const next = $derived(steps.find((s) => s.status !== "done" && s.id !== "letter")?.id ?? null);

	function go(id: SetupStepId | null) {
		windowShellStore.openSetup(id ? `/setup/${id}` : "/setup");
	}

	/** On to the next step here still to do, or back to the list. */
	async function advance(from: SetupStepId) {
		await setup.refresh();
		const i = HERE.indexOf(from);
		const after = steps.find((s) => HERE.indexOf(s.id) > i && s.status !== "done");
		go(after?.id ?? null);
	}

	async function skip(id: SetupStepId) {
		const key = SERVER[id];
		try {
			if (key) await gettingStarted.skip(key, true);
		} catch {
			/* moving on never waits on the server */
		}
		await advance(id);
	}

	const mark = (status: string) => (status === "done" ? "done" : status === "skipped" ? "skipped" : "open");
	const said = (status: string) =>
		status === "done" ? "done" : status === "skipped" ? "skipped" : "still to do";
</script>

<div class="host">
	<nav class="strip" aria-label="Setup steps">
		<button type="button" class="crumb" class:current={!step} onclick={() => go(null)}>Setup</button>
		<ol>
			{#each steps as s (s.id)}
				<li>
					<button
						type="button"
						class="step"
						class:current={step === s.id}
						disabled={!open(s.id)}
						aria-current={step === s.id ? "step" : undefined}
						aria-label="{labelOf(s.id)}, {said(s.status)}"
						onclick={() => go(s.id)}
					>
						<span class="dot {mark(s.status)}" aria-hidden="true"></span>
						{labelOf(s.id)}
					</button>
				</li>
			{/each}
		</ol>
	</nav>

	{#if !loaded}
		<div class="blank" aria-hidden="true"></div>
	{:else if step === "letter"}
		<div class="letter">
			<article class="paper">
				<FoundersLetter beginLabel={next ? intoLabel(next) : "Back to Setup"} onbegin={() => go(next)} />
			</article>
		</div>
	{:else if step === "names"}
		<StepNames onnext={() => advance("names")} />
	{:else if step === "subscription"}
		<StepSubscription onnext={() => advance("subscription")} />
	{:else if step === "connections"}
		<StepConnections onnext={() => advance("connections")} />
	{:else if step === "timeline"}
		<StepTimeline onnext={() => advance("timeline")} onskip={() => skip("timeline")} />
	{:else if step === "interview"}
		<StepInterview onnext={() => advance("interview")} onskip={() => skip("interview")} ondraw={() => go("timeline")} />
	{:else}
		<StepFrame
			title={next ? "Pick up where you left off" : "Setup is done"}
			subtitle={next
				? "Each step is a few minutes, and you can leave any of them and come back."
				: "You can open any step from the list above to change it."}
		>
			<ul class="list">
				{#each steps as s (s.id)}
					<li>
						<button type="button" class="row" disabled={!open(s.id)} onclick={() => go(s.id)}>
							<span class="dot {mark(s.status)}" aria-hidden="true"></span>
							<span class="row-label">{labelOf(s.id)}</span>
							<span class="row-state">{said(s.status)}</span>
						</button>
					</li>
				{/each}
			</ul>
			{#snippet actions()}
				{#if next}
					<button type="button" class="setup-go" onclick={() => go(next)}>{intoLabel(next)}</button>
				{/if}
				<button type="button" class="setup-past" onclick={() => goto("/setup/welcome")}>Play the opening again</button>
			{/snippet}
		</StepFrame>
	{/if}
</div>

<style>
	.host {
		height: 100%;
		overflow-y: auto;
		display: flex;
		flex-direction: column;
		background: var(--color-background);
	}
	.blank {
		flex: 1;
	}

	/* The steps, as a line of words under the tab: where you are, and the
	   way to any other. It scrolls sideways in a narrow pane rather than
	   wrapping into a second line of chrome. */
	.strip {
		flex: none;
		position: sticky;
		top: 0;
		z-index: 2;
		display: flex;
		align-items: center;
		gap: 16px;
		padding: 8px 16px;
		border-bottom: 1px solid var(--color-border);
		background: var(--color-background);
		overflow-x: auto;
		scrollbar-width: none;
	}
	.strip ol {
		display: flex;
		gap: 4px;
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.crumb,
	.step {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		min-height: 32px;
		padding: 0 8px;
		border: none;
		border-radius: 6px;
		background: none;
		font: inherit;
		font-size: 13px;
		white-space: nowrap;
		color: var(--color-foreground-muted);
		cursor: pointer;
		transition: color var(--m-quick) ease, background-color var(--m-quick) ease;
	}
	.crumb {
		color: var(--color-foreground);
	}
	.crumb:hover:not(.current),
	.step:hover:not(:disabled) {
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
		color: var(--color-foreground);
	}
	.step.current,
	.crumb.current {
		color: var(--color-foreground);
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
	}
	.step:disabled {
		opacity: 0.4;
		cursor: default;
	}
	.crumb:focus-visible,
	.step:focus-visible,
	.row:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
	}

	/* One mark per state: filled done, hollow to do, a bar set aside. */
	.dot {
		flex: none;
		width: 8px;
		height: 8px;
		border-radius: 50%;
		border: 1.5px solid currentColor;
	}
	.dot.done {
		border-color: var(--color-success);
		background: var(--color-success);
	}
	.dot.skipped {
		height: 0;
		border-radius: 0;
		border-width: 1px 0 0;
	}

	.list {
		width: 100%;
		max-width: 28rem;
		margin: 0 auto;
		padding: 0;
		list-style: none;
		border-top: 1px solid var(--color-border);
	}
	.list li {
		border-bottom: 1px solid var(--color-border);
	}
	.row {
		display: flex;
		align-items: center;
		gap: 12px;
		width: 100%;
		min-height: 48px;
		padding: 0 4px;
		border: none;
		background: none;
		font: inherit;
		font-size: 15px;
		text-align: left;
		color: var(--color-foreground);
		cursor: pointer;
	}
	.row:disabled {
		opacity: 0.4;
		cursor: default;
	}
	.row-label {
		flex: 1;
	}
	.row-state {
		font-size: 13px;
		color: var(--color-foreground-subtle);
	}

	/* The letter as a sheet in the pane, as on the stage but without its
	   entrance: here it is being reread, not arriving. */
	.letter {
		display: flex;
		justify-content: center;
		padding: 32px 16px 48px;
	}
	.paper {
		width: 100%;
		max-width: 46rem;
		padding: clamp(2rem, 5vw, 4rem) clamp(1.25rem, 5vw, 4rem);
		border-radius: 6px;
		background: var(--color-surface-overlay, var(--color-surface));
		outline: 1px solid color-mix(in srgb, var(--color-foreground) 9%, transparent);
		outline-offset: -1px;
	}

	@media (prefers-reduced-motion: reduce) {
		.crumb,
		.step {
			transition: none;
		}
	}
</style>
