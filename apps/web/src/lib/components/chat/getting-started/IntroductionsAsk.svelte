<!--
	Step 2 — Introductions, as one prompt. A single textarea; the text goes
	to the room as an ordinary turn, the model plays the facts back as a
	confirmation card (IntroductionsConfirmCard), and that card writes.
	The clause about the interview keeps this to the facts.
-->
<script lang="ts">
	import StepCard from "./StepCard.svelte";

	interface Props {
		forceOpen?: boolean;
		/** Send the text as the person's turn in this room. */
		onSend: (text: string) => void;
		/** No model yet: the ask waits. */
		disabled?: boolean;
	}
	let { forceOpen = false, onSend, disabled = false }: Props = $props();
	let text = $state("");

	function send() {
		const t = text.trim();
		if (!t || disabled) return;
		onSend(t);
		text = "";
	}
</script>

<StepCard
	step="introductions"
	title="Introductions"
	what="What you like to be called, what you will call it, where home is, and when you were born."
	skippable
	{forceOpen}
>
	<p class="hint">
		In your own words, all at once is fine. The story of your life comes later, in its own conversation; this is just the facts the record cannot supply.
	</p>
	<textarea
		class="ask"
		rows="3"
		bind:value={text}
		{disabled}
		placeholder="Call me Nick. I'll call it Ari. Home is Austin, and I was born on the 2nd of April, 1997."
		onkeydown={(e) => {
			if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) send();
		}}
	></textarea>
	<div class="row">
		<span class="note">{disabled ? "Once AI is connected." : "⌘↩ to send"}</span>
		<button type="button" class="send" onclick={send} disabled={disabled || !text.trim()}>Send</button>
	</div>
</StepCard>

<style>
	.hint {
		margin: 0 0 0.625rem;
		font-size: 0.875rem;
		color: var(--color-foreground-muted);
	}
	.ask {
		width: 100%;
		resize: vertical;
		font: inherit;
		padding: 0.6rem 0.75rem;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: var(--color-background);
		color: var(--color-foreground);
	}
	.ask:disabled {
		opacity: 0.5;
	}
	.row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-top: 0.5rem;
	}
	.note {
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
	}
	.send {
		font: inherit;
		font-size: 0.875rem;
		padding: 0.35rem 0.9rem;
		border-radius: 6px;
		border: 1px solid var(--color-foreground);
		background: var(--color-foreground);
		color: var(--color-background);
		cursor: pointer;
	}
	.send:disabled {
		opacity: 0.4;
		cursor: default;
	}
</style>
