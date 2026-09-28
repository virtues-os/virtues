<script lang="ts">
	/**
	 * The card at the top of every local chat: what this mode is, what it gets
	 * wrong, and why. Local mode is a demonstration of where fully local AI is
	 * today, and this card is where it says so. See
	 * agents/plan/local-model-plan.md for the conversations behind the words.
	 *
	 * It also holds the one-time download, before the model is on the server,
	 * and the "Think first" switch. The switch lives here rather than in the
	 * shared composer so the composer grows no control for one mode.
	 */
	import Card from '$lib/components/Card.svelte';
	import Button from '$lib/components/Button.svelte';
	import { localModel } from '$lib/stores/localModel.svelte';

	let { think = $bindable(false) }: { think?: boolean } = $props();

	const status = $derived(localModel.status);
	const gb = (bytes: number) => (bytes / 1e9).toFixed(1);
	const mb = (bytes: number) => Math.round(bytes / 1e6);
</script>

<Card class="local-card">
	<h2 class="title">A small model, on your server</h2>
	<p>
		This is Qwen3 0.6B, running on your server's NPU, the chip in it built for running models. Nothing you
		type here leaves your server.
	</p>
	<p>
		It's here to show where fully local AI is today. It's slow, it invents facts, and it gets simple things
		wrong. Don't rely on it for health, money, or personal decisions.
	</p>
	<p>
		The models in Chat run in data centers and are far larger. Fitting that quality into a server like this
		is one of the most active problems in AI right now.
	</p>

	{#if !status.ready}
		<div class="download">
			{#if status.downloading}
				<p class="note" aria-live="polite">
					Downloading the local model · {mb(status.downloadedBytes)} MB of {gb(status.totalBytes)} GB
				</p>
			{:else}
				<p class="note">
					The local model is {gb(status.totalBytes)} GB and downloads once to your server. After that, it runs
					without an internet connection.
				</p>
				{#if status.error}
					<p class="note error">{status.error}</p>
				{/if}
				<Button size="sm" onclick={() => localModel.startDownload()}>
					{status.error ? 'Try again' : 'Download the local model'}
				</Button>
			{/if}
		</div>
	{:else}
		<div class="think-row">
			<div>
				<div class="think-label">Think first</div>
				<div class="note">Slower, and usually better on questions with steps</div>
			</div>
			<button
				class="switch"
				class:on={think}
				role="switch"
				aria-checked={think}
				aria-label="Think first"
				onclick={() => (think = !think)}
			></button>
		</div>
	{/if}
</Card>

<style>
	:global(.local-card) {
		margin-bottom: 24px;
	}
	.title {
		font-size: 15px;
		font-weight: 600;
		color: var(--color-foreground);
		margin: 0 0 8px;
	}
	p {
		font-size: 14px;
		line-height: 1.55;
		color: var(--color-foreground-muted);
		margin: 0 0 8px;
	}
	.download {
		margin-top: 16px;
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 8px;
	}
	.note {
		font-size: 12px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
		margin: 0;
		font-variant-numeric: tabular-nums;
	}
	.note.error {
		color: var(--color-error);
	}
	.think-row {
		margin-top: 16px;
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
	}
	.think-label {
		font-size: 14px;
		color: var(--color-foreground);
	}
	.switch {
		width: 44px;
		height: 26px;
		border-radius: 13px;
		background: var(--color-border);
		position: relative;
		flex: none;
		border: 0;
		cursor: pointer;
		transition: background 150ms ease;
		padding: 0;
	}
	.switch.on {
		background: var(--color-success);
	}
	.switch::after {
		content: '';
		position: absolute;
		top: 3px;
		left: 3px;
		width: 20px;
		height: 20px;
		border-radius: 50%;
		background: var(--color-background);
		transition: transform 150ms ease;
	}
	.switch.on::after {
		transform: translateX(18px);
	}
	.switch:focus-visible {
		outline: 2px solid var(--color-foreground);
		outline-offset: 3px;
	}
	@media (prefers-reduced-motion: reduce) {
		.switch,
		.switch::after {
			transition: none;
		}
	}
</style>
