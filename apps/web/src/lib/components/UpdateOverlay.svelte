<script lang="ts">
	/**
	 * Update Overlay
	 *
	 * Full-screen overlay shown during a system update (container restart).
	 * Polls /health every 2-3 seconds until the server responds, then refreshes.
	 */
	import Icon from "$lib/components/Icon.svelte";
	import { onMount, onDestroy } from "svelte";

	let status = $state<"updating" | "reconnecting" | "error">("updating");
	let pollCount = $state(0);
	let pollInterval: ReturnType<typeof setInterval> | null = null;

	const MAX_POLLS = 90; // ~3 minutes at 2s intervals
	const POLL_INTERVAL_MS = 2000;
	// Wait a few seconds before starting to poll (let the old container shut down)
	const INITIAL_DELAY_MS = 5000;

	onMount(() => {
		setTimeout(() => {
			status = "reconnecting";
			pollInterval = setInterval(async () => {
				try {
					const res = await fetch("/health");
					if (res.ok) {
						// Server is back — refresh the page to load the new version
						if (pollInterval) {
							clearInterval(pollInterval);
							pollInterval = null;
						}
						window.location.reload();
						return;
					}
				} catch {
					// Expected — server is restarting
				}

				pollCount++;
				if (pollCount >= MAX_POLLS) {
					status = "error";
					if (pollInterval) {
						clearInterval(pollInterval);
						pollInterval = null;
					}
				}
			}, POLL_INTERVAL_MS);
		}, INITIAL_DELAY_MS);
	});

	onDestroy(() => {
		if (pollInterval) {
			clearInterval(pollInterval);
		}
	});

	function getStatusMessage(): string {
		switch (status) {
			case "updating":
				return "Updating Virtues...";
			case "reconnecting":
				return "Reconnecting...";
			case "error":
				return "Update is taking longer than expected";
			default:
				return "Please wait...";
		}
	}

	function getSubMessage(): string {
		switch (status) {
			case "updating":
				return "Preparing the new version. This usually takes about 30 seconds.";
			case "reconnecting":
				return "Waiting for the new version to come online...";
			case "error":
				return "The server may still be starting up. Try refreshing the page.";
			default:
				return "";
		}
	}
</script>

<div
	class="update-overlay"
	role="dialog"
	aria-modal="true"
	aria-labelledby="update-title"
>
	<div class="overlay-backdrop"></div>
	<div class="overlay-card">
		{#if status === "error"}
			<div class="icon-container error">
				<Icon icon="ri:error-warning-line" width="48" />
			</div>
		{:else}
			<div class="spinner-container">
				<div class="spinner"></div>
			</div>
		{/if}

		<h2 id="update-title">{getStatusMessage()}</h2>
		<p class="sub-message">{getSubMessage()}</p>

		{#if status !== "error"}
			<div class="progress-dots">
				<span class="dot" class:active={pollCount % 3 === 0}></span>
				<span class="dot" class:active={pollCount % 3 === 1}></span>
				<span class="dot" class:active={pollCount % 3 === 2}></span>
			</div>
		{:else}
			<div class="error-actions">
				<button
					class="retry-button"
					onclick={() => window.location.reload()}
				>
					Refresh Page
				</button>
			</div>
		{/if}
	</div>
</div>

<style>
	.update-overlay {
		position: fixed;
		inset: 0;
		z-index: 9999;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.overlay-backdrop {
		position: absolute;
		inset: 0;
		background: color-mix(
			in srgb,
			var(--color-background, #0a0a0a) 95%,
			transparent
		);
		backdrop-filter: blur(8px);
	}

	.overlay-card {
		position: relative;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 1rem;
		padding: 3rem 4rem;
		background: var(--color-surface, #1a1a1a);
		border: 1px solid var(--color-border, #333);
		border-radius: 1rem;
		box-shadow: 0 25px 50px -12px rgba(0, 0, 0, 0.5);
		text-align: center;
		max-width: 400px;
	}

	.spinner-container {
		width: 64px;
		height: 64px;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.spinner {
		width: 48px;
		height: 48px;
		border: 3px solid var(--color-border, #333);
		border-top-color: var(--color-primary, #3b82f6);
		border-radius: 50%;
		animation: spin 1s linear infinite;
	}

	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}

	.icon-container {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 64px;
		height: 64px;
	}

	.icon-container.error {
		color: var(--color-warning, #f59e0b);
	}

	h2 {
		margin: 0;
		font-size: 1.25rem;
		font-weight: 600;
		color: var(--color-foreground, #fff);
	}

	.sub-message {
		margin: 0;
		font-size: 0.875rem;
		color: var(--color-foreground-muted, #888);
		max-width: 280px;
	}

	.progress-dots {
		display: flex;
		gap: 0.5rem;
		margin-top: 0.5rem;
	}

	.dot {
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--color-border, #333);
		transition: background 0.3s ease;
	}

	.dot.active {
		background: var(--color-primary, #3b82f6);
	}

	.error-actions {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		margin-top: 0.5rem;
	}

	.retry-button {
		padding: 0.625rem 1.5rem;
		background: var(--color-primary, #3b82f6);
		color: white;
		border: none;
		border-radius: 0.5rem;
		font-size: 0.875rem;
		font-weight: 500;
		cursor: pointer;
		transition: opacity 0.15s ease;
	}

	.retry-button:hover {
		opacity: 0.9;
	}

	@media (prefers-reduced-motion: reduce) {
		.spinner {
			animation: none;
		}
		.dot {
			transition: none;
		}
	}
</style>
