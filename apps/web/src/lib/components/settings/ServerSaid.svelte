<script lang="ts">
	/**
	 * Your server's own error text, quoted under a sentence that already
	 * says what went wrong. The sentence carries the state, so the quote
	 * stays in the page's ink.
	 */
	let { text }: { text: string } = $props();
</script>

<p class="said">Your server said:</p>
<code>{text}</code>

<style>
	.said {
		margin: 16px 0 0;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	code {
		display: block;
		margin-top: 4px;
		padding: 8px 12px;
		border-radius: var(--radius-md);
		background: var(--color-surface-elevated);
		font-family: var(--font-mono);
		font-size: 13px;
		line-height: 1.45;
		color: var(--color-foreground);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
</style>
