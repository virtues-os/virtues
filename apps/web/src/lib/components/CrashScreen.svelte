<!--
	The whole window, when a screen throws while rendering. The root layout's
	boundary draws it, so it replaces everything, sidebar included, and it is
	the only thing the person can see until they reload.

	That is why it is not `ErrorState`: that one is a block inside a pane that
	still has its chrome around it. Here there is no chrome, so the screen
	draws the window's materials itself: the desk, and one sheet of paper
	standing `--window-air` in from the edges, the way an empty pane would.
	On the paper: one serif line, one sentence, and the Reload button.
-->
<script lang="ts">
	import Button from "$lib/components/Button.svelte";
	import Card from "$lib/components/Card.svelte";
	import TextAction from "$lib/components/TextAction.svelte";
	import { reloadApp } from "$lib/reload";

	let { error }: { error: unknown } = $props();

	let reloading = $state(false);
	let showDetails = $state(false);
	let copied = $state(false);

	const details = $derived.by(() => {
		if (error instanceof Error) {
			const stack = error.stack?.split("\n").slice(0, 8).join("\n");
			return stack && stack.includes(error.message) ? stack : `${error.message}\n${stack ?? ""}`.trim();
		}
		return String(error);
	});

	function reload() {
		reloading = true;
		reloadApp();
	}

	async function copy() {
		try {
			await navigator.clipboard.writeText(details);
			copied = true;
			setTimeout(() => (copied = false), 1600);
		} catch {
			copied = false;
		}
	}
</script>

<main class="crash">
	<div class="paper">
		<div class="body">
			<h1>This screen couldn't open</h1>
			<p class="lede">Something on it failed while it was drawing. Reload to open it again. Your data is not affected.</p>

			<div class="actions">
				<Button icon="mynaui:refresh" onclick={reload} loading={reloading}>Reload</Button>
				<TextAction quiet onclick={() => (showDetails = !showDetails)}>
					{showDetails ? "Hide details" : "Show details"}
				</TextAction>
			</div>

			{#if showDetails}
				<Card class="details">
					<pre>{details}</pre>
					<div class="copy">
						<TextAction quiet onclick={copy}>{copied ? "Copied" : "Copy"}</TextAction>
					</div>
				</Card>
			{/if}
		</div>
	</div>
</main>

<style>
	.crash {
		min-height: 100vh;
		min-height: 100dvh;
		display: flex;
		padding: var(--window-air);
		background: var(--desk);
		color: var(--color-foreground);
	}
	.paper {
		flex: 1;
		display: grid;
		place-items: center;
		padding: 32px 16px;
		border-radius: var(--radius-xl);
		background: var(--color-page);
	}
	.body {
		width: min(100%, 32rem);
		display: flex;
		flex-direction: column;
		align-items: center;
		text-align: center;
		animation: rise 0.5s cubic-bezier(0.2, 0.7, 0.2, 1) both;
	}

	h1 {
		margin: 0;
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: var(--text-title);
		line-height: 1.15;
		text-wrap: balance;
	}
	.lede {
		margin: 12px 0 0;
		max-width: 26rem;
		font-family: var(--font-sans);
		font-size: var(--text-body);
		line-height: 1.5;
		color: var(--color-foreground-muted);
		text-wrap: pretty;
	}
	.actions {
		margin-top: 24px;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 12px;
	}

	.body :global(.details) {
		margin-top: 20px;
		width: 100%;
		text-align: left;
		animation: rise 0.3s ease both;
	}
	pre {
		margin: 0;
		max-height: 12rem;
		overflow: auto;
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		line-height: 1.5;
		color: var(--color-foreground-muted);
		white-space: pre-wrap;
		word-break: break-word;
	}
	.copy {
		margin-top: 8px;
		display: flex;
		justify-content: flex-end;
	}

	@keyframes rise {
		from {
			opacity: 0;
			transform: translateY(8px);
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.body,
		.body :global(.details) {
			animation: none;
		}
	}
</style>
