<script lang="ts">
	/**
	 * What a Settings page, or one block on it, shows when your server
	 * didn't send what it needs: one sentence and Try again, left-aligned
	 * in the page's prose. ErrorState is the centered block a whole pane
	 * shows elsewhere in the app; Settings pages read as text, so they
	 * use this.
	 */
	import Button from '$lib/components/Button.svelte';

	let { message, onretry }: { message: string; onretry: () => void } = $props();
</script>

<div class="load-error" role="alert">
	<p>{message}</p>
	<Button variant="secondary" onclick={onretry}>Try again</Button>
</div>

<style>
	.load-error {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 8px;
		margin-top: 24px;
	}

	p {
		margin: 0;
		max-width: 60ch;
		font-size: 14px;
		line-height: 1.5;
		color: var(--color-error);
	}
</style>
