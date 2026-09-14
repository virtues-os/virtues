<!--
	One synthetic message of the getting-started room, dispatched by id:
	the mast, a step's card, the promise line, or the authored first line.
	ChatView hands every `gs-*` message here instead of the markdown path.
-->
<script lang="ts">
	import {
		GS_MAST_ID,
		GS_PROMISE_ID,
		GS_FIRST_LINE_ID,
		FIRST_LINE,
		gsCardStep,
	} from "./getting-started";
	import type { GettingStartedStepId } from "$lib/api/client";
	import GettingStartedMast from "./GettingStartedMast.svelte";
	import ConnectAiCard from "./ConnectAiCard.svelte";
	import IntroductionsAsk from "./IntroductionsAsk.svelte";
	import ConnectWorldCard from "./ConnectWorldCard.svelte";
	import InterviewCard from "./InterviewCard.svelte";
	import PromiseLine from "./PromiseLine.svelte";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";

	interface Props {
		id: string;
		/** Send text as the person's turn (introductions). */
		onSend: (text: string) => void;
		/** A `show_step` tool part: the card, opened, whatever its position. */
		forceOpen?: boolean;
	}
	let { id, onSend, forceOpen = false }: Props = $props();
	const step = $derived<GettingStartedStepId | null>(gsCardStep(id));

	function scrollToStep(s: GettingStartedStepId) {
		document.querySelector(`[data-gs-step="${s}"]`)?.scrollIntoView({ behavior: "smooth", block: "start" });
	}
</script>

{#if id === GS_MAST_ID}
	<GettingStartedMast onOpenStep={scrollToStep} />
{:else if id === GS_PROMISE_ID}
	<PromiseLine />
{:else if id === GS_FIRST_LINE_ID}
	<p class="first-line">{FIRST_LINE}</p>
{:else if step === "connect_ai"}
	<ConnectAiCard {forceOpen} />
{:else if step === "introductions"}
	<IntroductionsAsk {forceOpen} {onSend} disabled={!gettingStarted.aiConnected} />
{:else if step === "connect_world"}
	<ConnectWorldCard {forceOpen} />
{:else if step === "interview"}
	<InterviewCard {forceOpen} />
{/if}

<style>
	.first-line {
		margin: 0.5rem 0 0;
		color: var(--color-foreground);
	}
</style>
