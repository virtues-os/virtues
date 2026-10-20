<script lang="ts">
	/**
	 * SentMessageCard
	 *
	 * What `send_imessage` sent, in the reply where the model sent it. Framed,
	 * because these are the owner's words, sent as them: a card is you
	 * answering (agents/build/design-grammar.md §2). The words are set in the
	 * serif at the size of what you kept; the line above says who and when.
	 * A failed send falls through to the chat's error branch instead.
	 */
	interface Props {
		/** Running while the Mac hands the message to Messages. */
		sending: boolean;
		/** The name or address it went to. */
		to: string;
		text: string;
		personId?: string | null;
		sentAt?: string;
		onOpenPerson?: (personId: string) => void;
	}

	let { sending, to, text, personId, sentAt, onOpenPerson }: Props = $props();

	const time = $derived.by(() => {
		if (!sentAt) return "";
		const at = new Date(sentAt);
		return Number.isNaN(at.getTime())
			? ""
			: at.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
	});
</script>

<figure class="sent-message" class:sending aria-live="polite">
	<figcaption>
		<span>
			{sending ? "Sending to" : "Sent to"}
			{#if personId && onOpenPerson && !sending}
				<button type="button" class="person" onclick={() => onOpenPerson(personId)}>{to}</button>
			{:else}
				{to}
			{/if}
		</span>
		{#if time && !sending}<time datetime={sentAt}>{time}</time>{/if}
	</figcaption>
	<blockquote>{text}</blockquote>
</figure>

<style>
	.sent-message {
		margin: 0.75rem 0;
		max-width: 40em;
		padding: 0.75rem 1rem 0.875rem;
		border: 1px solid var(--color-border);
		border-radius: 12px;
		background: var(--color-surface);
	}

	figcaption {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 1rem;
		font-family: var(--font-sans);
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	.person {
		font: inherit;
		color: var(--color-primary);
		background: none;
		border: none;
		padding: 0;
		cursor: pointer;
	}

	.person:hover {
		text-decoration: underline;
		text-underline-offset: 2px;
	}

	time {
		font-family: var(--font-mono);
		font-size: 12px;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground-subtle);
	}

	blockquote {
		margin: 0.375rem 0 0;
		font-family: var(--font-serif);
		font-size: 16px;
		line-height: 1.5;
		color: var(--color-foreground);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}

	.sending blockquote {
		color: var(--color-foreground-muted);
	}
</style>
