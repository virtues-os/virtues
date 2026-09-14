<!--
	The room's controls: one surface, under the transcript and above the
	composer, showing what the step being waited on needs. Never a message,
	never inline with prose — the room speaks in turns (the server appends
	them), and this is the one place a person acts.
-->
<script lang="ts">
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";
	import ConnectAiActions from "./ConnectAiActions.svelte";
	import IntegrationsPicker from "./IntegrationsPicker.svelte";
	import ChapterLifelineLive from "$lib/components/chat/interview/ChapterLifelineLive.svelte";
	import Act from "./ui/Act.svelte";
	import Choices from "./ui/Choices.svelte";

	const step = $derived(gettingStarted.steps.find((s) => s.status === "open") ?? null);
	const interviewUnderway = $derived(gettingStarted.interviewUnderway);
	const interviewDone = $derived(gettingStarted.step("interview")?.status === "done");

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

{#if interviewDone}
	<!-- The close answers the opening: the same plate, drawn from the
	     chapters the person named. -->
	<ChapterLifelineLive />
{:else if step && !interviewUnderway}
	<section class="controls" aria-label="What this step needs">
		{#if step.id === "connect_ai"}
			<ConnectAiActions />
		{:else if step.id === "introductions"}
			<!-- Nothing to press: they answer in the composer, and the model
			     plays it back with one button to confirm. -->
		{:else if step.id === "connect_world"}
			<IntegrationsPicker />
			<Choices>
				<Act variant="primary" onclick={() => void gettingStarted.skip("connect_world", true)}>Continue</Act>
			</Choices>
		{:else if step.id === "interview"}
			<Choices>
				<Act variant="primary" disabled={starting} onclick={startInterview}>
					{starting ? "Starting…" : "Start the interview"}
				</Act>
				<Act onclick={() => void gettingStarted.skip("interview", true)}>Not now</Act>
			</Choices>
		{/if}
	</section>
{/if}

<style>
	.controls {
		max-width: 48rem;
		margin: 0 auto;
		padding: 0 2rem 0.5rem;
		width: 100%;
		box-sizing: border-box;
	}
</style>
