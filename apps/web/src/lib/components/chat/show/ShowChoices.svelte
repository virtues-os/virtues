<!--
	A next step under the reply: the question, why it comes up now, and one to
	three buttons, the first filled. A tap sends the button's words as your
	next message, exactly as if you had written them.

	The card shows only on the newest finished turn. It goes away when you tap
	a button or close it, and your next message then shows what you chose, so
	an older turn draws nothing here.

	It sits on the resting fill a sent message uses (design.md, "Grays"), so the
	card and your own messages share one gray. The ∴ leads the context line and
	marks the step to take.
-->
<script lang="ts">
	import { slide } from "svelte/transition";
	import Button from "$lib/components/Button.svelte";
	import IconButton from "$lib/components/IconButton.svelte";

	interface Props {
		/** The question the buttons answer. */
		title?: string | null;
		/** Why this step comes up now. */
		context?: string | null;
		options: string[];
		active: boolean;
		onChoose?: (text: string) => Promise<boolean>;
	}
	let { title, context, options, active, onChoose }: Props = $props();

	let chosen = $state<string | null>(null);
	let closed = $state(false);

	async function choose(text: string) {
		if (!active || chosen || !onChoose) return;
		// Held while it sends, so a second tap cannot send twice; let go if
		// the chat could not take it, so the card comes back.
		chosen = text;
		if (!(await onChoose(text))) chosen = null;
	}
</script>

{#if active && !closed && !chosen && options.length > 0}
	<aside class="next" aria-label="Next step" transition:slide={{ duration: 220 }}>
		<div class="close">
			<IconButton icon="mynaui:x" label="Close" size="nested" onclick={() => (closed = true)} />
		</div>
		{#if title}
			<p class="question">{title}</p>
		{/if}
		{#if context}
			<p class="context">
				<svg class="therefore" width="10" height="10" viewBox="0 0 12 12" aria-hidden="true">
					<circle cx="6" cy="2.5" r="1.7" />
					<circle cx="2.4" cy="9.2" r="1.7" />
					<circle cx="9.6" cy="9.2" r="1.7" />
				</svg>{context}
			</p>
		{/if}
		<div class="actions">
			{#each options as option, i}
				<Button variant={i === 0 ? "primary" : "secondary"} onclick={() => choose(option)}>{option}</Button>
			{/each}
		</div>
	</aside>
{/if}

<style>
	.next {
		position: relative;
		margin: 1rem 0 0.5rem;
		padding: 16px 18px 14px;
		border-radius: var(--radius-lg);
		background: var(--fill-bg);
	}
	.close {
		position: absolute;
		top: 10px;
		right: 10px;
	}
	.question {
		margin: 0;
		padding-right: 2rem;
		font-family: var(--font-serif);
		font-size: var(--text-lead);
		line-height: 1.35;
		color: var(--color-foreground);
	}
	.context {
		margin: 6px 0 0;
		padding-right: 2rem;
		font-family: var(--font-sans);
		font-size: var(--text-small);
		line-height: 1.5;
		color: var(--color-foreground-subtle);
	}
	.therefore {
		display: inline-block;
		vertical-align: -0.05em;
		margin-right: 7px;
		fill: var(--color-primary);
	}
	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
		margin-top: 12px;
	}
</style>
