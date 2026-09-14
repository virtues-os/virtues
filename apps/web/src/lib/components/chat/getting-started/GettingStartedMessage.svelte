<!--
	One synthetic message of the getting-started room: its text, rendered
	exactly like any assistant turn, plus the controls for the step that is
	up now (the bottom ask) or for a step the model brought back (a
	`show_step` tool part). Nothing else: no card, no frame.
-->
<script lang="ts">
	import Markdown from "$lib/components/Markdown.svelte";
	import StepActions from "./StepActions.svelte";
	import { GS_PROMISE_ID, gsNowStep } from "./getting-started";
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
</div>
{#if step}
	<StepActions {step} />
{:else if id === GS_PROMISE_ID}
	<StepActions step="promise" />
{/if}
