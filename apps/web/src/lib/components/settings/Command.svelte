<script lang="ts">
	/**
	 * A command to run on your server, with an outlined Copy button beside it.
	 * Settings shows a command where the work has to happen in a terminal on
	 * the server: backup keys, drives, restoring, exporting, upgrading by hand.
	 * A long command scrolls inside its box rather than wrapping, so it can be
	 * read as one line.
	 */
	import Button from '$lib/components/Button.svelte';

	let { command }: { command: string } = $props();

	let copied = $state(false);
	let timer: ReturnType<typeof setTimeout> | undefined;

	async function copy() {
		try {
			await navigator.clipboard.writeText(command);
		} catch {
			return;
		}
		copied = true;
		clearTimeout(timer);
		timer = setTimeout(() => (copied = false), 1600);
	}
</script>

<div class="command">
	<code>{command}</code>
	<Button variant="secondary" onclick={copy}>{copied ? 'Copied' : 'Copy'}</Button>
</div>

<style>
	.command {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		min-width: 0;
	}

	code {
		flex: 1;
		min-width: 0;
		padding: 8px 12px;
		border-radius: var(--radius-md);
		background: var(--color-surface-elevated);
		font-family: var(--font-mono);
		font-size: 13px;
		line-height: 1.4;
		color: var(--color-foreground);
		white-space: nowrap;
		overflow-x: auto;
	}
</style>
