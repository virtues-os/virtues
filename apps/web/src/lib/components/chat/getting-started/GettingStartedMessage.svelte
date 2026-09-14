<!--
	One synthetic message of the getting-started room: its text, rendered
	exactly like any assistant turn, plus the controls for the step that is
	up now (the bottom ask) or for a step the model brought back (a
	`show_step` tool part). Nothing else: no card, no frame.
-->
<script lang="ts">
	import Markdown from "$lib/components/Markdown.svelte";
	import StepActions from "./StepActions.svelte";
	import ChapterLifeline from "$lib/components/chat/interview/ChapterLifeline.svelte";
	import ChapterLifelineLive from "$lib/components/chat/interview/ChapterLifelineLive.svelte";
	import { GS_PROMISE_ID, GS_INTERVIEW_OPENING_ID, gsNowStep } from "./getting-started";
	import type { GettingStartedStepId } from "$lib/api/client";

	interface Props {
		id: string;
		text: string;
	}
	let { id, text }: Props = $props();
	const step = $derived<GettingStartedStepId | null>(gsNowStep(id));
</script>

<div class="text-base text-foreground assistant-response">
	<Markdown content={text} isStreaming={false} />
	{#if id === GS_INTERVIEW_OPENING_ID}
		<!-- Under the interview's heading, wider than the column: one
		     fictional life on one wire. The table that follows lists it. -->
		<ChapterLifeline />
	{/if}
</div>
{#if id === "gs-done-interview"}
	<!-- The close answers the opening: the same plate, drawn from the
	     chapters the person named. -->
	<ChapterLifelineLive />
{/if}
{#if step}
	<StepActions {step} />
{:else if id === GS_PROMISE_ID}
	<StepActions step="promise" />
{/if}
