<!--
	The room's controls: one surface, under the transcript and above the
	composer, showing what the step being waited on needs. Never a message,
	never inline with prose — the room speaks in turns (the server appends
	them), and this is the one place a person acts.
-->
<script lang="ts">
	import { goto } from "$app/navigation";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";
	import ConnectAiActions from "./ConnectAiActions.svelte";
	import IntegrationsPicker from "./IntegrationsPicker.svelte";
	import Act from "./ui/Act.svelte";
	import Choices from "./ui/Choices.svelte";

	const step = $derived(gettingStarted.steps.find((s) => s.status === "open") ?? null);
	const interviewUnderway = $derived(gettingStarted.interviewUnderway);
	/** The first day the box wrote up, once one exists: the promise, shown
	 *  rather than told — a door to the page itself. */
	const firstDay = $derived(gettingStarted.state?.first_day ?? null);

	let starting = $state(false);
	async function startInterview() {
		if (starting) return;
		starting = true;
		try {
			await gettingStarted.startInterview();
		} finally {
			starting = false;
		}
	}
</script>

<!-- NOT the chapters plate. It used to stand here whenever the interview was
     done, which made a MOMENT into furniture: it never went away, and every
     message sent afterwards slid in above it — ask "what's next?" and your own
     question appears over the top of your life. The plate belongs beside the
     close that produced it, inline in the thread, and ChatView renders it
     there with the two doors. The permanent way back is the Chapters door on
     that card. -->
{#if step && !interviewUnderway}
	<section class="controls" aria-label="What this step needs">
		{#if step.id === "connect_ai"}
			<ConnectAiActions />
		{:else if step.id === "introductions"}
			<!-- Nothing to press: they answer in the composer, and the model
			     plays it back with one button to confirm. -->
		{:else if step.id === "connect_world"}
			<IntegrationsPicker />
			<Choices>
				<!-- THIS BUTTON IS THE COMPLETION. The step no longer closes
				     itself when the first credential lands — it closes when the
				     person says they are finished adding — so Continue is the
				     thing that ends the step, not a no-op beside a status that
				     had already moved on. With nothing connected it is the same
				     "Not now" every other step uses for setting aside. -->
				{#if (step.connected ?? 0) > 0}
					<Act variant="primary" onclick={() => void gettingStarted.skip("connect_world", true)}>Continue</Act>
				{:else}
					<Act onclick={() => void gettingStarted.skip("connect_world", true)}>Not now</Act>
				{/if}
			</Choices>
		{:else if step.id === "interview"}
			<Choices>
				<Act variant="primary" disabled={starting} onclick={startInterview}>
					{starting ? "Starting…" : "Start the interview"}
				</Act>
				<Act onclick={() => void gettingStarted.skip("interview", true)}>Not now</Act>
				{#if firstDay}
					<Act variant="plain" onclick={() => void goto(`/day/day_${firstDay}`)}>Read your first page</Act>
				{/if}
			</Choices>
		{/if}
	</section>
{/if}

<style>
	/* No width, no gutter of its own: this sits INSIDE .messages-container,
	   which already holds the column to 48rem and its 2rem gutters. Setting
	   them again here indented every button by a full gutter past the prose
	   it belongs to. */
	.controls {
		width: 100%;
	}
</style>
