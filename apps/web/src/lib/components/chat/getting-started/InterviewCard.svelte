<!--
	Step 4 — In your own words. A hand-off card, not the interview: that
	room is a witness with zero tools and its transcript is the drafter's
	material, so nothing of it happens here. Done when the document exists.
-->
<script lang="ts">
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";
	import { INTERVIEW_CHAT_ID } from "$lib/components/chat/interview/interview";
	import StepCard from "./StepCard.svelte";

	interface Props {
		forceOpen?: boolean;
	}
	let { forceOpen = false }: Props = $props();
	const underway = $derived(gettingStarted.step("interview")?.underway ?? false);

	function open() {
		windowShellStore.openTabFromRoute(`/chat/${INTERVIEW_CHAT_ID}`, { label: "In your own words" });
	}
</script>

<StepCard
	step="interview"
	title="In your own words"
	what="A conversation about your life, about twenty minutes, one question at a time. Stop anywhere; it keeps your place."
	skippable
	{forceOpen}
>
	<p class="why">
		The record holds what happened, not what it meant. This is where you say which years were the hard ones, who mattered, and what a good day is. Your words become a document that is yours to keep and correct.
	</p>
	<button type="button" class="btn" onclick={open}>{underway ? "Continue the interview" : "Start the interview"}</button>
</StepCard>

<style>
	.why {
		margin: 0 0 0.75rem;
		font-size: 0.9rem;
		color: var(--color-foreground-muted);
	}
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
</style>
