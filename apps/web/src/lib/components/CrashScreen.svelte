<!--
	The whole window, when a screen throws while rendering. The root layout's
	boundary draws it, so it replaces everything, sidebar included, and it is
	the only thing the person can see until they reload.

	That is why it is not `ErrorState`: that one is a block inside a pane that
	still has its chrome around it. Here there is no chrome, so the screen has
	to stand on its own as a page: a watercolor plate, one serif line, one
	sentence, and one button.

	The plate is a writing desk under an open window, its pages blown out
	into the air. Like the getting-started room's cover (`RoomCover`), the
	paper is knocked out to alpha so the wash sits on whatever the page is,
	and a dark theme lays a paper card under it through `--identity-dark`.
-->
<script lang="ts">
	import Button from "$lib/components/Button.svelte";
	import { reloadApp } from "$lib/reload";

	let { error }: { error: unknown } = $props();

	let painted = $state(false);
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
	<div class="sheet">
		<figure class="plate" class:painted>
			<img
				src="/covers/crash.png"
				alt="A watercolor of a writing desk under an open window, its pages blown up into the air"
				width="1300"
				height="529"
				decoding="async"
				onload={() => (painted = true)}
			/>
		</figure>

		<h1>This screen couldn't open</h1>
		<p class="lede">
			Something on it failed while it was drawing. Reload to open it again. Your data on your server is not
			affected.
		</p>

		<div class="actions">
			<Button icon="ri:refresh-line" onclick={reload} loading={reloading}>Reload</Button>
			<button class="link" type="button" aria-expanded={showDetails} onclick={() => (showDetails = !showDetails)}>
				{showDetails ? "Hide details" : "Show details"}
			</button>
		</div>

		{#if showDetails}
			<div class="details">
				<pre>{details}</pre>
				<button class="link copy" type="button" onclick={copy}>{copied ? "Copied" : "Copy"}</button>
			</div>
		{/if}
	</div>
</main>

<style>
	.crash {
		min-height: 100vh;
		min-height: 100dvh;
		display: grid;
		place-items: center;
		padding: 2rem 1rem;
		background: var(--background);
		color: var(--foreground);
	}
	.sheet {
		width: min(100%, 34rem);
		display: flex;
		flex-direction: column;
		align-items: center;
		text-align: center;
		animation: rise 0.7s cubic-bezier(0.2, 0.7, 0.2, 1) both;
	}

	/* The plate. The paper card exists only where `--identity-dark` is 1. */
	.plate {
		--plate-dark: var(--identity-dark, 0);
		--plate-paper: #f3efe6;
		width: 100%;
		margin: 0 0 1.75rem;
		padding: calc(var(--plate-dark) * 14px);
		border-radius: calc(var(--plate-dark) * 12px);
		background: color-mix(in srgb, var(--plate-paper) calc(var(--plate-dark) * 100%), transparent);
		opacity: 0;
		transition: opacity 0.8s ease;
	}
	.plate.painted {
		opacity: 1;
	}
	.plate img {
		display: block;
		width: 100%;
		height: auto;
		aspect-ratio: 1300 / 529;
	}

	@keyframes rise {
		from {
			opacity: 0;
			transform: translateY(8px);
		}
	}

	/* The words */
	h1 {
		margin: 0;
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: clamp(1.75rem, 4vw, 2.25rem);
		line-height: 1.15;
		letter-spacing: -0.01em;
		text-wrap: balance;
	}
	.lede {
		margin: 0.875rem 0 0;
		font-family: var(--font-sans);
		font-size: 0.9375rem;
		line-height: 1.55;
		color: var(--foreground-muted);
		text-wrap: pretty;
		max-width: 26rem;
	}
	.actions {
		margin-top: 1.75rem;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.875rem;
	}
	.link {
		background: none;
		border: none;
		padding: 0;
		font-family: var(--font-sans);
		font-size: 0.8125rem;
		color: var(--foreground-subtle);
		cursor: pointer;
		text-underline-offset: 3px;
	}
	.link:hover {
		color: var(--foreground);
		text-decoration: underline;
	}
	.link:focus-visible {
		outline: 2px solid var(--border-focus);
		outline-offset: 3px;
		border-radius: var(--radius-xs);
	}
	.details {
		position: relative;
		margin-top: 1.25rem;
		width: 100%;
		text-align: left;
		border: 1px solid var(--border);
		border-radius: var(--radius-xl);
		background: var(--surface);
		animation: rise 0.3s ease both;
	}
	.details pre {
		margin: 0;
		padding: 0.875rem 4rem 0.875rem 1rem;
		max-height: 12rem;
		overflow: auto;
		font-family: var(--font-sans);
		font-size: 0.75rem;
		line-height: 1.5;
		color: var(--foreground-muted);
		white-space: pre-wrap;
		word-break: break-word;
	}
	.copy {
		position: absolute;
		top: 0.625rem;
		right: 0.75rem;
		font-size: 0.75rem;
		background: var(--surface);
		padding: 0 0.25rem;
	}

	@media (prefers-reduced-motion: reduce) {
		.sheet,
		.details {
			animation: none;
		}
		.plate {
			transition: none;
		}
	}
</style>
